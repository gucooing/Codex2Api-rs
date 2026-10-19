//! Pinned official Codex client version this proxy is implemented against.
//!
//! Official source is reference only. Communication with OpenAI/Codex servers
//! must match a Codex CLI that is logged in directly, for this snapshot.

use serde::Serialize;

/// Codex2API release tag captured at build time, independent of the official CLI version.
pub const APP_VERSION: &str = env!("CODEX2API_BUILD_TAG");

/// Official Codex git repository used as protocol reference.
pub const CODEX_REPO: &str = "https://github.com/openai/codex";

/// Official Git ref the local snapshot was taken from.
pub const CODEX_REF_BRANCH: &str = CODEX_RELEASE_TAG;

/// Exact official source commit used for the supplier contract.
pub const CODEX_REF_COMMIT: &str = "83086188ae0614bd882818ff1aa23f3194962012";

/// Commit timestamp (UTC).
pub const CODEX_REF_COMMIT_DATE: &str = "2026-10-15T18:57:07Z";

/// First-line commit message of the reference snapshot.
pub const CODEX_REF_COMMIT_MESSAGE: &str = "## New Features";

/// Local path of the official source snapshot in this repo.
pub const CODEX_REF_PATH: &str = "reference/codex";

/// Current official packaged CLI release this proxy pretends to be.
///
/// GitHub latest stable: https://github.com/openai/codex/releases/tag/rust-v0.163.0
/// (`CARGO_PKG_VERSION` stamped into that binary). Not the source-tree `0.0.0`.
pub const CODEX_RELEASE_VERSION: &str = "0.163.0";

/// GitHub release tag for [`CODEX_RELEASE_VERSION`].
pub const CODEX_RELEASE_TAG: &str = "rust-v0.163.0";

/// Commit the `rust-v0.163.0` tag points at.
pub const CODEX_RELEASE_COMMIT: &str = CODEX_REF_COMMIT;

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
/// Official OAuth access-token resource, distinct from the ID-token client audience.
pub const OAUTH_ACCESS_AUDIENCE: &str = "https://api.openai.com/v1";
/// Lifetimes observed in a signature-verified official Codex OAuth pair (2026-09-25).
pub const OAUTH_ACCESS_TOKEN_TTL: i64 = 864000;
pub const OAUTH_ID_TOKEN_TTL: i64 = 3600;

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

/// Shared supplier endpoint and protocol header definitions.
pub const CHATGPT_ORIGIN: &str = "https://chatgpt.com";
pub const CHATGPT_REFERER: &str = "https://chatgpt.com/";
pub const CHATGPT_WHAM_BASE_URL: &str = "https://chatgpt.com/backend-api/wham/";
pub const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const CHATGPT_DISCOVERY_URL: &str = "https://chatgpt.com/backend-api/wham/accounts/check";
pub const OPENAI_INPUT_TOKENS_URL: &str = "https://api.openai.com/v1/responses/input_tokens";
pub const HEADER_VERSION: &str = "version";
pub const HEADER_TURN_METADATA: &str = "x-codex-turn-metadata";
pub const HEADER_WINDOW_ID: &str = "x-codex-window-id";
pub const HEADER_TURN_STATE: &str = "x-codex-turn-state";
pub const HEADER_GUARDIAN: &str = "x-codex-guardian";
pub const HEADER_SUBAGENT: &str = "x-openai-subagent";
pub const HEADER_PARENT_THREAD_ID: &str = "x-codex-parent-thread-id";
pub const HEADER_ROUTING_OVERRIDE: &str = "x-openai-account-routing-override";
pub const HEADER_ROUTING_HINT: &str = "x-codex-routing-hint";
pub const HEADER_BETA_FEATURES: &str = "x-codex-beta-features";
pub const HEADER_RESPONSES_LITE: &str = "x-openai-internal-codex-responses-lite";
pub const HEADER_MEMGEN: &str = "x-openai-memgen-request";
pub const HEADER_TIMING_METRICS: &str = "x-responsesapi-include-timing-metrics";
pub const HEADER_INFERENCE_CALL_ID: &str = "x-codex-inference-call-id";
pub const HEADER_ATTESTATION: &str = "x-oai-attestation";
pub const HEADER_RESIDENCY: &str = "x-openai-internal-codex-residency";
pub const HEADER_FEDRAMP: &str = "x-openai-fedramp";
pub const HEADER_ORGANIZATION: &str = "openai-organization";
pub const HEADER_PROJECT: &str = "openai-project";
pub const HEADER_OPENAI_BETA: &str = "openai-beta";

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

pub const CODEX_PATH_RESPONSES: &str = "responses";
pub const CODEX_PATH_COMPACT: &str = "responses/compact";
pub const CODEX_PATH_INPUT_TOKENS: &str = "responses/input_tokens";
pub const CODEX_PATH_GUARDIAN: &str = "guardian";
pub const CODEX_PATH_GUARDIAN_CLASSIFIER: &str = "guardian-classifier";
pub const CODEX_PATH_MODELS: &str = "models";
pub const CODEX_PATH_USAGE: &str = "wham/usage";
pub const CODEX_PATH_MEMORY_SUMMARY: &str = "memories/trace_summarize";
pub const CODEX_PATH_SEARCH: &str = "alpha/search";
pub const CODEX_PATH_IMAGE_GENERATION: &str = "images/generations";
pub const CODEX_PATH_IMAGE_EDIT: &str = "images/edits";
pub const WHAM_PATH_USAGE: &str = "usage";
pub const WHAM_PATH_ACCOUNTS: &str = "accounts/check";
pub const WHAM_PATH_PROFILE: &str = "profiles/me";
pub const WHAM_PATH_CONFIG: &str = "config/bundle";
pub const WHAM_PATH_SETTINGS: &str = "settings/user";
pub const WHAM_PATH_CREDITS: &str = "rate-limit-reset-credits";
pub const WHAM_PATH_CONSUME_CREDIT: &str = "rate-limit-reset-credits/consume";
pub const WHAM_PATH_TASKS: &str = "tasks/list";
pub const WHAM_PATH_TASK: &str = "tasks/{task_id}";
pub const WHAM_PATH_SIBLING_TURNS: &str = "tasks/{task_id}/turns/{turn_id}/sibling_turns";
pub const WHAM_PATH_CREATE_TASK: &str = "tasks";
pub const WHAM_PATH_TASK_TURNS: &str = "tasks/{task_id}/turns";
pub const WHAM_PATH_TASK_TURN: &str = "tasks/{task_id}/turns/{turn_id}";
pub const WHAM_PATH_TASK_LOGS: &str = "tasks/{task_id}/turns/{turn_id}/logs";
pub const WHAM_PATH_CANCEL_TASK: &str = "tasks/{task_id}/cancel";
pub const WHAM_PATH_ARCHIVE_TASK: &str = "tasks/{task_id}/archive";
pub const WHAM_PATH_MESSAGES: &str = "workspace-messages";
pub const WHAM_PATH_THREAD_USAGE: &str = "usage/thread_usage/query";
pub const WHAM_PATH_TURN_ESTIMATES: &str = "usage/thread-estimates/query";
pub const CHATGPT_PATH_ACCOUNTS_CHECK: &str = "/backend-api/accounts/check/v4-2023-04-27";
pub const CHATGPT_PATH_SUBSCRIPTIONS: &str = "/backend-api/subscriptions";
pub const CHATGPT_PATH_PRIVACY: &str = "/backend-api/settings/account_user_setting";
pub const CHATGPT_PATH_FEATURED_PLUGINS: &str = "/backend-api/plugins/featured";
pub const CHATGPT_PATH_PLUGINS: &str = "/backend-api/ps/plugins/list";
pub const CHATGPT_PATH_INSTALLED_PLUGINS: &str = "/backend-api/ps/plugins/installed";
pub const CHATGPT_PATH_SUGGESTED_PLUGINS: &str = "/backend-api/ps/plugins/suggested/codex";
pub const CHATGPT_PATH_ANALYTICS_EVENTS: &str = "/backend-api/codex/analytics-events/events";
pub const CHATGPT_PATH_STATSIG_BOOTSTRAP: &str = "/backend-api/wham/statsig/bootstrap";
pub const CHATGPT_PATH_TRACES: &str = "/backend-api/o11y/v1/traces";
pub const CHATGPT_PATH_CONNECTOR_DIRECTORY: &str = "/backend-api/connectors/directory/list";
pub const CHATGPT_PATH_SITE_STATUS: &str = "/backend-api/aura/site_status";

pub const OAUTH_CALLBACK_PATH: &str = "/auth/callback";
pub const OAUTH_CANCEL_PATH: &str = "/cancel";
pub const OAUTH_AUTHORIZE_PATH: &str = "/oauth/authorize";
pub const OAUTH_TOKEN_PATH: &str = "/oauth/token";
pub const OAUTH_REVOKE_PATH: &str = "/oauth/revoke";
