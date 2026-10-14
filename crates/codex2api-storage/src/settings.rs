use serde::{Deserialize, Serialize};

use crate::{Result, Storage};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UaMode {
    #[default]
    Blacklist,
    Whitelist,
}

/// Global inbound gateway policy, persisted independently of account fingerprints.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct GatewaySettings {
    pub ua_mode: UaMode,
    pub ua_rules: Vec<String>,
    pub scheduler_mode: String,
    pub sticky_weighted: bool,
    pub sticky_escape: bool,
    pub sticky_ttl_seconds: u64,
    pub pool_top_k: usize,
    pub pool_wait_ms: u64,
}

impl Default for GatewaySettings {
    fn default() -> Self {
        Self {
            ua_mode: UaMode::default(),
            ua_rules: Vec::new(),
            scheduler_mode: "advanced".into(),
            sticky_weighted: false,
            sticky_escape: true,
            sticky_ttl_seconds: 3600,
            pool_top_k: 7,
            pool_wait_ms: 10000,
        }
    }
}

impl GatewaySettings {
    pub fn from_lines(ua_mode: UaMode, rules: &str) -> Self {
        Self {
            ua_mode,
            ua_rules: rules
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            ..Self::default()
        }
    }

    /// Case-insensitive substring matching; `*` matches any sequence of characters.
    /// Empty blacklists allow all; empty whitelists allow none.
    pub fn allows_user_agent(&self, user_agent: &str) -> bool {
        let ua = user_agent.to_lowercase();
        let matches = |rule: &String| {
            let rule = rule.trim().to_lowercase();
            if rule.is_empty() {
                return false;
            }
            let mut remaining = ua.as_str();
            for part in rule.split('*').filter(|part| !part.is_empty()) {
                let Some(index) = remaining.find(part) else {
                    return false;
                };
                remaining = &remaining[index + part.len()..];
            }
            true
        };
        let matched = self.ua_rules.iter().any(matches);
        match self.ua_mode {
            UaMode::Blacklist => !matched,
            UaMode::Whitelist => matched,
        }
    }
}

impl Storage {
    pub async fn gateway_settings(&self) -> Result<GatewaySettings> {
        if let Some(value) = self.runtime.cached("gateway-settings") {
            return Ok(value);
        }
        let _fill = self.runtime.cache_fill("gateway-settings").await;
        if let Some(value) = self.runtime.cached("gateway-settings") {
            return Ok(value);
        }
        let epoch = self.runtime.epoch();
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta WHERE key = 'gateway_settings'")
                .fetch_optional(self.pool())
                .await?;
        let value = match value {
            Some(value) => serde_json::from_str(&value)?,
            None => GatewaySettings::default(),
        };
        self.runtime.affinity_ttl.store(
            value.sticky_ttl_seconds,
            std::sync::atomic::Ordering::Release,
        );
        self.runtime.cache(
            "gateway-settings".into(),
            &value,
            epoch,
            std::time::Duration::from_secs(30),
        );
        Ok(value)
    }

    pub async fn save_gateway_settings(&self, settings: &GatewaySettings) -> Result<()> {
        sqlx::query("INSERT INTO meta (key, value) VALUES ('gateway_settings', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(serde_json::to_string(settings)?)
            .execute(self.pool()).await?;
        self.runtime.affinity_ttl.store(
            settings.sticky_ttl_seconds,
            std::sync::atomic::Ordering::Release,
        );
        self.invalidate_runtime_cache();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_invert_multiple_fuzzy_rules() {
        for mode in [UaMode::Blacklist, UaMode::Whitelist] {
            let settings = GatewaySettings::from_lines(mode, " codex_cli \r\ntrusted*client\n\n");
            assert_eq!(settings.ua_rules.len(), 2);
            for ua in ["Codex_CLI_RS/0.154.0", "My Trusted desktop CLIENT/1.0"] {
                assert_eq!(
                    settings.allows_user_agent(ua),
                    mode == UaMode::Whitelist,
                    "{ua}"
                );
            }
            for ua in ["", "other-client", "client trusted", "trusted"] {
                assert_eq!(
                    settings.allows_user_agent(ua),
                    mode == UaMode::Blacklist,
                    "{ua}"
                );
            }
        }
        assert!(GatewaySettings::default().allows_user_agent(""));
        assert!(
            !GatewaySettings::from_lines(UaMode::Whitelist, "\n \n").allows_user_agent("anything")
        );
        assert!(!GatewaySettings::from_lines(UaMode::Blacklist, "*").allows_user_agent(""));
        assert!(
            GatewaySettings::from_lines(UaMode::Blacklist, "\n \n").allows_user_agent("anything")
        );
    }
}
