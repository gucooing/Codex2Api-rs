use super::*;

#[test]
fn originator_and_release_version_are_official_constants() {
    let runtime = HostRuntime::generate();
    assert_eq!(runtime.originator, "codex_cli_rs");
    assert!(runtime.user_agent.starts_with("codex_cli_rs/0.156.1 "));
    assert!(runtime.user_agent.contains(&runtime.os_type));
    assert!(runtime.user_agent.contains(&runtime.arch));
    assert!(runtime.user_agent.contains(&runtime.os_version));
}

#[test]
fn frozen_fingerprint_does_not_change() {
    let id = new_installation_id();
    let identity = AccountIdentity::new("acct", id, HostRuntime::generate());
    let json = identity.fingerprint_json().unwrap();
    let loaded = HttpFingerprint::from_json(&json).unwrap();
    assert_eq!(loaded, identity.http_fingerprint);
}

#[test]
fn installation_id_is_uuid_v4() {
    let id = new_installation_id();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_eq!(canonicalize_installation_id(&id).unwrap(), id);
}

#[test]
fn http_fingerprint_matches_official_cli_headers() {
    let id = new_installation_id();
    let identity = AccountIdentity::new("acct", id.clone(), HostRuntime::generate());
    let fp = &identity.http_fingerprint;
    assert_eq!(fp.originator, "codex_cli_rs");
    assert_eq!(fp.installation_id, id);
    assert!(fp.user_agent.starts_with("codex_cli_rs/0.156.1 "));
    let json = identity.fingerprint_json().unwrap();
    let loaded = HttpFingerprint::from_json(&json).unwrap();
    assert_eq!(loaded, *fp);
}

#[test]
fn auth_json_roundtrip_via_tokens_shape() {
    let auth = AuthDotJson::chatgpt(
        TokenData {
            id_token: "id".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            account_id: Some("acct_1".into()),
        },
        None,
    );
    let tokens = auth.to_supplier_tokens("row-1");
    let loaded = AuthDotJson::from_supplier_tokens(&tokens).unwrap();
    assert_eq!(loaded.auth_mode.as_deref(), Some("chatgpt"));
    assert_eq!(loaded.chatgpt_account_id(), Some("acct_1"));
    assert_eq!(loaded.tokens.unwrap().access_token, "access");
}
