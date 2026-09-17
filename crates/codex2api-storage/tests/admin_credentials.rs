use codex2api_storage::Storage;

#[tokio::test]
async fn credential_changes_are_atomic_and_allow_changing_either_field() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("admin.sqlite"))
        .await
        .unwrap();
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    // If revoking sessions fails, the credential change must roll back as well.
    sqlx::query("CREATE TRIGGER reject_session_delete BEFORE DELETE ON admin_sessions BEGIN SELECT RAISE(ABORT, 'test failure'); END")
        .execute(storage.pool()).await.unwrap();
    assert!(
        storage
            .change_admin_credentials("admin", "admin", "operator", "new-password")
            .await
            .is_err()
    );
    assert!(
        storage
            .verify_admin("admin", "admin")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .get_admin_session(&session.id)
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("DROP TRIGGER reject_session_delete")
        .execute(storage.pool())
        .await
        .unwrap();
    storage
        .change_admin_credentials("admin", "admin", "operator", "")
        .await
        .unwrap();
    assert!(
        storage
            .verify_admin("operator", "admin")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .get_admin_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    storage
        .change_admin_credentials("operator", "admin", "operator", " new-password ")
        .await
        .unwrap();
    assert!(
        storage
            .verify_admin("operator", "admin")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .verify_admin("operator", " new-password ")
            .await
            .unwrap()
            .is_some()
    );
    assert_ne!(
        storage.require_admin_user().await.unwrap().password_hash,
        " new-password "
    );
    assert!(
        storage
            .change_admin_credentials("operator", "admin", "stale-change", "anything")
            .await
            .is_err()
    );
    storage.close().await;
}
