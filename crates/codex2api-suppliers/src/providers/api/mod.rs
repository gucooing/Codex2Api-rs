//! API-key supplier accounts: one Base URL, one key, one wire format. There is no
//! OAuth, no token refresh and no device fingerprint; a rejected key is replaced by
//! an administrator. Each account keeps its own HTTP client and outbound proxy.
mod credentials;
mod error;
pub use credentials::{ApiCredentials, normalize_base_url};
pub use error::ApiAuthError;
pub type Result<T> = std::result::Result<T, ApiAuthError>;

use codex2api_common::providers::api as channel;
use codex2api_common::version::api as version;
use codex2api_storage::{NewSupplierAccount, SupplierAccount, SupplierAccountUpdate};
use reqwest::header::{HeaderMap, HeaderValue};
use std::sync::Arc;

/// One HTTP client per supplier account; connection pools are never shared.
pub struct ApiHttpClient {
    pub client: reqwest::Client,
    pub(crate) proxy: Option<String>,
}
impl ApiHttpClient {
    pub fn with_proxy(proxy: Option<&str>) -> Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none());
        if let Some(proxy) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(codex2api_storage::parse_proxy_url(
                proxy,
            )?)?);
        }
        Ok(Self {
            client: builder.build()?,
            proxy: proxy.map(str::to_owned),
        })
    }
}

#[derive(Clone)]
struct CachedHttp {
    snapshot: Arc<codex2api_storage::SupplierTransportSnapshot>,
    client: Arc<ApiHttpClient>,
}

#[derive(Clone)]
pub struct ApiAuthService {
    storage: Option<codex2api_storage::Storage>,
    clients: Arc<crate::account_cache::AccountCache<CachedHttp>>,
}

/// The fixed identity and the key, as the request headers of one account.
pub fn headers(credentials: &ApiCredentials, stream: bool) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static(version::USER_AGENT),
    );
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static(if stream {
            "text/event-stream"
        } else {
            "application/json"
        }),
    );
    let key = |value: String| {
        HeaderValue::from_str(&value).map_err(|_| ApiAuthError::Invalid("API Key 无效".into()))
    };
    match credentials.auth_scheme.as_str() {
        channel::AUTH_X_API_KEY => {
            headers.insert(version::X_API_KEY_HEADER, key(credentials.api_key.clone())?);
            headers.insert(
                version::ANTHROPIC_VERSION_HEADER,
                HeaderValue::from_static(version::ANTHROPIC_VERSION),
            );
        }
        _ => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                key(format!("Bearer {}", credentials.api_key))?,
            );
        }
    }
    Ok(headers)
}

/// The display name of an account the administrator did not name: host and key tail.
pub fn default_display_name(credentials: &ApiCredentials) -> String {
    let host = url::Url::parse(&credentials.base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| credentials.base_url.clone());
    format!("{host} …{}", credentials.key_suffix())
}

impl ApiAuthService {
    pub fn new(storage: Option<codex2api_storage::Storage>) -> Self {
        Self {
            storage,
            clients: Arc::default(),
        }
    }
    pub fn storage(&self) -> Result<&codex2api_storage::Storage> {
        self.storage
            .as_ref()
            .ok_or_else(|| ApiAuthError::Invalid("API storage unavailable".into()))
    }

    pub async fn account_http(&self, id: &str) -> Result<Arc<ApiHttpClient>> {
        let snapshot = self.storage()?.supplier_transport_snapshot(id).await?;
        if snapshot.provider_id != codex2api_common::API {
            return Err(ApiAuthError::AccountMismatch);
        }
        let current = snapshot.clone();
        Ok(self
            .clients
            .get_or_update(
                id,
                |old| {
                    Arc::ptr_eq(&old.snapshot, &current)
                        || old.snapshot.cache_generation > current.cache_generation
                },
                |old| async move {
                    let proxy = snapshot.proxy_url.clone();
                    if let Some(old) = &old
                        && old.client.proxy == proxy
                    {
                        return Ok(CachedHttp {
                            snapshot,
                            client: old.client.clone(),
                        });
                    }
                    let client = Arc::new(ApiHttpClient::with_proxy(proxy.as_deref())?);
                    Ok::<_, ApiAuthError>(CachedHttp { snapshot, client })
                },
            )
            .await?
            .client)
    }
    pub async fn evict_account_http(&self, id: &str) {
        self.clients.remove(id);
    }

    /// The account's credentials and the revision they were read on.
    pub async fn credentials(&self, id: &str) -> Result<(Arc<ApiCredentials>, i64)> {
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| ApiAuthError::TokensNotFound(id.into()))?;
        if snapshot.provider_id != codex2api_common::API {
            return Err(ApiAuthError::AccountMismatch);
        }
        let credentials =
            snapshot.decode(|| ApiCredentials::from_supplier_tokens(&snapshot.tokens))?;
        Ok((credentials, snapshot.auth_revision))
    }

    /// Create an account for a key, or return the existing account of the same key at
    /// the same Base URL. The identity is the Base URL and the key's fingerprint.
    pub async fn create(
        &self,
        name: Option<&str>,
        mut credentials: ApiCredentials,
        proxy: Option<&str>,
    ) -> Result<(SupplierAccount, bool)> {
        credentials.validate()?;
        let storage = self.storage()?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut new = NewSupplierAccount::without_identity(
            codex2api_common::API,
            uuid::Uuid::new_v4().to_string(),
        );
        new.id = Some(id.clone());
        new.status = codex2api_storage::SupplierStatus::Active;
        new.chatgpt_account_id = Some(credentials.base_url.clone());
        new.chatgpt_user_id = Some(credentials.key_fingerprint());
        new.display_name = Some(
            name.map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| default_display_name(&credentials)),
        );
        let account = storage
            .save_authorized_account(new, credentials.to_supplier_tokens(&id), proxy)
            .await?;
        let reused = account.id != id;
        self.evict_account_http(&account.id).await;
        Ok((account, reused))
    }

    /// Replace the key, Base URL or wire of an existing account; the account keeps
    /// its ID, scheduling, tags and models.
    pub async fn replace(
        &self,
        id: &str,
        mut credentials: ApiCredentials,
    ) -> Result<SupplierAccount> {
        credentials.validate()?;
        let storage = self.storage()?;
        let account = storage.require_account(id).await?;
        if account.provider_id != codex2api_common::API {
            return Err(ApiAuthError::AccountMismatch);
        }
        storage
            .update_account(
                id,
                SupplierAccountUpdate {
                    chatgpt_account_id: Some(credentials.base_url.clone()),
                    chatgpt_user_id: Some(credentials.key_fingerprint()),
                    ..Default::default()
                },
            )
            .await?;
        let mut new = NewSupplierAccount::without_identity(
            codex2api_common::API,
            account.installation_id.clone(),
        );
        new.id = Some(id.to_owned());
        new.status = account.status;
        new.display_name = account.display_name.clone();
        new.chatgpt_account_id = Some(credentials.base_url.clone());
        new.chatgpt_user_id = Some(credentials.key_fingerprint());
        let saved = storage
            .save_authorized_account(
                new,
                credentials.to_supplier_tokens(id),
                account.proxy_id.as_deref(),
            )
            .await?;
        self.evict_account_http(id).await;
        Ok(saved)
    }
}
