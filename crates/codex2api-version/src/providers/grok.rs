//! Grok Build wire constants, independent of the Codex baseline.
//! Reference: gucooing/grok-build, source revision recorded alongside its commit.
pub const REPOSITORY: &str = "https://github.com/gucooing/grok-build";
pub const COMMIT: &str = "2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8";
pub const SOURCE_REV: &str = "559751fdcec02d413e4c57c8832ab275e4f44980";
pub const VERSION: &str = "1.0.45";
pub const ISSUER: &str = "https://auth.x.ai";
pub const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
pub const BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";
pub const TOKEN_AUTH: &str = "xai-grok-cli";
pub const SCOPE: &str = "openid profile email offline_access grok-cli:access api:access conversations:read conversations:write workspaces:read workspaces:write";
pub const ACCESS_TOKEN_TTL: i64 = 3600;

pub fn user_agent(os: &str, arch: &str) -> String {
    let os = match os {
        "Windows" | "windows" => "windows",
        "Mac OS" | "macos" => "macos",
        _ => "linux",
    };
    format!("grok-pager/{VERSION} grok-shell/{VERSION} ({os}; {arch})")
}
