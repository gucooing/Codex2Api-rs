use super::*;

#[test]
fn originator_and_release_version_are_official_constants() {
    let runtime = HostRuntime::generate();
    assert_eq!(runtime.originator, "codex_cli_rs");
    assert!(runtime.user_agent.starts_with("codex_cli_rs/0.157.0 "));
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
    assert!(fp.user_agent.starts_with("codex_cli_rs/0.157.0 "));
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

#[tokio::test]
async fn upgrading_stored_user_agents_preserves_identity_and_is_idempotent() {
    use codex2api_storage::{NewSupplierAccount, Storage};
    let path =
        std::env::temp_dir().join(format!("codex-ua-upgrade-{}.sqlite", uuid::Uuid::new_v4()));
    let storage = Storage::open(&path).await.unwrap();
    let mut identity =
        AccountIdentity::new("fixture", new_installation_id(), HostRuntime::generate());
    let old_ua =
        identity
            .user_agent
            .replacen(codex2api_version::CODEX_PACKAGE_VERSION, "0.156.1", 1);
    identity.http_fingerprint.user_agent = old_ua.clone();
    identity.http_fingerprint.timezone = Some("Asia/Taipei".into());
    let mut fingerprint: serde_json::Value =
        serde_json::from_str(&identity.fingerprint_json().unwrap()).unwrap();
    fingerprint["future_field"] = serde_json::json!({"preserved":true});
    let account = storage
        .create_account(NewSupplierAccount::pending_identity(
            &identity.installation_id,
            &identity.originator,
            &old_ua,
            &identity.os_type,
            &identity.os_version,
            &identity.arch,
            "",
            fingerprint.to_string(),
        ))
        .await
        .unwrap();
    let store = SupplierAccountStore::open(storage.clone());
    store.align_user_agents().await.unwrap();
    let aligned = storage.require_account(&account.id).await.unwrap();
    let expected = identity.official_user_agent();
    assert_eq!(aligned.user_agent, expected);
    fingerprint["user_agent"] = expected.into();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&aligned.http_fingerprint_json).unwrap(),
        fingerprint
    );
    assert_eq!(aligned.installation_id, account.installation_id);
    assert_eq!(
        (&aligned.os_type, &aligned.os_version, &aligned.arch),
        (&account.os_type, &account.os_version, &account.arch)
    );
    store.align_user_agents().await.unwrap();
    let repeated = storage.require_account(&account.id).await.unwrap();
    assert_eq!(repeated.updated_at, aligned.updated_at);
    assert_eq!(
        repeated.http_fingerprint_json,
        aligned.http_fingerprint_json
    );
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(
        reopened
            .require_account(&account.id)
            .await
            .unwrap()
            .user_agent,
        aligned.user_agent
    );
    reopened.close().await;
    drop(reopened);
    drop(store);
    drop(storage);
    // Windows may release SQLite's worker handle just after pool close returns.
    for attempt in 0..10 {
        match std::fs::remove_file(&path) {
            Ok(()) => break,
            Err(error) if attempt < 9 && error.raw_os_error() == Some(32) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(error) => panic!("remove fixture database: {error}"),
        }
    }
}
