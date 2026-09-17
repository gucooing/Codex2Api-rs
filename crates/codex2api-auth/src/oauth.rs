use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

use crate::error::{AuthError, Result};
use crate::tokens::{ExchangedTokens, RefreshResponse, parse_token_endpoint_error};
use codex2api_version::{
    DEFAULT_ORIGINATOR, OAUTH_CALLBACK_FALLBACK_PORT, OAUTH_CALLBACK_PORT, OAUTH_CLIENT_ID,
    OAUTH_ISSUER, OAUTH_REVOKE_URL, OAUTH_SCOPE, OAUTH_TOKEN_URL,
};

/// Official localhost callback path used by Codex CLI.
pub const CALLBACK_PATH: &str = "/auth/callback";
pub const CANCEL_PATH: &str = "/cancel";

const REVOKE_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const TOKEN_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub issuer: String,
    pub client_id: String,
    pub token_url: String,
    pub revoke_url: String,
    pub scope: String,
    pub callback_port: u16,
    pub callback_fallback_port: u16,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            issuer: OAUTH_ISSUER.to_string(),
            client_id: OAUTH_CLIENT_ID.to_string(),
            token_url: OAUTH_TOKEN_URL.to_string(),
            revoke_url: OAUTH_REVOKE_URL.to_string(),
            scope: OAUTH_SCOPE.to_string(),
            callback_port: OAUTH_CALLBACK_PORT,
            callback_fallback_port: OAUTH_CALLBACK_FALLBACK_PORT,
        }
    }
}

impl OAuthConfig {
    pub fn authorize_url(&self) -> String {
        format!("{}/oauth/authorize", self.issuer.trim_end_matches('/'))
    }

    pub fn token_endpoint(&self) -> String {
        if self.token_url.is_empty() {
            format!("{}/oauth/token", self.issuer.trim_end_matches('/'))
        } else {
            self.token_url.clone()
        }
    }

    pub fn revoke_endpoint(&self) -> String {
        if self.revoke_url.is_empty() {
            format!("{}/oauth/revoke", self.issuer.trim_end_matches('/'))
        } else {
            self.revoke_url.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingLogin {
    pub state: String,
    pub pkce: PkceCodes,
    pub redirect_uri: String,
    pub authorize_url: String,
    pub callback_port: u16,
}

#[derive(Debug, Clone, Default)]
pub struct CallbackQuery {
    pub path: String,
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

impl CallbackQuery {
    pub fn is_cancel(&self) -> bool {
        self.path == CANCEL_PATH
    }
}

/// Localhost listener for the official `/auth/callback` redirect.
pub struct CallbackListener {
    listener: TcpListener,
    pub port: u16,
    shutdown: Arc<Notify>,
}

impl CallbackListener {
    pub async fn bind(cfg: &OAuthConfig) -> Result<Self> {
        let (listener, port) = bind_callback_listener(cfg).await?;
        Ok(Self {
            listener,
            port,
            shutdown: Arc::new(Notify::new()),
        })
    }

    pub fn cancel_handle(&self) -> Arc<Notify> {
        self.shutdown.clone()
    }

    pub fn cancel(&self) {
        self.shutdown.notify_waiters();
    }

    pub fn into_parts(self) -> (TcpListener, u16, Arc<Notify>) {
        (self.listener, self.port, self.shutdown)
    }
}

pub fn generate_pkce() -> PkceCodes {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}{CALLBACK_PATH}")
}

/// Query keys and order match official Codex CLI `build_authorize_url`.
pub fn build_authorize_url(
    cfg: &OAuthConfig,
    redirect_uri: &str,
    pkce: &PkceCodes,
    state: &str,
) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", redirect_uri),
        ("scope", cfg.scope.as_str()),
        ("code_challenge", pkce.code_challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", DEFAULT_ORIGINATOR),
    ];
    let qs = query
        .into_iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{qs}", cfg.authorize_url())
}

pub fn start_pending_login(cfg: &OAuthConfig, port: u16) -> PendingLogin {
    let pkce = generate_pkce();
    let state = generate_state();
    let redirect_uri = redirect_uri(port);
    let authorize_url = build_authorize_url(cfg, &redirect_uri, &pkce, &state);
    PendingLogin {
        state,
        pkce,
        redirect_uri,
        authorize_url,
        callback_port: port,
    }
}

/// Authorization-code exchange. Body matches official Codex CLI:
/// `grant_type=authorization_code&code&redirect_uri&client_id&code_verifier`
pub async fn exchange_code_for_tokens(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<ExchangedTokens> {
    let token_endpoint = cfg.token_endpoint();
    tracing::info!(
        token_endpoint = %token_endpoint,
        redirect_uri = %redirect_uri,
        "starting oauth token exchange"
    );
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding::encode(code),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&cfg.client_id),
        urlencoding::encode(code_verifier)
    );
    let resp = http
        .post(&token_endpoint)
        // Official CLI uses create_raw_auth_client: no originator / Codex UA.
        .header("Content-Type", "application/x-www-form-urlencoded")
        .timeout(TOKEN_HTTP_TIMEOUT)
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let detail = parse_token_endpoint_error(&text);
        tracing::warn!(
            %status,
            error_code = detail.error_code.as_deref().unwrap_or("unknown"),
            "oauth token exchange returned non-success status"
        );
        return Err(AuthError::token_endpoint(status, detail.display_message));
    }
    let tokens: TokenResponse = resp.json().await?;
    tracing::info!(%status, "oauth token exchange succeeded");
    Ok(ExchangedTokens {
        id_token: tokens.id_token,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    })
}

/// Optional post-login exchange attempted by the official browser login flow.
pub async fn obtain_api_key(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    id_token: &str,
) -> Result<String> {
    #[derive(Deserialize)]
    struct ApiKeyResponse {
        access_token: String,
    }
    let response = http
        .post(cfg.token_endpoint())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .timeout(TOKEN_HTTP_TIMEOUT)
        .body(format!(
            "grant_type={}&client_id={}&requested_token={}&subject_token={}&subject_token_type={}",
            urlencoding::encode("urn:ietf:params:oauth:grant-type:token-exchange"),
            urlencoding::encode(&cfg.client_id),
            urlencoding::encode("openai-api-key"),
            urlencoding::encode(id_token),
            urlencoding::encode("urn:ietf:params:oauth:token-type:id_token"),
        ))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<ApiKeyResponse>().await?.access_token)
}

/// Refresh POST JSON `{ client_id, grant_type: "refresh_token", refresh_token }`.
pub async fn refresh_tokens(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    refresh_token: &str,
) -> Result<RefreshResponse> {
    if refresh_token.is_empty() {
        return Err(AuthError::MissingRefreshToken);
    }
    let refresh_request = RefreshRequest {
        client_id: cfg.client_id.clone(),
        grant_type: "refresh_token",
        refresh_token: refresh_token.to_string(),
    };
    let endpoint = cfg.token_endpoint();
    let resp = http
        .post(&endpoint)
        // Official CLI: create_client() already carries originator + User-Agent.
        // This request only sets Content-Type, matching request_chatgpt_token_refresh.
        .header("Content-Type", "application/json")
        .timeout(TOKEN_HTTP_TIMEOUT)
        .json(&refresh_request)
        .send()
        .await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.json::<RefreshResponse>().await?);
    }
    let body = resp.text().await.unwrap_or_default();
    tracing::error!("Failed to refresh token: {status}: {body}");
    let detail = parse_token_endpoint_error(&body);
    let code = detail.error_code.as_deref().unwrap_or("");
    let permanent = status.as_u16() == 401
        || (status.as_u16() == 400 && code.eq_ignore_ascii_case("invalid_grant"))
        || matches!(
            code.to_ascii_lowercase().as_str(),
            "refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated"
        );
    let message = if permanent {
        refresh_failure_message(code)
    } else {
        detail.display_message
    };
    Err(AuthError::Refresh(format!("{status}: {message}")))
}

/// Revoke POST JSON to `https://auth.openai.com/oauth/revoke`.
/// Prefers the refresh token; falls back to the access token.
pub async fn revoke_tokens(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    refresh_token: Option<&str>,
    access_token: Option<&str>,
) -> Result<()> {
    let (token, hint, include_client_id) =
        if let Some(refresh) = refresh_token.filter(|s| !s.is_empty()) {
            (refresh, "refresh_token", true)
        } else if let Some(access) = access_token.filter(|s| !s.is_empty()) {
            (access, "access_token", false)
        } else {
            return Ok(());
        };
    let request = RevokeTokenRequest {
        token,
        token_type_hint: hint,
        client_id: include_client_id.then(|| cfg.client_id.clone()),
    };
    let endpoint = cfg.revoke_endpoint();
    let resp = http
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .header("originator", DEFAULT_ORIGINATOR)
        .timeout(REVOKE_HTTP_TIMEOUT)
        .json(&request)
        .send()
        .await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().await.unwrap_or_default();
    let message = parse_token_endpoint_error(&body).display_message;
    Err(AuthError::token_endpoint(
        status,
        format!("failed to revoke {hint}: {message}"),
    ))
}

pub async fn bind_callback_listener(cfg: &OAuthConfig) -> Result<(TcpListener, u16)> {
    match TcpListener::bind(("127.0.0.1", cfg.callback_port)).await {
        Ok(listener) => {
            let port = listener.local_addr()?.port();
            return Ok((listener, port));
        }
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            try_send_cancel(cfg.callback_port).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", cfg.callback_port)).await {
                let port = listener.local_addr()?.port();
                return Ok((listener, port));
            }
        }
        Err(err) => return Err(err.into()),
    }

    match TcpListener::bind(("127.0.0.1", cfg.callback_fallback_port)).await {
        Ok(listener) => {
            let port = listener.local_addr()?.port();
            Ok((listener, port))
        }
        Err(_) => Err(AuthError::CallbackBind(cfg.callback_port)),
    }
}

pub async fn wait_for_callback(
    listener: &TcpListener,
    shutdown: &Notify,
    expected_state: &str,
) -> Result<CallbackQuery> {
    let mut accepted = accept_callback_request(listener, shutdown).await?;
    if accepted.query.state.as_deref() != Some(expected_state) {
        let _ = accepted.respond_state_mismatch().await;
        return Err(AuthError::StateMismatch);
    }
    Ok(accepted.query)
}

pub async fn write_callback_html(stream: &mut TcpStream, status: u16, body: &str) -> Result<()> {
    write_http_response(stream, status, "text/html; charset=utf-8", body.as_bytes()).await
}

pub fn success_html() -> &'static str {
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>Codex2API</title></head>\
     <body><p>Sign-in complete. You can close this window.</p></body></html>"
}

pub fn error_html(message: &str) -> String {
    let escaped = html_escape(message);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Codex2API</title></head>\
         <body><p>Sign-in failed: {escaped}</p></body></html>"
    )
}

/// Accept one HTTP request on the callback listener and return the stream so the
/// caller can exchange tokens before writing the final HTML page.
pub async fn accept_callback_request(
    listener: &TcpListener,
    shutdown: &Notify,
) -> Result<AcceptedCallback> {
    loop {
        tokio::select! {
            _ = shutdown.notified() => return Err(AuthError::Cancelled),
            accepted = listener.accept() => {
                let (mut stream, _) = accepted?;
                let path = match read_http_target(&mut stream).await {
                    Ok(path) => path,
                    Err(_) => {
                        let _ = write_http_response(&mut stream, 400, "text/plain; charset=utf-8", b"Bad Request").await;
                        continue;
                    }
                };
                let parsed = match url::Url::parse(&format!("http://localhost{path}")) {
                    Ok(url) => url,
                    Err(_) => {
                        let _ = write_http_response(&mut stream, 400, "text/plain; charset=utf-8", b"Bad Request").await;
                        continue;
                    }
                };
                let route = parsed.path().to_string();
                if route == CANCEL_PATH {
                    let _ = write_http_response(&mut stream, 200, "text/plain; charset=utf-8", b"cancelled").await;
                    return Err(AuthError::Cancelled);
                }
                if route != CALLBACK_PATH {
                    let _ = write_http_response(&mut stream, 404, "text/plain; charset=utf-8", b"Not Found").await;
                    continue;
                }
                let query = callback_query_from_url(&parsed);
                return Ok(AcceptedCallback { stream, query });
            }
        }
    }
}

pub struct AcceptedCallback {
    pub stream: TcpStream,
    pub query: CallbackQuery,
}

impl AcceptedCallback {
    pub async fn respond_success(&mut self) -> Result<()> {
        write_callback_html(&mut self.stream, 200, success_html()).await
    }

    pub async fn respond_error(&mut self, status: u16, message: &str) -> Result<()> {
        write_callback_html(&mut self.stream, status, &error_html(message)).await
    }

    pub async fn respond_state_mismatch(&mut self) -> Result<()> {
        write_http_response(
            &mut self.stream,
            400,
            "text/plain; charset=utf-8",
            b"State mismatch",
        )
        .await
    }
}

fn callback_query_from_url(url: &url::Url) -> CallbackQuery {
    let mut query = CallbackQuery {
        path: url.path().to_string(),
        ..CallbackQuery::default()
    };
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" if !value.is_empty() => query.code = Some(value.into_owned()),
            "state" if !value.is_empty() => query.state = Some(value.into_owned()),
            "error" if !value.is_empty() => query.error = Some(value.into_owned()),
            "error_description" if !value.is_empty() => {
                query.error_description = Some(value.into_owned())
            }
            _ => {}
        }
    }
    query
}

async fn read_http_target(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 64 * 1024 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let first = text.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    if !method.eq_ignore_ascii_case("GET") && !method.eq_ignore_ascii_case("HEAD") {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "only GET is supported on the oauth callback listener",
        ));
    }
    Ok(target.to_string())
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    let _ = stream.shutdown().await;
    Ok(())
}

async fn try_send_cancel(port: u16) {
    let Ok(stream) = tokio::time::timeout(
        Duration::from_secs(2),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    else {
        return;
    };
    let Ok(mut stream) = stream else {
        return;
    };
    let req =
        format!("GET /cancel HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(req.as_bytes()).await;
    let mut buf = [0u8; 64];
    let _ = stream.read(&mut buf).await;
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn refresh_failure_message(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "refresh_token_expired" => {
            "Your access token could not be refreshed because your refresh token has expired. Please log out and sign in again."
                .to_string()
        }
        "refresh_token_reused" => {
            "Your access token could not be refreshed because your refresh token was already used. Please log out and sign in again."
                .to_string()
        }
        "refresh_token_invalidated" => {
            "Your access token could not be refreshed because your refresh token was revoked. Please log out and sign in again."
                .to_string()
        }
        _ => "Your access token could not be refreshed. Please log out and sign in again.".to_string(),
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

#[derive(Serialize)]
struct RefreshRequest {
    client_id: String,
    grant_type: &'static str,
    refresh_token: String,
}

#[derive(Serialize)]
struct RevokeTokenRequest<'a> {
    token: &'a str,
    token_type_hint: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_query_keys_match_official() {
        let cfg = OAuthConfig::default();
        let pkce = PkceCodes {
            code_verifier: "verifier".into(),
            code_challenge: "challenge".into(),
        };
        let url = build_authorize_url(
            &cfg,
            "http://localhost:1455/auth/callback",
            &pkce,
            "state123",
        );
        let parsed = url::Url::parse(&url).expect("url");
        assert_eq!(
            parsed.as_str().split('?').next().unwrap(),
            "https://auth.openai.com/oauth/authorize"
        );
        let pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "response_type",
                "client_id",
                "redirect_uri",
                "scope",
                "code_challenge",
                "code_challenge_method",
                "id_token_add_organizations",
                "codex_cli_simplified_flow",
                "state",
                "originator",
            ]
        );
        assert_eq!(pairs[0].1, "code");
        assert_eq!(pairs[1].1, OAUTH_CLIENT_ID);
        assert_eq!(pairs[2].1, "http://localhost:1455/auth/callback");
        assert_eq!(pairs[3].1, OAUTH_SCOPE);
        assert_eq!(pairs[4].1, "challenge");
        assert_eq!(pairs[5].1, "S256");
        assert_eq!(pairs[6].1, "true");
        assert_eq!(pairs[7].1, "true");
        assert_eq!(pairs[8].1, "state123");
        assert_eq!(pairs[9].1, DEFAULT_ORIGINATOR);
    }

    #[test]
    fn pkce_s256_is_url_safe_unpadded() {
        let pkce = generate_pkce();
        assert!(pkce.code_verifier.len() >= 43);
        assert!(!pkce.code_verifier.contains('='));
        assert!(!pkce.code_challenge.contains('='));
        let digest = Sha256::digest(pkce.code_verifier.as_bytes());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        assert_eq!(pkce.code_challenge, expected);
    }

    #[test]
    fn redirect_uri_matches_official_path() {
        assert_eq!(redirect_uri(1455), "http://localhost:1455/auth/callback");
    }

    #[test]
    fn default_config_uses_pinned_constants() {
        let cfg = OAuthConfig::default();
        assert_eq!(cfg.client_id, "app_EMoamEEZ73f0CkXaXp7hrann");
        assert_eq!(cfg.issuer, "https://auth.openai.com");
        assert_eq!(cfg.token_url, "https://auth.openai.com/oauth/token");
        assert_eq!(cfg.revoke_url, "https://auth.openai.com/oauth/revoke");
        assert_eq!(cfg.callback_port, 1455);
        assert_eq!(cfg.callback_fallback_port, 1457);
    }
}
