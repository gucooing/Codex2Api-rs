//! A handshake's routing cookies, frozen at their original absolute expiry.
use codex2api_common::version::ROUTING_COOKIE_NAMES;
use codex2api_storage::{RoutingKey, Storage};
use cookie_store::{Cookie, CookieStore};
use http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatgptRoutingPin {
    connection_id: String,
    account_id: String,
    auth_revision: i64,
    endpoint: String,
    cookies: Vec<Cookie<'static>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::cookie::CookieStore as _;

    fn endpoint() -> Url {
        Url::parse("https://chatgpt.com/backend-api/codex/responses").unwrap()
    }

    fn pin(value: &str) -> ChatgptRoutingPin {
        let headers = HeaderMap::from_iter([(
            http::header::SET_COOKIE,
            HeaderValue::from_str(value).unwrap(),
        )]);
        ChatgptRoutingPin::capture("supplier", 7, &endpoint(), &headers).unwrap()
    }

    #[test]
    fn parent_pin_survives_other_traffic_and_rejects_other_scopes() {
        let endpoint = endpoint();
        let pin = pin("__oailb=parent; Path=/backend-api; Secure; Max-Age=600");
        let shared = codex2api_suppliers::transport::AccountCookieStore::default();
        let unrelated = HeaderValue::from_static("__oailb=other; Path=/; Secure");
        let challenge = HeaderValue::from_static("__cf_bm=challenge; Path=/; Secure");
        shared.set_cookies(&mut [&unrelated, &challenge].into_iter(), &endpoint);
        let current = shared.cookies(&endpoint).unwrap();
        let cookies = pin
            .merge_cookie_header(&endpoint, "supplier", 7, Some(&current))
            .unwrap();
        assert!(cookies.to_str().unwrap().contains("__oailb=parent"));
        assert!(cookies.to_str().unwrap().contains("__cf_bm=challenge"));
        assert!(!cookies.to_str().unwrap().contains("other"));
        assert!(cookies.is_sensitive());
        for url in [
            "http://chatgpt.com/backend-api/codex/responses",
            "https://us.chatgpt.com/backend-api/codex/responses",
            "https://chatgpt.com:8443/backend-api/codex/responses",
            "https://chatgpt.com/backend-api/codex/guardian",
        ] {
            assert!(
                pin.merge_cookie_header(&Url::parse(url).unwrap(), "supplier", 7, None)
                    .is_none()
            );
        }
        assert!(
            pin.merge_cookie_header(&endpoint, "another", 7, None)
                .is_none()
        );
        assert!(
            pin.merge_cookie_header(&endpoint, "supplier", 8, None)
                .is_none()
        );
    }

    #[tokio::test]
    async fn persisted_max_age_is_not_restarted_when_the_parent_is_loaded() {
        let pin = pin("__cflb=parent; Path=/; Secure; Max-Age=1");
        let encoded = serde_json::to_string(&pin).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
        let loaded: ChatgptRoutingPin = serde_json::from_str(&encoded).unwrap();
        assert_eq!(loaded.connection_id(), pin.connection_id());
        assert!(
            loaded
                .merge_cookie_header(&endpoint(), "supplier", 7, None)
                .is_none()
        );
    }

    #[test]
    fn ambiguous_or_oversized_response_cookies_cannot_create_a_pin() {
        let mut headers = HeaderMap::new();
        headers.append(
            "set-cookie",
            HeaderValue::from_static("__oailb=one; Path=/; Secure"),
        );
        headers.append(
            "set-cookie",
            HeaderValue::from_static("__oailb=two; Path=/; Secure"),
        );
        assert!(ChatgptRoutingPin::capture("supplier", 7, &endpoint(), &headers).is_none());
        headers.clear();
        headers.insert(
            "set-cookie",
            HeaderValue::from_str(&format!("__oailb={}; Path=/; Secure", "x".repeat(8192)))
                .unwrap(),
        );
        assert!(ChatgptRoutingPin::capture("supplier", 7, &endpoint(), &headers).is_none());
        headers.insert(
            "set-cookie",
            HeaderValue::from_static("chatgpt_session=never-pin; Path=/; Secure"),
        );
        assert!(ChatgptRoutingPin::capture("supplier", 7, &endpoint(), &headers).is_none());
    }

    #[tokio::test]
    async fn redis_pins_bind_exact_response_consumer_supplier_and_login() {
        let directory = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(directory.path().join("routing-pin"))
            .await
            .unwrap();
        let parent = RoutingKey::response("consumer", "chatgpt", "response-one");
        let other = RoutingKey::response("consumer", "chatgpt", "response-two");
        let first = pin("__oailb=first; Path=/; Secure; Max-Age=600");
        let second = pin("__oailb=second; Path=/; Secure; Max-Age=600");
        first.remember(&storage, &parent).await.unwrap();
        let (a, b) = tokio::join!(
            second.remember(&storage, &parent),
            second.remember(&storage, &other)
        );
        a.unwrap();
        b.unwrap();
        let loaded = ChatgptRoutingPin::load(&storage, &parent, "supplier", 7)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.connection_id(), first.connection_id());
        let loaded_other = ChatgptRoutingPin::load(&storage, &other, "supplier", 7)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded_other.connection_id(), second.connection_id());
        for (key, supplier, revision) in [
            (
                RoutingKey::response("another", "chatgpt", "response-one"),
                "supplier",
                7,
            ),
            (
                RoutingKey::response("consumer", "api", "response-one"),
                "supplier",
                7,
            ),
            (parent.clone(), "another", 7),
            (parent.clone(), "supplier", 8),
        ] {
            assert!(
                ChatgptRoutingPin::load(&storage, &key, supplier, revision)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        storage
            .remember_response_owner(parent.clone(), "another".into())
            .await
            .unwrap();
        assert!(
            ChatgptRoutingPin::load(&storage, &parent, "supplier", 7)
                .await
                .unwrap()
                .is_none()
        );
        storage.close().await;
    }
}

impl ChatgptRoutingPin {
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub(crate) fn capture(
        account_id: &str,
        auth_revision: i64,
        endpoint: &Url,
        headers: &HeaderMap,
    ) -> Option<Self> {
        if !codex2api_suppliers::transport::chatgpt_cookie_url_allowed(endpoint) {
            return None;
        }
        let mut jar = CookieStore::default();
        for name in ROUTING_COOKIE_NAMES {
            let mut matches = headers
                .get_all(http::header::SET_COOKIE)
                .iter()
                .filter(|header| {
                    header
                        .to_str()
                        .ok()
                        .and_then(|v| v.split_once('='))
                        .is_some_and(|(key, _)| key.trim() == name)
                });
            if let Some(cookie) = matches.next() {
                if cookie.as_bytes().len() > 8192 || matches.next().is_some() {
                    return None;
                }
                if let Ok(cookie) = cookie.to_str() {
                    let _ = jar.parse(cookie, endpoint);
                }
            }
        }
        if jar.get_request_values(endpoint).next().is_none() {
            return None;
        }
        Some(Self {
            connection_id: uuid::Uuid::new_v4().to_string(),
            account_id: account_id.into(),
            auth_revision,
            endpoint: endpoint.as_str().into(),
            cookies: jar.iter_unexpired().cloned().collect(),
        })
    }

    pub(crate) fn merge_cookie_header(
        &self,
        endpoint: &Url,
        account_id: &str,
        auth_revision: i64,
        existing: Option<&HeaderValue>,
    ) -> Option<HeaderValue> {
        let original = Url::parse(&self.endpoint).ok()?;
        if self.account_id != account_id
            || self.auth_revision != auth_revision
            || !codex2api_suppliers::transport::chatgpt_cookie_url_allowed(endpoint)
            || endpoint.origin() != original.origin()
            || endpoint.path() != original.path()
        {
            return None;
        }
        let mut jar = CookieStore::default();
        for cookie in &self.cookies {
            if ROUTING_COOKIE_NAMES.contains(&cookie.name()) {
                // Insert the already-parsed cookie. Re-parsing Max-Age would extend its lifetime.
                let _ = jar.insert(cookie.clone(), endpoint);
            }
        }
        let mut pins: Vec<_> = jar
            .get_request_values(endpoint)
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        if pins.is_empty() {
            return None;
        }
        let mut values: Vec<_> = existing
            .and_then(|v| v.to_str().ok())
            .into_iter()
            .flat_map(|value| value.split(';'))
            .map(str::trim)
            .filter(|part| {
                !part.is_empty()
                    && !part
                        .split_once('=')
                        .is_some_and(|(name, _)| ROUTING_COOKIE_NAMES.contains(&name.trim()))
            })
            .map(str::to_owned)
            .collect();
        values.append(&mut pins);
        let mut header = HeaderValue::from_str(&values.join("; ")).ok()?;
        header.set_sensitive(true);
        Some(header)
    }

    pub async fn load(
        storage: &Storage,
        owner: &RoutingKey,
        supplier: &str,
        revision: i64,
    ) -> crate::Result<Option<Self>> {
        storage
            .chatgpt_response_pin(owner, supplier, revision)
            .await?
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| {
                    crate::UpstreamError::Internal("Invalid ChatGPT response routing state".into())
                })
            })
            .transpose()
    }

    pub(crate) async fn remember(
        &self,
        storage: &Storage,
        owner: &RoutingKey,
    ) -> crate::Result<()> {
        storage
            .remember_chatgpt_response_pin(
                owner,
                &self.account_id,
                self.auth_revision,
                &serde_json::to_string(self)?,
            )
            .await?;
        Ok(())
    }
}
