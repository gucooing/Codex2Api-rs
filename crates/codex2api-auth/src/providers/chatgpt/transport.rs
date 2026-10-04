use std::sync::Arc;

use codex2api_accounts::AccountIdentity;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::Result;

pub fn ensure_tls_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn custom_ca() -> Result<Vec<rustls_pki_types::CertificateDer<'static>>> {
    use rustls_pki_types::pem::PemObject;
    let path = std::env::var_os("CODEX_CA_CERTIFICATE")
        .filter(|p| !p.is_empty())
        .or_else(|| std::env::var_os("SSL_CERT_FILE").filter(|p| !p.is_empty()));
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let certs = rustls_pki_types::CertificateDer::pem_file_iter(path)
        .map_err(anyhow::Error::from)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(anyhow::Error::from)?;
    if certs.is_empty() {
        return Err(anyhow::anyhow!("Custom CA file contains no certificates").into());
    }
    Ok(certs)
}

pub fn http_builder() -> Result<reqwest::ClientBuilder> {
    ensure_tls_provider();
    let certs = custom_ca()?;
    let mut builder = reqwest::Client::builder().no_proxy();
    if !certs.is_empty() {
        builder = builder.use_rustls_tls();
        for cert in certs {
            builder = builder.add_root_certificate(reqwest::Certificate::from_der(&cert)?);
        }
    }
    Ok(builder)
}

pub fn websocket_tls_config() -> Result<Arc<rustls::ClientConfig>> {
    ensure_tls_provider();
    let mut roots = rustls::RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    for cert in custom_ca()? {
        roots.add(cert).map_err(anyhow::Error::from)?;
    }
    Ok(Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

/// Same infrastructure-cookie allowlist as pinned Codex, scoped to one account.
#[derive(Default, Debug)]
pub struct AccountCookieStore {
    jar: Jar,
}

fn allowed_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            matches!(
                host,
                "chatgpt.com" | "chat.openai.com" | "chatgpt-staging.com"
            ) || host.ends_with(".chatgpt.com")
                || host.ends_with(".chatgpt-staging.com")
        })
}

fn allowed_cookie(value: &HeaderValue) -> bool {
    value
        .to_str()
        .ok()
        .and_then(|s| s.split_once('='))
        .is_some_and(|(name, _)| {
            let name = name.trim();
            matches!(
                name,
                "__cf_bm"
                    | "__cflb"
                    | "__cfruid"
                    | "__cfseq"
                    | "__cfwaitingroom"
                    | "__oailb"
                    | "_cfuvid"
                    | "cf_clearance"
                    | "cf_ob_info"
                    | "cf_use_ob"
            ) || name.starts_with("cf_chl_")
        })
}

impl CookieStore for AccountCookieStore {
    fn set_cookies(&self, cookies: &mut dyn Iterator<Item = &HeaderValue>, url: &reqwest::Url) {
        if allowed_url(url) {
            self.jar
                .set_cookies(&mut cookies.filter(|value| allowed_cookie(value)), url);
        }
    }

    fn cookies(&self, url: &reqwest::Url) -> Option<HeaderValue> {
        allowed_url(url).then(|| self.jar.cookies(url)).flatten()
    }
}

pub fn identity_headers(identity: &AccountIdentity) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "originator",
        HeaderValue::from_static(codex2api_version::DEFAULT_ORIGINATOR),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&identity.official_user_agent()).map_err(anyhow::Error::from)?,
    );
    Ok(headers)
}

/// Independent connections and cookies for each account, including OAuth exchange.
pub struct AccountHttpClients {
    pub identity: AccountIdentity,
    proxy_url: Option<String>,
    pub raw: reqwest::Client,
    pub authenticated: reqwest::Client,
    pub api: reqwest::Client,
    pub routed_api: reqwest::Client,
    pub cookies: Arc<AccountCookieStore>,
    pub refresh_lock: Arc<tokio::sync::Mutex<()>>,
    pub routing_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AccountHttpClients {
    pub fn new(identity: &AccountIdentity) -> Result<Self> {
        Self::with_proxy(identity, None)
    }

    pub fn with_proxy(identity: &AccountIdentity, proxy_url: Option<&str>) -> Result<Self> {
        Self::build(
            identity,
            proxy_url,
            Arc::new(AccountCookieStore::default()),
            Arc::new(tokio::sync::Mutex::new(())),
            Arc::new(tokio::sync::Mutex::new(())),
        )
    }

    fn build(
        identity: &AccountIdentity,
        proxy_url: Option<&str>,
        cookies: Arc<AccountCookieStore>,
        refresh_lock: Arc<tokio::sync::Mutex<()>>,
        routing_lock: Arc<tokio::sync::Mutex<()>>,
    ) -> Result<Self> {
        let builder = || -> Result<reqwest::ClientBuilder> {
            let mut builder = http_builder()?;
            if let Some(url) = proxy_url {
                let url = codex2api_storage::parse_proxy_url(url)?;
                builder = builder.proxy(reqwest::Proxy::all(url)?);
            }
            Ok(builder)
        };
        let raw = builder()?.build()?;
        let authenticated = builder()?
            .default_headers(identity_headers(identity)?)
            .cookie_provider(cookies.clone())
            .build()?;
        // API endpoint builders choose their own header set (e.g. WHAM has no version/originator).
        let api = builder()?.cookie_provider(cookies.clone()).build()?;
        let routed_api = builder()?
            .redirect(reqwest::redirect::Policy::none())
            .cookie_provider(cookies.clone())
            .build()?;
        Ok(Self {
            identity: identity.clone(),
            proxy_url: proxy_url.map(str::to_owned),
            raw,
            authenticated,
            api,
            routed_api,
            cookies,
            refresh_lock,
            routing_lock,
        })
    }

    pub fn proxy_url(&self) -> Option<&str> {
        self.proxy_url.as_deref()
    }

    pub(crate) fn matches(&self, identity: &AccountIdentity, proxy_url: Option<&str>) -> bool {
        self.identity == *identity && self.proxy_url() == proxy_url
    }

    /// Rebuild every connection pool while retaining this account's cookies and refresh lock.
    pub(crate) fn reconfigure(
        &self,
        identity: &AccountIdentity,
        proxy_url: Option<&str>,
    ) -> Result<Self> {
        Self::build(
            identity,
            proxy_url,
            self.cookies.clone(),
            self.refresh_lock.clone(),
            self.routing_lock.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_are_allowlisted_scoped_and_isolated() {
        let a = AccountCookieStore::default();
        let b = AccountCookieStore::default();
        let url = "https://chatgpt.com/backend-api/codex/responses"
            .parse()
            .unwrap();
        let values = [
            HeaderValue::from_static("__cf_bm=allowed; Secure; Path=/"),
            HeaderValue::from_static("__oailb=route; Secure; Path=/"),
            HeaderValue::from_static("session=private; Secure; Path=/"),
        ];
        a.set_cookies(&mut values.iter(), &url);
        let cookies = a.cookies(&url).unwrap();
        let cookies = cookies.to_str().unwrap();
        assert!(cookies.contains("__cf_bm=allowed"));
        assert!(cookies.contains("__oailb=route"));
        assert!(!cookies.contains("session="));
        assert!(b.cookies(&url).is_none());
        for url in [
            "http://chatgpt.com/",
            "https://auth.openai.com/",
            "https://chatgpt.com.evil.example/",
        ] {
            assert!(a.cookies(&url.parse().unwrap()).is_none());
        }
    }
}
