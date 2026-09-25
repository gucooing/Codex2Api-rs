//! Supplier-owned workspace routing discovered from the official accounts response.
use std::{sync::Arc, time::Duration};

use chrono::Utc;
use codex2api_auth::{AuthService, transport::AccountHttpClients};
use codex2api_storage::QuotaSnapshot;
use http::{HeaderMap, HeaderValue};
use serde::Serialize;
use serde_json::Value;

use crate::{Result, UpstreamClient, UpstreamError, client::RequestAuth};

const ROUTING_HEADER: &str = "x-openai-account-routing-override";
const DISCOVERY_URL: &str = "https://chatgpt.com/backend-api/wham/accounts/check";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkspaceRoute {
    pub account_id: String,
    pub backend_origin: String,
    pub account_routing_override: String,
}

impl WorkspaceRoute {
    pub fn from_accounts(value: &Value, account_id: &str) -> Result<Self> {
        let invalid = |message: &str| UpstreamError::WorkspaceRouting(message.into());
        let accounts = value
            .get("accounts")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("official response is missing workspace routing accounts"))?;
        let mut matching = accounts
            .iter()
            .filter(|entry| entry["id"].as_str() == Some(account_id));
        let entry = matching
            .next()
            .ok_or_else(|| invalid("selected supplier workspace is missing"))?;
        if matching.next().is_some() {
            return Err(invalid("selected supplier workspace is duplicated"));
        }
        let origin = entry["workspace_backend_origin"]
            .as_str()
            .ok_or_else(|| invalid("workspace backend origin is missing"))?;
        // Official discovery uses this sentinel for an unconstrained backend.
        // It keeps the configured bootstrap origin; it is not a malformed URL.
        let unconstrained = origin == "NO_CONSTRAINT";
        let url = reqwest::Url::parse(if unconstrained {
            codex2api_version::CHATGPT_BACKEND_BASE_URL
        } else {
            origin
        })
        .map_err(|_| invalid("workspace backend origin is invalid"))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || (!unconstrained && url.path() != "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || origin.trim() != origin
        {
            return Err(invalid(
                "workspace backend must be an HTTPS origin without credentials",
            ));
        }
        let constraint = entry["account_routing_override"]
            .as_str()
            .filter(|value| matches!(*value, "NO_CONSTRAINT" | "us" | "us_cr"))
            .ok_or_else(|| invalid("workspace routing constraint is missing or invalid"))?;
        Ok(Self {
            account_id: account_id.to_owned(),
            backend_origin: url.origin().ascii_serialization(),
            account_routing_override: constraint.to_owned(),
        })
    }

    pub(crate) fn apply(&self, request_url: &str, headers: &mut HeaderMap) -> Result<String> {
        let invalid = || UpstreamError::WorkspaceRouting("invalid request destination".into());
        let mut url = reqwest::Url::parse(request_url).map_err(|_| invalid())?;
        let origin = reqwest::Url::parse(&self.backend_origin).map_err(|_| invalid())?;
        // WSS and HTTPS use the same selected workspace origin and path.
        url.set_host(origin.host_str()).map_err(|_| invalid())?;
        url.set_port(origin.port()).map_err(|_| invalid())?;
        headers.remove(ROUTING_HEADER);
        if self.account_routing_override != "NO_CONSTRAINT" {
            headers.insert(
                ROUTING_HEADER,
                HeaderValue::from_str(&self.account_routing_override)?,
            );
        }
        Ok(url.into())
    }
}

pub(crate) fn is_workspace_endpoint(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("chatgpt.com")
            && (url.path() == "/backend-api/codex/responses"
                || url.path().starts_with("/backend-api/codex/responses/"))
    })
}

/// Captures the credentials, route and account HTTP context used by a WS handshake.
#[derive(Clone)]
pub struct WorkspaceConnection {
    auth: AuthService,
    supplier_id: String,
    revision: i64,
    route: WorkspaceRoute,
    http: Arc<AccountHttpClients>,
}

impl WorkspaceConnection {
    pub async fn check_current(&self) -> Result<()> {
        let storage = self.auth.storage()?;
        if storage.supplier_auth_revision(&self.supplier_id).await? != Some(self.revision) {
            return Err(UpstreamError::WorkspaceChanged);
        }
        let (snapshot, revision) = storage
            .supplier_routing_snapshot(&self.supplier_id)
            .await?
            .ok_or(UpstreamError::WorkspaceChanged)?;
        if revision != Some(self.revision)
            || WorkspaceRoute::from_accounts(&snapshot.value, &self.route.account_id)
                .ok()
                .as_ref()
                != Some(&self.route)
            || !Arc::ptr_eq(
                &self.auth.account_http(&self.supplier_id).await?,
                &self.http,
            )
        {
            return Err(UpstreamError::WorkspaceChanged);
        }
        Ok(())
    }
}

pub(crate) struct RoutedWorkspace {
    pub route: WorkspaceRoute,
    pub connection: Option<WorkspaceConnection>,
}

impl UpstreamClient {
    pub(crate) async fn workspace_route(
        &self,
        request: &RequestAuth,
        refresh: bool,
    ) -> Result<RoutedWorkspace> {
        let snapshot = self.workspace_snapshot(request, refresh).await?;
        let id = request
            .account_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                UpstreamError::WorkspaceRouting("supplier has no ChatGPT account ID".into())
            })?;
        let route = WorkspaceRoute::from_accounts(&snapshot.value, id)?;
        let connection = self.auth_service().map(|auth| WorkspaceConnection {
            auth: auth.clone(),
            supplier_id: self.identity().account_id.clone(),
            revision: request.revision,
            route: route.clone(),
            http: self.account_http.clone(),
        });
        Ok(RoutedWorkspace { route, connection })
    }

    /// Explicit administrator refresh uses the same persisted source as inference routing.
    pub async fn refresh_workspace_details(&self) -> Result<QuotaSnapshot> {
        self.synchronize_auth().await?;
        let mut retried = false;
        loop {
            let request = self.request_auth()?;
            match self.workspace_snapshot(&request, true).await {
                Err(error)
                    if error.is_unauthorized() && !retried && self.auth_service().is_some() =>
                {
                    retried = true;
                    self.refresh_access_token(&request.access_token).await?;
                }
                result => return result,
            }
        }
    }

    async fn workspace_snapshot(
        &self,
        request: &RequestAuth,
        refresh: bool,
    ) -> Result<QuotaSnapshot> {
        // Account reconfiguration preserves this lock; only snapshots live in SQLite.
        let _guard = self.account_http.routing_lock.lock().await;
        let storage = self.auth_service().map(AuthService::storage).transpose()?;
        let id = &self.identity().account_id;
        if let Some(storage) = storage {
            if storage.supplier_auth_revision(id).await? != Some(request.revision) {
                return Err(UpstreamError::WorkspaceChanged);
            }
            if !refresh
                && let Some((snapshot, revision)) = storage.supplier_routing_snapshot(id).await?
                && revision == Some(request.revision)
            {
                return Ok(snapshot);
            }
        }
        let mut headers = request.headers.clone();
        headers.remove("version");
        headers.remove("originator");
        let discovery_url = DISCOVERY_URL;
        #[cfg(test)]
        let discovery_url = self.discovery_url.as_deref().unwrap_or(discovery_url);
        let result = async {
            let response = self
                .account_http
                .api
                .get(discovery_url)
                .headers(headers)
                .timeout(Duration::from_secs(30))
                .send()
                .await?;
            let status = response.status();
            let body = response.bytes().await?;
            if !status.is_success() {
                // A recoverable discovery 401 must not disable a supplier before refresh succeeds.
                if status != reqwest::StatusCode::UNAUTHORIZED {
                    self.record_http_status(status.as_u16()).await;
                }
                return Err(UpstreamError::status(
                    status,
                    String::from_utf8_lossy(&body),
                ));
            }
            Ok(QuotaSnapshot {
                value: serde_json::from_slice(&body)?,
                observed_at: Utc::now(),
            })
        }
        .await;
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error @ (UpstreamError::Http(_) | UpstreamError::Json(_))) => {
                self.record_communication_error("ChatGPT 官方工作区路由读取失败")
                    .await;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if let Some(storage) = storage
            && !storage
                .store_supplier_routing_snapshot(id, request.revision, &snapshot)
                .await?
        {
            return Err(UpstreamError::WorkspaceChanged);
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_accounts::{AccountIdentity, HostRuntime, SupplierAccountStore};
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn accounts(origin: &str, constraint: &str) -> Value {
        json!({"accounts":[
            {"id":"other","workspace_backend_origin":"https://other.example","account_routing_override":"us_cr"},
            {"id":"selected","workspace_backend_origin":origin,"account_routing_override":constraint}
        ]})
    }

    #[test]
    fn routing_validates_identity_and_origin_without_changing_paths() {
        for constraint in ["NO_CONSTRAINT", "us", "us_cr"] {
            let route = WorkspaceRoute::from_accounts(
                &accounts("https://regional.example:8443", constraint),
                "selected",
            )
            .unwrap();
            for scheme in ["https", "wss"] {
                let mut headers = HeaderMap::from_iter([(
                    http::HeaderName::from_static(ROUTING_HEADER),
                    HeaderValue::from_static("caller"),
                )]);
                let actual = route
                    .apply(
                        &format!(
                            "{scheme}://chatgpt.com/backend-api/codex/responses/compact?keep=1"
                        ),
                        &mut headers,
                    )
                    .unwrap();
                assert_eq!(
                    actual,
                    format!(
                        "{scheme}://regional.example:8443/backend-api/codex/responses/compact?keep=1"
                    )
                );
                assert_eq!(
                    headers.get(ROUTING_HEADER).map(|v| v.to_str().unwrap()),
                    (constraint != "NO_CONSTRAINT").then_some(constraint)
                );
            }
        }
        for origin in [
            "",
            " NO_CONSTRAINT",
            "https://regional.example ",
            " https://regional.example",
            "http://regional.example",
            "https://user@regional.example",
            "https://regional.example/path",
            "https://regional.example?query=1",
            "https://regional.example/#fragment",
        ] {
            assert!(WorkspaceRoute::from_accounts(&accounts(origin, "us"), "selected").is_err());
        }
        for constraint in ["", "eu", "US", "caller"] {
            assert!(
                WorkspaceRoute::from_accounts(
                    &accounts("https://regional.example", constraint),
                    "selected"
                )
                .is_err()
            );
        }
        let mut value = accounts("https://regional.example", "us");
        assert!(WorkspaceRoute::from_accounts(&value, "absent").is_err());
        value["accounts"][1]
            .as_object_mut()
            .unwrap()
            .remove("workspace_backend_origin");
        assert!(WorkspaceRoute::from_accounts(&value, "selected").is_err());
        value["accounts"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other"}));
        assert!(WorkspaceRoute::from_accounts(&value, "other").is_err());
    }

    #[test]
    fn unconstrained_backend_preserves_official_destinations_and_routing_constraints() {
        for constraint in ["NO_CONSTRAINT", "us", "us_cr"] {
            let route =
                WorkspaceRoute::from_accounts(&accounts("NO_CONSTRAINT", constraint), "selected")
                    .unwrap();
            assert_eq!(route.backend_origin, "https://chatgpt.com");
            for endpoint in [
                crate::Endpoint::Responses,
                crate::Endpoint::Compact,
                crate::Endpoint::Guardian,
                crate::Endpoint::GuardianClassifier,
            ] {
                for scheme in ["https", "wss"] {
                    let url = endpoint.url().replacen("https", scheme, 1);
                    let mut headers = HeaderMap::new();
                    assert_eq!(route.apply(&url, &mut headers).unwrap(), url);
                    assert_eq!(
                        headers
                            .get(ROUTING_HEADER)
                            .map(|value| value.to_str().unwrap()),
                        (constraint != "NO_CONSTRAINT").then_some(constraint),
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn persisted_routing_rejects_stale_discovery_and_invalidates_existing_connections() {
        let path =
            std::env::temp_dir().join(format!("codex-routing-{}.sqlite", uuid::Uuid::new_v4()));
        let storage = codex2api_storage::Storage::open(&path).await.unwrap();
        let store = SupplierAccountStore::open(storage.clone());
        let account = store.create_pending().await.unwrap().account;
        let other = store.create_pending().await.unwrap().account;
        let mut credentials = codex2api_auth::AuthDotJson::chatgpt(
            codex2api_auth::TokenData {
                id_token: "id".into(),
                access_token: "access".into(),
                refresh_token: "refresh".into(),
                account_id: Some("selected".into()),
            },
            Some(Utc::now()),
        );
        store
            .save_auth_for_account(&account.id, &credentials)
            .await
            .unwrap();
        let revision = storage
            .supplier_auth_revision(&account.id)
            .await
            .unwrap()
            .unwrap();
        let mut snapshot = QuotaSnapshot {
            value: accounts("NO_CONSTRAINT", "us"),
            observed_at: Utc::now(),
        };
        assert!(
            storage
                .store_supplier_routing_snapshot(&account.id, revision, &snapshot)
                .await
                .unwrap()
        );
        assert!(
            storage
                .supplier_routing_snapshot(&other.id)
                .await
                .unwrap()
                .is_none()
        );
        let auth = AuthService::new(store.clone()).unwrap();
        let pool = crate::UpstreamPool::new(auth.clone());
        let client = pool.get(&account.id).await.unwrap();
        client.synchronize_auth().await.unwrap();
        let routing = client
            .workspace_route(&client.request_auth().unwrap(), false)
            .await
            .unwrap();
        let connection = routing.connection.unwrap();
        connection.check_current().await.unwrap();
        snapshot.value = accounts("https://regional-2.example", "us_cr");
        assert!(
            storage
                .store_supplier_routing_snapshot(&account.id, revision, &snapshot)
                .await
                .unwrap()
        );
        assert!(matches!(
            connection.check_current().await,
            Err(UpstreamError::WorkspaceChanged)
        ));
        let route = client
            .workspace_route(&client.request_auth().unwrap(), false)
            .await
            .unwrap();
        let connection = route.connection.unwrap();
        connection.check_current().await.unwrap();
        credentials.tokens.as_mut().unwrap().access_token = "renewed-access".into();
        store
            .save_auth_for_account(&account.id, &credentials)
            .await
            .unwrap();
        let new_revision = storage
            .supplier_auth_revision(&account.id)
            .await
            .unwrap()
            .unwrap();
        assert!(new_revision > revision);
        assert!(
            !storage
                .store_supplier_routing_snapshot(&account.id, revision, &snapshot)
                .await
                .unwrap()
        );
        assert!(matches!(
            connection.check_current().await,
            Err(UpstreamError::WorkspaceChanged)
        ));
        client.synchronize_auth().await.unwrap();
        assert_eq!(client.request_auth().unwrap().revision, new_revision);
        assert_eq!(
            client.request_auth().unwrap().access_token,
            "renewed-access"
        );
        assert!(
            storage
                .store_supplier_routing_snapshot(&account.id, new_revision, &snapshot)
                .await
                .unwrap()
        );
        // A fresh pool reads the persisted route rather than querying a provider.
        let reopened = crate::UpstreamPool::new(auth)
            .get(&account.id)
            .await
            .unwrap();
        reopened.synchronize_auth().await.unwrap();
        assert_eq!(
            reopened
                .workspace_route(&reopened.request_auth().unwrap(), false)
                .await
                .unwrap()
                .route
                .backend_origin,
            "https://regional-2.example"
        );
        storage.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn http_discovers_the_selected_workspace_routes_compact_and_rejects_redirects() {
        let (listener, address) = crate::proxy_fixture::listener().await;
        let origin = format!("https://{address}");
        let reply_accounts = accounts(&origin, "us").to_string();
        let tls = crate::proxy_fixture::acceptor();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let server = tokio::spawn(async move {
            for _ in 0..4 {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut stream = tls.accept(tcp).await.unwrap();
                let raw = crate::proxy_fixture::read_headers(&mut stream)
                    .await
                    .unwrap();
                let length = raw
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                let (status, extra, response) = if raw.starts_with("GET /accounts/check ") {
                    ("200 OK", "", reply_accounts.as_str())
                } else if raw.starts_with("POST /backend-api/codex/responses ") {
                    ("302 Found", "Location: /must-not-follow\r\n", "{}")
                } else {
                    assert!(raw.starts_with("POST /backend-api/codex/responses/compact "));
                    ("200 OK", "", "{}")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{extra}\r\n{response}",
                    response.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                tx.send((raw.to_ascii_lowercase(), body)).await.unwrap();
            }
        });
        let mut client = UpstreamClient::new(
            AccountIdentity::new("supplier", "install", HostRuntime::generate()),
            "upstream-token".into(),
            Some("selected".into()),
        )
        .unwrap();
        client.discovery_url = Some(format!("{origin}/accounts/check"));
        let ca = reqwest::Certificate::from_pem(include_bytes!(
            "../../codex2api-auth/tests/fixtures/proxy-ca.pem"
        ))
        .unwrap();
        let builder = || {
            codex2api_auth::transport::http_builder()
                .unwrap()
                .no_proxy()
                .add_root_certificate(ca.clone())
        };
        let http = Arc::get_mut(&mut client.account_http).unwrap();
        http.api = builder().build().unwrap();
        http.routed_api = builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-codex-guardian", HeaderValue::from_static("reviewer"));
        headers.insert(
            ROUTING_HEADER,
            HeaderValue::from_static("caller-controlled"),
        );
        headers.insert(
            "x-codex-routing-hint",
            HeaderValue::from_static("caller-hint"),
        );
        let body = bytes::Bytes::from_static(br#"{"model":"test","service_tier":"priority","input":[],"client_metadata":{"parent_response_id":"parent"}}"#);
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            client.forward_endpoint(crate::Endpoint::Responses, body, headers),
        )
        .await
        .unwrap();
        assert!(matches!(response, Err(UpstreamError::WorkspaceRouting(_))));
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            client.forward_endpoint(
                crate::Endpoint::Compact,
                bytes::Bytes::from_static(br#"{"model":"test","input":[]}"#),
                HeaderMap::new(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let (discovery, _) = rx.recv().await.unwrap();
        assert!(discovery.contains("chatgpt-account-id: selected"));
        assert!(!discovery.contains("\r\nversion:"));
        assert!(!discovery.contains("\r\noriginator:"));
        let (request, bytes) = rx.recv().await.unwrap();
        assert!(request.contains("x-openai-account-routing-override: us"));
        assert!(request.contains("x-codex-guardian: reviewer"));
        assert!(request.contains("authorization: bearer upstream-token"));
        assert!(!request.contains("caller-controlled"));
        assert!(!request.contains("x-codex-routing-hint:"));
        let body: Value =
            serde_json::from_slice(&zstd::stream::decode_all(bytes.as_slice()).unwrap()).unwrap();
        assert!(body.get("service_tier").is_none());
        assert_eq!(body["client_metadata"]["parent_response_id"], "parent");
        rx.recv().await.unwrap();
        let (compact, _) = rx.recv().await.unwrap();
        assert!(compact.contains("x-openai-account-routing-override: us"));
        server.await.unwrap();
    }
}
