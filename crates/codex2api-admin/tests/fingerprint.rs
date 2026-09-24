mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn release_upgrade_updates_only_derived_user_agents_and_is_idempotent() {
    let f = Fixture::new().await;
    let a = f.state.accounts.create_pending().await.unwrap().account;
    let b = f.state.accounts.create_pending().await.unwrap().account;
    let old = a
        .user_agent
        .replace(codex2api_version::CODEX_PACKAGE_VERSION, "0.100.0");
    let mut fp: serde_json::Value = serde_json::from_str(&a.http_fingerprint_json).unwrap();
    fp["user_agent"] = old.clone().into();
    fp["future_identity_field"] = json!({"preserved":true});
    assert!(
        f.storage
            .align_supplier_user_agent(&a, &old, &fp.to_string())
            .await
            .unwrap()
    );
    let before = f.storage.require_account(&a.id).await.unwrap();
    f.state.accounts.align_user_agents().await.unwrap();
    let after = f.storage.require_account(&a.id).await.unwrap();
    fp["user_agent"] = a.user_agent.clone().into();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&after.http_fingerprint_json).unwrap(),
        fp
    );
    assert_eq!(after.user_agent, a.user_agent);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.os_type, before.os_type);
    assert_eq!(after.os_version, before.os_version);
    assert_eq!(after.arch, before.arch);
    assert_eq!(after.proxy_id, before.proxy_id);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(
        f.storage
            .require_account(&b.id)
            .await
            .unwrap()
            .http_fingerprint_json,
        b.http_fingerprint_json
    );
    f.state.accounts.align_user_agents().await.unwrap();
    assert_eq!(
        f.storage
            .require_account(&a.id)
            .await
            .unwrap()
            .http_fingerprint_json,
        after.http_fingerprint_json
    );
}
#[tokio::test]
async fn fingerprint_validation_persistence_and_transport_are_account_isolated() {
    let f = Fixture::new().await;
    let a = f.state.accounts.create_pending().await.unwrap().account;
    let b = f.state.accounts.create_pending().await.unwrap().account;
    let b_http = f.state.auth.account_http(&b.id).await.unwrap();
    let path = format!("/admin/api/suppliers/{}/fingerprint", a.id);
    let mut input = json!({"os_type":"Windows","os_version":"11","arch":"x86_64","terminal":"Windows Terminal/1","proxy_id":null,"timezone":"Asia/Taipei"});
    for field in ["os_type", "os_version", "arch", "terminal"] {
        let mut invalid = input.clone();
        invalid[field] = "bad\r\nheader".into();
        assert_eq!(
            f.request("PUT", &path, invalid).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    input["timezone"] = "bad/timezone".into();
    assert_eq!(
        f.request("PUT", &path, input.clone()).await.status(),
        StatusCode::BAD_REQUEST
    );
    input["timezone"] = "Asia/Taipei".into();
    assert_eq!(
        f.request("PUT", &path, input).await.status(),
        StatusCode::OK
    );
    let saved = f.storage.require_account(&a.id).await.unwrap();
    assert_eq!(saved.installation_id, a.installation_id);
    assert_eq!(saved.originator, codex2api_version::DEFAULT_ORIGINATOR);
    assert_eq!(saved.os_type, "Windows");
    assert!(saved.user_agent.contains("Windows Terminal/1"));
    assert!(std::sync::Arc::ptr_eq(
        &b_http,
        &f.state.auth.account_http(&b.id).await.unwrap()
    ));
    assert_eq!(
        f.storage.require_account(&b.id).await.unwrap().user_agent,
        b.user_agent
    );
}
