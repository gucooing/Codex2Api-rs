use codex2api_storage::{
    AccountStatus, AccountTokens, NewAccount, Storage, TurnStateProbeResult, TurnStateSettings,
};

async fn account(storage: &Storage, id: &str) {
    let mut new =
        NewAccount::pending_identity(id, "codex_cli_rs", "test", "test", "test", "test", "", "{}");
    new.id = Some(id.into());
    new.chatgpt_account_id = Some(format!("owner-{id}"));
    new.status = AccountStatus::Active;
    storage.create_account(new).await.unwrap();
}
fn success<'a>(token: &'a str, time: i64) -> TurnStateProbeResult<'a> {
    TurnStateProbeResult {
        token: Some(token),
        issued_at: time,
        expires_at: time + 3570,
        refresh_at: time + 3000,
        now: time,
        status: 200,
        result: "accepted",
        next_probe_at: time + 300,
    }
}

#[tokio::test]
async fn turn_state_isolation_leases_persistence_and_invalidation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    for id in ["a", "b"] {
        account(&storage, id).await;
    }
    assert!(!storage.turn_state_settings("a").await.unwrap().0.enabled);
    let config = TurnStateSettings {
        enabled: true,
        ..Default::default()
    };
    storage
        .save_turn_state_settings("a", &config)
        .await
        .unwrap();
    storage
        .save_turn_state_settings("b", &config)
        .await
        .unwrap();
    let (_, rev) = storage.turn_state_settings("a").await.unwrap();
    storage
        .ensure_turn_state_entry("a", "model", "owner-b", &rev)
        .await
        .unwrap();
    assert!(storage.turn_state_entries("a").await.unwrap().is_empty());
    storage
        .ensure_turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap();
    let entry = storage
        .turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap()
        .unwrap();
    let (first, second) = tokio::join!(
        storage.claim_turn_state_probe(&entry, 100, 300),
        storage.claim_turn_state_probe(&entry, 100, 300)
    );
    let leases: Vec<_> = [first.unwrap(), second.unwrap()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(leases.len(), 1);
    storage
        .finish_turn_state_probe(&entry, &leases[0], success("secret-a", 100))
        .await
        .unwrap();
    assert!(
        storage
            .turn_state_entry("a", "other-model", "owner-a", &rev)
            .await
            .unwrap()
            .is_none()
    );
    assert!(storage.turn_state_entries("b").await.unwrap().is_empty());
    let reopened = Storage::open(&path).await.unwrap();
    let entry = reopened
        .turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.token.as_deref(), Some("secret-a"));
    assert!(
        storage
            .claim_turn_state_probe(&entry, 200, 300)
            .await
            .unwrap()
            .is_none()
    );
    storage.record_turn_state_use(&entry, true).await.unwrap();
    storage.record_turn_state_use(&entry, false).await.unwrap();
    storage.observe_turn_state(&entry, false).await.unwrap();
    storage.observe_turn_state(&entry, false).await.unwrap();
    let entry = storage
        .turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (entry.injections, entry.client_states, entry.refresh_at),
        (1, 1, 0)
    );
    let lease = storage
        .claim_turn_state_probe(&entry, 500, 300)
        .await
        .unwrap()
        .unwrap();
    storage.clear_turn_state("a").await.unwrap();
    storage
        .finish_turn_state_probe(&entry, &lease, success("stale", 500))
        .await
        .unwrap();
    storage
        .ensure_turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap();
    assert!(storage.turn_state_entries("a").await.unwrap().is_empty());
    let (_, rev) = storage.turn_state_settings("a").await.unwrap();
    storage
        .ensure_turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap();
    storage
        .upsert_account_tokens(AccountTokens {
            account_id: "a".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    storage
        .upsert_account_tokens(AccountTokens {
            account_id: "a".into(),
            access_token: Some("new".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(storage.turn_state_entries("a").await.unwrap().is_empty());
    assert_ne!(storage.turn_state_settings("a").await.unwrap().1, rev);
    let (_, rev) = storage.turn_state_settings("a").await.unwrap();
    storage
        .ensure_turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap();
    storage
        .save_turn_state_settings("a", &TurnStateSettings::default())
        .await
        .unwrap();
    assert!(storage.turn_state_entries("a").await.unwrap().is_empty());
    storage.delete_account("a").await.unwrap();
    assert_eq!(storage.turn_state_settings("a").await.unwrap().1, "");
}

#[test]
fn turn_state_configuration_is_bounded() {
    assert!(TurnStateSettings::default().validate().is_ok());
    for settings in [
        TurnStateSettings {
            ttl: i64::MAX,
            ..Default::default()
        },
        TurnStateSettings {
            renew: 3600,
            ..Default::default()
        },
        TurnStateSettings {
            cooldown: 0,
            ..Default::default()
        },
        TurnStateSettings {
            models: vec!["<script>".into()],
            ..Default::default()
        },
        TurnStateSettings {
            models: vec![],
            ..Default::default()
        },
    ] {
        assert!(settings.validate().is_err());
    }
}
