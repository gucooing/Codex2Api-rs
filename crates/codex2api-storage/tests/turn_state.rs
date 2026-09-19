use codex2api_storage::{
    AccountStatus, AccountTokens, NewAccount, Storage, TurnStateObservation, TurnStateSettings,
};

async fn account(storage: &Storage, id: &str) {
    let mut new =
        NewAccount::pending_identity(id, "codex_cli_rs", "test", "test", "test", "test", "", "{}");
    new.id = Some(id.into());
    new.chatgpt_account_id = Some(format!("owner-{id}"));
    new.status = AccountStatus::Active;
    storage.create_account(new).await.unwrap();
}
fn observation(token: Option<&str>, issued_at: i64, from_client: bool) -> TurnStateObservation<'_> {
    TurnStateObservation {
        from_client,
        token,
        issued_at,
        now: issued_at,
        status: if from_client { 0 } else { 200 },
        result: if token.is_some() {
            "accepted"
        } else {
            "missing_header"
        },
        length: token.map_or(0, |t| t.len() as i64),
        blocks: if token.is_some() { 10 } else { 0 },
        injected: false,
    }
}

#[tokio::test]
async fn natural_state_is_isolated_persistent_and_monotonic_under_concurrent_captures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    for id in ["a", "b"] {
        account(&storage, id).await;
    }
    let settings = TurnStateSettings {
        enabled: true,
        ..Default::default()
    };
    for id in ["a", "b"] {
        storage
            .save_turn_state_settings(id, &settings)
            .await
            .unwrap();
    }
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
    let (a, b) = tokio::join!(
        storage.record_turn_state_observation(
            &entry,
            &settings,
            observation(Some("new-state"), 200, false)
        ),
        storage.record_turn_state_observation(
            &entry,
            &settings,
            observation(Some("older-state"), 100, false)
        )
    );
    a.unwrap();
    b.unwrap();
    storage
        .record_turn_state_observation(&entry, &settings, observation(None, 300, false))
        .await
        .unwrap();
    assert!(storage.turn_state_entries("b").await.unwrap().is_empty());
    assert!(
        storage
            .turn_state_entry("a", "other", "owner-a", &rev)
            .await
            .unwrap()
            .is_none()
    );
    let reopened = Storage::open(&path).await.unwrap();
    let saved = reopened
        .turn_state_entry("a", "model", "owner-a", &rev)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.token.as_deref(), Some("new-state"));
    assert_eq!(saved.source, "response");
    assert_eq!(saved.response_count, 3);
    assert_eq!(saved.response_result, "missing_header");
    assert_eq!(saved.expires_at, 3770);
    assert!((1..=2).contains(&saved.response_captures));
}

#[tokio::test]
async fn clear_reconfigure_and_reauthorize_reject_inflight_observations() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("state.sqlite"))
        .await
        .unwrap();
    account(&storage, "a").await;
    let settings = TurnStateSettings {
        enabled: true,
        ..Default::default()
    };
    storage
        .save_turn_state_settings("a", &settings)
        .await
        .unwrap();
    for action in ["clear", "settings", "auth", "identity"] {
        let (_, revision) = storage.turn_state_settings("a").await.unwrap();
        storage
            .ensure_turn_state_entry("a", "model", "owner-a", &revision)
            .await
            .unwrap();
        let entry = storage
            .turn_state_entry("a", "model", "owner-a", &revision)
            .await
            .unwrap()
            .unwrap();
        storage
            .record_turn_state_observation(
                &entry,
                &settings,
                observation(Some("client-state"), 200, true),
            )
            .await
            .unwrap();
        match action {
            "clear" => storage.clear_turn_state("a").await.unwrap(),
            "settings" => storage
                .save_turn_state_settings("a", &settings)
                .await
                .unwrap(),
            "auth" => {
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
                        access_token: Some("changed".into()),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
            }
            "identity" => {
                sqlx::query("UPDATE accounts SET chatgpt_account_id='new-owner' WHERE id='a'")
                    .execute(storage.pool())
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert_ne!(storage.turn_state_settings("a").await.unwrap().1, revision);
        storage
            .record_turn_state_observation(
                &entry,
                &settings,
                observation(Some("late-state"), 400, false),
            )
            .await
            .unwrap();
        storage
            .ensure_turn_state_entry("a", "model", "owner-a", &revision)
            .await
            .unwrap();
        assert!(storage.turn_state_entries("a").await.unwrap().is_empty());
    }
    storage.delete_account("a").await.unwrap();
    assert_eq!(storage.turn_state_settings("a").await.unwrap().1, "");
}

#[test]
fn settings_accept_legacy_json_but_remove_probe_configuration_on_save() {
    let settings: TurnStateSettings = serde_json::from_str(
        r#"{"enabled":true,"models":["gpt-6-astra"],"ttl":3600,"renew":600,"cooldown":300}"#,
    )
    .unwrap();
    assert!(settings.enabled);
    assert!(settings.validate().is_ok());
    let saved = serde_json::to_value(settings).unwrap();
    assert!(saved.get("renew").is_none());
    assert!(saved.get("cooldown").is_none());
    for settings in [
        TurnStateSettings {
            ttl: i64::MAX,
            ..Default::default()
        },
        TurnStateSettings {
            ttl: 119,
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
