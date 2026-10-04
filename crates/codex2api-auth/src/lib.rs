//! Official ChatGPT/Codex OAuth login, refresh, and revoke.
//!
//! Behavior matches official Codex CLI at the pinned commit
//! `a956835d020762cb2b570053af06f643a11c0ecc`.
//!
//! The library never opens a system browser. Callers (admin UI) receive
//! an authorization URL or device code and present it to the user.
//! Browser callback URLs are submitted manually; no local listener is started.
//!
//! `flow` orchestrates AuthService; `manual` handles callback submission/device login;
//! `oauth` implements the pinned OAuth exchanges and protocol helpers;
//! `persist` writes account credentials; `tokens` interprets token data;
//! `transport` owns isolated account HTTP clients and cookie/TLS policy.
//! Admin HTTP routes are registered in codex2api-admin, not here.

use providers::chatgpt::draft;
pub use providers::grok;
pub mod error;
pub mod providers;
pub use providers::chatgpt::flow;
pub use providers::chatgpt::manual;
pub use providers::chatgpt::oauth;
pub use providers::chatgpt::persist;
pub use providers::chatgpt::tokens;
pub use providers::chatgpt::transport;

pub use codex2api_accounts::{AuthDotJson, TokenData};
pub use error::{AuthError, Result};
pub use flow::AuthService;
pub use manual::LoginFlow;
pub use oauth::{
    AcceptedCallback, CALLBACK_PATH, CallbackListener, CallbackQuery, OAuthConfig, PendingLogin,
    PkceCodes, accept_callback_request, bind_callback_listener, build_authorize_url,
    exchange_code_for_tokens, generate_pkce, generate_state, redirect_uri, refresh_tokens,
    revoke_tokens, start_pending_login, wait_for_callback,
};
pub use persist::{
    CompletedLogin, bind_completed_login, complete_login, load_auth, persist_auth,
    persist_exchanged_tokens, refresh_account, revoke_account,
};
pub use tokens::{
    AUTH_MODE_CHATGPT, ExchangedTokens, IdTokenInfo, RefreshResponse, TokenSet,
    default_http_client, parse_chatgpt_jwt_claims, parse_chatgpt_subscription_expiration,
    parse_jwt_expiration, refresh_http_client, should_refresh, token_set_from_auth,
};
