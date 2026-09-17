use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

/// Official-looking Codex CLI runtime fields for one account.
///
/// `originator` and the UA version token are official constants. OS / arch /
/// version / terminal are chosen once per account, persisted, and never
/// regenerated. They are not read from the proxy host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRuntime {
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
}

/// One realistic `(os_info Type, Version, arch, terminal-detection token)` tuple.
///
/// `os_type` / `os_version` / `arch` match `os_info` Display output used by
/// official `get_codex_user_agent`. Terminal tokens match official
/// `codex_terminal_detection::user_agent()`.
struct RuntimeProfile {
    os_type: &'static str,
    os_version: &'static str,
    arch: &'static str,
    terminal: &'static str,
}

const RUNTIME_PROFILES: &[RuntimeProfile] = &[
    RuntimeProfile {
        os_type: "Windows",
        os_version: "10.0.19045",
        arch: "x86_64",
        terminal: "WindowsTerminal",
    },
    RuntimeProfile {
        os_type: "Windows",
        os_version: "10.0.22631",
        arch: "x86_64",
        terminal: "WindowsTerminal",
    },
    RuntimeProfile {
        os_type: "Windows",
        os_version: "10.0.26100",
        arch: "x86_64",
        terminal: "vscode",
    },
    RuntimeProfile {
        os_type: "Windows",
        os_version: "10.0.26200",
        arch: "x86_64",
        terminal: "WindowsTerminal",
    },
    RuntimeProfile {
        os_type: "Windows",
        os_version: "10.0.26100",
        arch: "aarch64",
        terminal: "WindowsTerminal",
    },
    RuntimeProfile {
        os_type: "Mac OS",
        os_version: "14.7.2",
        arch: "aarch64",
        terminal: "iTerm.app/3.5.11",
    },
    RuntimeProfile {
        os_type: "Mac OS",
        os_version: "15.3.1",
        arch: "aarch64",
        terminal: "Ghostty",
    },
    RuntimeProfile {
        os_type: "Mac OS",
        os_version: "15.6.0",
        arch: "aarch64",
        terminal: "Apple_Terminal",
    },
    RuntimeProfile {
        os_type: "Mac OS",
        os_version: "14.6.1",
        arch: "x86_64",
        terminal: "iTerm.app/3.4.23",
    },
    RuntimeProfile {
        os_type: "Mac OS",
        os_version: "15.2.0",
        arch: "aarch64",
        terminal: "vscode",
    },
    RuntimeProfile {
        os_type: "Ubuntu",
        os_version: "22.4.0",
        arch: "x86_64",
        terminal: "gnome-terminal",
    },
    RuntimeProfile {
        os_type: "Ubuntu",
        os_version: "24.4.0",
        arch: "x86_64",
        terminal: "vscode",
    },
    RuntimeProfile {
        os_type: "Debian",
        os_version: "12.0.0",
        arch: "x86_64",
        terminal: "kitty",
    },
    RuntimeProfile {
        os_type: "Fedora",
        os_version: "41.0.0",
        arch: "x86_64",
        terminal: "WezTerm",
    },
    RuntimeProfile {
        os_type: "Arch",
        os_version: "rolling",
        arch: "x86_64",
        terminal: "Alacritty",
    },
    RuntimeProfile {
        os_type: "Ubuntu",
        os_version: "24.4.0",
        arch: "aarch64",
        terminal: "tmux",
    },
];

impl HostRuntime {
    /// Pick a random official-looking OS/terminal profile for a new account.
    pub fn generate() -> Self {
        let profile = &RUNTIME_PROFILES[rand::rng().random_range(0..RUNTIME_PROFILES.len())];
        Self::from_profile(profile)
    }

    fn from_profile(profile: &RuntimeProfile) -> Self {
        let originator = codex2api_version::DEFAULT_ORIGINATOR.to_string();
        let user_agent = sanitize_user_agent(codex2api_version::official_user_agent(
            profile.os_type,
            profile.os_version,
            profile.arch,
            profile.terminal,
        ));
        Self {
            originator,
            user_agent,
            os_type: profile.os_type.to_string(),
            os_version: profile.os_version.to_string(),
            arch: profile.arch.to_string(),
        }
    }
}

/// Official Codex CLI application-layer HTTP fingerprint for one account.
///
/// Matches logged-in CLI request identity:
/// `originator`, `User-Agent`, `x-codex-installation-id`, plus this account's
/// cookie jar. OS/terminal in the UA are rolled once at account creation and
/// frozen in SQLite. Re-login never regenerates them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpFingerprint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub installation_id: String,
    #[serde(default)]
    pub cookies: Vec<String>,
}

impl HttpFingerprint {
    pub fn from_runtime(runtime: &HostRuntime, installation_id: impl Into<String>) -> Self {
        Self {
            originator: runtime.originator.clone(),
            timezone: None,
            user_agent: runtime.user_agent.clone(),
            os_type: runtime.os_type.clone(),
            os_version: runtime.os_version.clone(),
            arch: runtime.arch.clone(),
            installation_id: installation_id.into(),
            cookies: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "{}" {
            return Err(crate::error::AccountError::InvalidHttpFingerprint);
        }
        Ok(serde_json::from_str(trimmed)?)
    }
}

/// Isolated Codex request identity for one upstream account.
///
/// Persisted on the `accounts` SQLite row. Upstream headers are built from the
/// frozen [`HttpFingerprint`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountIdentity {
    pub account_id: String,
    pub installation_id: String,
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub http_fingerprint: HttpFingerprint,
}

impl AccountIdentity {
    pub fn new(
        account_id: impl Into<String>,
        installation_id: impl Into<String>,
        runtime: HostRuntime,
    ) -> Self {
        let installation_id = installation_id.into();
        let http_fingerprint = HttpFingerprint::from_runtime(&runtime, installation_id.clone());
        Self {
            account_id: account_id.into(),
            installation_id,
            originator: runtime.originator,
            user_agent: runtime.user_agent,
            os_type: runtime.os_type,
            os_version: runtime.os_version,
            arch: runtime.arch,
            http_fingerprint,
        }
    }

    pub fn from_account(account: &codex2api_storage::Account) -> Self {
        let http_fingerprint = HttpFingerprint::from_json(&account.http_fingerprint_json)
            .unwrap_or_else(|_| HttpFingerprint {
                timezone: None,
                originator: account.originator.clone(),
                user_agent: account.user_agent.clone(),
                os_type: account.os_type.clone(),
                os_version: account.os_version.clone(),
                arch: account.arch.clone(),
                installation_id: account.installation_id.clone(),
                cookies: Vec::new(),
            });
        Self {
            account_id: account.id.clone(),
            installation_id: account.installation_id.clone(),
            originator: http_fingerprint.originator.clone(),
            user_agent: http_fingerprint.user_agent.clone(),
            os_type: http_fingerprint.os_type.clone(),
            os_version: http_fingerprint.os_version.clone(),
            arch: http_fingerprint.arch.clone(),
            http_fingerprint,
        }
    }

    pub fn runtime(&self) -> HostRuntime {
        HostRuntime {
            originator: self.originator.clone(),
            user_agent: self.user_agent.clone(),
            os_type: self.os_type.clone(),
            os_version: self.os_version.clone(),
            arch: self.arch.clone(),
        }
    }

    pub fn fingerprint_json(&self) -> Result<String> {
        self.http_fingerprint.to_json()
    }

    /// Keep the persisted OS/terminal while aligning the product/version prefix.
    pub fn official_user_agent(&self) -> String {
        let stored = if self.http_fingerprint.user_agent.is_empty() {
            &self.user_agent
        } else {
            &self.http_fingerprint.user_agent
        };
        let terminal = stored
            .split_once(") ")
            .map(|(_, terminal)| terminal)
            .unwrap_or("unknown");
        sanitize_user_agent(codex2api_version::official_user_agent(
            &self.os_type,
            &self.os_version,
            &self.arch,
            terminal,
        ))
    }
}

/// Generate a new official-style installation_id (UUID v4, canonical hyphenated).
pub fn new_installation_id() -> String {
    Uuid::new_v4().to_string()
}

/// Canonicalize an existing installation_id. Invalid values are rejected so a
/// bound account never silently rotates to a new UUID.
pub fn canonicalize_installation_id(id: &str) -> Result<String> {
    let parsed = Uuid::parse_str(id.trim())
        .map_err(|_| crate::error::AccountError::InvalidInstallationId(id.to_string()))?;
    Ok(parsed.to_string())
}

fn sanitize_user_agent(candidate: String) -> String {
    if candidate.chars().all(|ch| matches!(ch, ' '..='~')) {
        return candidate;
    }
    candidate
        .chars()
        .map(|ch| if matches!(ch, ' '..='~') { ch } else { '_' })
        .collect()
}
