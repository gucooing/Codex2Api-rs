//! Pinned official Codex client version this proxy is implemented against.
//!
//! Official source is reference only. Communication with OpenAI/Codex servers
//! must match a Codex CLI that is logged in directly, for this snapshot.

use serde::Serialize;

/// Official Codex git repository used as protocol reference.
pub const CODEX_REPO: &str = "https://github.com/openai/codex";

/// Branch the local snapshot was taken from.
pub const CODEX_REF_BRANCH: &str = "main";

/// Exact commit the current implementation is aligned with.
pub const CODEX_REF_COMMIT: &str = "a8964cb1bad67bc26a826fb07d1bef99c6a3f008";

/// Commit timestamp (UTC).
pub const CODEX_REF_COMMIT_DATE: &str = "2026-09-15T05:44:41Z";

/// First-line commit message of the reference snapshot.
pub const CODEX_REF_COMMIT_MESSAGE: &str = "Render standalone display math in the TUI (#45612)";

/// Local path of the official source snapshot in this repo.
pub const CODEX_REF_PATH: &str = "reference/codex";

/// Current official packaged CLI release this proxy pretends to be.
///
/// GitHub latest stable: https://github.com/openai/codex/releases/tag/rust-v0.154.0
/// (`CARGO_PKG_VERSION` stamped into that binary). Not the source-tree `0.0.0`.
pub const CODEX_RELEASE_VERSION: &str = "0.154.0";

/// GitHub release tag for [`CODEX_RELEASE_VERSION`].
pub const CODEX_RELEASE_TAG: &str = "rust-v0.154.0";

/// Commit the `rust-v0.154.0` tag points at.
pub const CODEX_RELEASE_COMMIT: &str = "6b9826e3aa83b1a5947db50f4332cb9c65f1b340";

/// User-Agent version token. Always the packaged release, never source `0.0.0`.
pub const CODEX_PACKAGE_VERSION: &str = CODEX_RELEASE_VERSION;

/// Official HTTP header names / values used by Codex CLI on Responses requests.
pub const HEADER_ORIGINATOR: &str = "originator";
pub const HEADER_CHATGPT_ACCOUNT_ID: &str = "ChatGPT-Account-ID";
pub const HEADER_INSTALLATION_ID: &str = "x-codex-installation-id";
pub const HEADER_SESSION_ID: &str = "session-id";
pub const HEADER_THREAD_ID: &str = "thread-id";
pub const HEADER_CLIENT_REQUEST_ID: &str = "x-client-request-id";
pub const HEADER_ACCEPT_SSE: &str = "text/event-stream";

/// Default originator used by official Codex CLI (`codex_cli_rs`).
pub const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";

/// Official OAuth client id for Codex CLI login.
pub const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// Official OAuth issuer.
pub const OAUTH_ISSUER: &str = "https://auth.openai.com";

/// Official token endpoint.
pub const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

/// Official revoke endpoint.
pub const OAUTH_REVOKE_URL: &str = "https://auth.openai.com/oauth/revoke";

/// Official ChatGPT backend base URL (trailing slash, as in Codex config default).
pub const CHATGPT_BACKEND_BASE_URL: &str = "https://chatgpt.com/backend-api/";

/// Official Codex Responses API base URL.
pub const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

/// Official Responses path relative to the Codex base URL.
pub const RESPONSES_PATH: &str = "/responses";

/// Default OAuth callback port used by Codex CLI.
pub const OAUTH_CALLBACK_PORT: u16 = 1455;

/// Fallback OAuth callback port used by Codex CLI.
pub const OAUTH_CALLBACK_FALLBACK_PORT: u16 = 1457;

/// Official OAuth scopes.
pub const OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

/// User-Agent format used by official Codex CLI:
/// `{originator}/{build_version} ({os_type} {os_version}; {arch}) {terminal_ua}`
pub fn official_user_agent(
    os_type: &str,
    os_version: &str,
    arch: &str,
    terminal_ua: &str,
) -> String {
    format!(
        "{DEFAULT_ORIGINATOR}/{CODEX_PACKAGE_VERSION} ({os_type} {os_version}; {arch}) {terminal_ua}"
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexReference {
    pub repo: &'static str,
    pub branch: &'static str,
    pub commit: &'static str,
    pub commit_date: &'static str,
    pub commit_message: &'static str,
    pub path: &'static str,
    pub package_version: &'static str,
    pub release_tag: &'static str,
    pub release_commit: &'static str,
    pub originator: &'static str,
    pub oauth_client_id: &'static str,
}

pub fn reference() -> CodexReference {
    CodexReference {
        repo: CODEX_REPO,
        branch: CODEX_REF_BRANCH,
        commit: CODEX_REF_COMMIT,
        commit_date: CODEX_REF_COMMIT_DATE,
        commit_message: CODEX_REF_COMMIT_MESSAGE,
        path: CODEX_REF_PATH,
        package_version: CODEX_PACKAGE_VERSION,
        release_tag: CODEX_RELEASE_TAG,
        release_commit: CODEX_RELEASE_COMMIT,
        originator: DEFAULT_ORIGINATOR,
        oauth_client_id: OAUTH_CLIENT_ID,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_is_pinned() {
        assert_eq!(CODEX_REF_COMMIT.len(), 40);
        assert_eq!(CODEX_PACKAGE_VERSION, "0.154.0");
        assert_eq!(CODEX_RELEASE_TAG, "rust-v0.154.0");
        assert_eq!(DEFAULT_ORIGINATOR, "codex_cli_rs");
        assert_eq!(OAUTH_CLIENT_ID, "app_EMoamEEZ73f0CkXaXp7hrann");
    }
}
