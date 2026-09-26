//! Populate an explicitly selected disposable database for local UI review.
use codex2api_storage::{Storage, VirtualAccount, hash_password};
use serde_json::json;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Pass the disposable review database path"))?;
    anyhow::ensure!(
        path.replace('\\', "/").contains("/target/review-next-"),
        "Only target/review-next-* databases may be seeded"
    );
    let storage = Storage::open(path).await?;
    // Explicit local fixtures: no official authorization or upstream usage is implied.
    for (id, label, status, error) in [
        (
            "review-supplier-active",
            "本地验收供应账户",
            codex2api_storage::SupplierStatus::Active,
            false,
        ),
        (
            "review-supplier-disabled",
            "本地验收停用账户",
            codex2api_storage::SupplierStatus::Disabled,
            false,
        ),
        (
            "review-supplier-error",
            "本地验收错误账户",
            codex2api_storage::SupplierStatus::Active,
            true,
        ),
    ] {
        if storage.get_account(id).await?.is_none() {
            let identity = codex2api_accounts::AccountIdentity::new(
                id,
                codex2api_accounts::new_installation_id(),
                codex2api_accounts::HostRuntime::generate(),
            );
            let mut account = codex2api_storage::NewSupplierAccount::pending_identity(
                identity.installation_id.clone(),
                identity.originator.clone(),
                identity.official_user_agent(),
                identity.os_type.clone(),
                identity.os_version.clone(),
                identity.arch.clone(),
                "",
                identity.http_fingerprint.to_json()?,
            );
            account.id = Some(id.into());
            account.status = status;
            account.display_name = Some(label.into());
            account.email = Some(format!("{id}@example.test"));
            account.plan_type = Some("pro".into());
            storage.create_account(account).await?;
            storage
                .upsert_supplier_tokens(codex2api_storage::SupplierTokens {
                    account_id: id.into(),
                    access_token: Some("local-review-fixture".into()),
                    ..Default::default()
                })
                .await?;
        }
        let limits = if id == "review-supplier-disabled" {
            json!({"primary_window":{"used_percent":0,"limit_window_seconds":2592000,"reset_after_seconds":2590527},"secondary_window":null})
        } else {
            json!({"primary_window":{"used_percent":18,"limit_window_seconds":18000,"reset_at":chrono::Utc::now().timestamp()+7200},"secondary_window":{"used_percent":47,"limit_window_seconds":604800,"reset_at":chrono::Utc::now().timestamp()+266400}})
        };
        storage
            .store_account_quota(
                id,
                &codex2api_storage::QuotaSnapshot {
                    observed_at: chrono::Utc::now(),
                    value: json!({"rate_limit":limits}),
                },
            )
            .await?;
        if error {
            storage
                .record_supplier_error(id, "本地验收记录：ChatGPT 官方通信失败（HTTP 503）")
                .await?;
        }
    }
    let account = VirtualAccount {
        id: "review-consumer".into(),
        provider_id: "chatgpt".into(),
        username: "review-consumer".into(),
        password_hash: hash_password("review-local-password")?,
        name: "本地验收账户".into(),
        email: "review@example.test".into(),
        plan_id: "plus".into(),
        plan_type: "plus".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage
        .save_virtual_account_operation(&account, "admin")
        .await?;
    // One priced day exercises monthly cycle totals and sparse chart bar widths.
    let cycle_price = codex2api_storage::ModelPrice {
        provider_id: "chatgpt".into(),
        model: "review-cycle-model".into(),
        tier: "standard".into(),
        min_input_tokens: 0,
        input_rate: 2_000_000,
        cached_rate: 500_000,
        cache_write_rate: 2_000_000,
        output_rate: 4_000_000,
        source: "custom".into(),
        revision: 0,
    };
    storage.save_model_price(&cycle_price).await?;
    let mut cycle_record = codex2api_storage::UsageRecord {
        id: "review-cycle-usage".into(),
        account_id: "review-supplier-disabled".into(),
        account_name: "本地验收停用账户".into(),
        subject_id: account.id.clone(),
        subject_name: "本地周期用量验收".into(),
        model: Some(cycle_price.model.clone()),
        endpoint: "/v1/responses".into(),
        transport: "http".into(),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        status: "in_progress".into(),
        ..Default::default()
    };
    sqlx::query("DELETE FROM usage_records WHERE id=?")
        .bind(&cycle_record.id)
        .execute(storage.pool())
        .await?;
    storage.insert_usage(&cycle_record).await?;
    cycle_record.status = "completed".into();
    cycle_record.input_tokens = Some(3_000_000);
    cycle_record.cached_tokens = Some(1_000_000);
    cycle_record.output_tokens = Some(800_000);
    cycle_record.reasoning_tokens = Some(200_000);
    storage.finish_usage(&cycle_record).await?;
    for (key, value) in [
        (
            "cloud_preferences",
            json!({"branch_format":"codex/{task_id}","git_diff_mode":"unified"}),
        ),
        (
            "browser_settings",
            json!({"preferences":{"approval_mode":"always_ask","history_approval_mode":"always_ask","download_approval_mode":"always_ask","upload_approval_mode":"always_ask"},"rules":{"origin":{"https://example.test":"allow"},"download":{},"upload":{},"full_cdp":{}}}),
        ),
    ] {
        storage
            .save_virtual_client_state(&account.id, key, &value, None)
            .await?;
    }
    // A review fixture represents previously captured client data, not an admin mutation API.
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision,write_origin,updated_at_ms) VALUES(?, 'installed_plugins',?,1,'client',?) ON CONFLICT(virtual_account_id,state_key) DO UPDATE SET value_json=excluded.value_json").bind(&account.id).bind(json!({"plugins":[{"id":"review-plugin","name":"本地验收插件","status":"installed","description":"本地验收的客户端安装记录","version":"1.0.0"}]}).to_string()).bind(chrono::Utc::now().timestamp_millis()).execute(storage.pool()).await?;
    storage.save_virtual_resource(&account.id,"task","review-task",None,&json!({"task":{"id":"review-task","title":"本地验收记录（未调用上游）","status":"completed"}})).await?;
    storage
        .record_desktop_diagnostic(
            "review-diagnostic",
            "fixture",
            Some(&account.id),
            1,
            &json!([{"type":"review","count":1}]),
        )
        .await?;
    storage
        .save_desktop_resource(
            "/review-fixture.txt",
            b"Local review fixture",
            &json!({"content-type":"text/plain"}),
        )
        .await?;
    for (id, status, http_status, error) in [
        ("review-usage-model", "completed", 200, None),
        ("review-usage-client-stop", "client_stopped", 200, None),
        (
            "review-usage-failure",
            "failed",
            429,
            Some("本地验收：已达到上游用量限制，请稍后重试。"),
        ),
        (
            "review-usage-stream-failure",
            "failed",
            200,
            Some("本地验收：输入超出模型上下文限制。"),
        ),
    ] {
        let mut record = codex2api_storage::UsageRecord {
            id: id.into(),
            account_id: "review-supplier-active".into(),
            account_name: "本地验收供应账户".into(),
            subject_id: "review-consumer".into(),
            subject_name: id.into(),
            model: Some("gpt-6-astra".into()),
            actual_model: Some("gpt-5.6-luna".into()),
            upstream_request_id: Some(format!("req-{id}")),
            endpoint: "/v1/responses".into(),
            transport: "http".into(),
            requested_at_ms: chrono::Utc::now().timestamp_millis(),
            status: "in_progress".into(),
            ..Default::default()
        };
        sqlx::query("DELETE FROM usage_records WHERE id=?")
            .bind(id)
            .execute(storage.pool())
            .await?;
        storage.insert_usage(&record).await?;
        record.status = status.into();
        record.http_status = Some(http_status);
        record.first_byte_ms = Some(1569);
        record.total_ms = Some(145383);
        record.input_tokens = Some(24830);
        record.output_tokens = Some(10803);
        record.cached_tokens = Some(24320);
        record.cache_write_tokens = Some(256);
        record.reasoning_tokens = Some(2573);
        record.error_message = error.map(str::to_owned);
        record.error_code = error.map(|_| {
            if http_status == 429 {
                "rate_limit_exceeded"
            } else {
                "context_length_exceeded"
            }
            .into()
        });
        storage.finish_usage(&record).await?;
    }
    // Explicit local records for testing direct page jumps and last-page boundaries.
    for index in 0..105 {
        let id = format!("pagination-fixture-{index:03}");
        sqlx::query("INSERT OR IGNORE INTO model_catalog(provider_id,model,kind,enabled) VALUES('chatgpt',?,'text',0)")
            .bind(&id).execute(storage.pool()).await?;
        sqlx::query("DELETE FROM usage_records WHERE id=?")
            .bind(&id)
            .execute(storage.pool())
            .await?;
        let mut record = codex2api_storage::UsageRecord {
            id: id.clone(),
            subject_id: account.id.clone(),
            subject_name: id.clone(),
            account_id: "review-supplier-active".into(),
            account_name: "本地验收供应账户".into(),
            model: Some("pagination-fixture".into()),
            endpoint: "/v1/responses".into(),
            reasoning_effort: Some("xhigh".into()),
            service_tier: Some("default".into()),
            transport: "http".into(),
            status: "in_progress".into(),
            requested_at_ms: chrono::Utc::now().timestamp_millis() - 86400000 + index,
            ..Default::default()
        };
        storage.insert_usage(&record).await?;
        record.status = "completed".into();
        record.http_status = Some(200);
        record.input_tokens = Some(1200);
        record.output_tokens = Some(200);
        storage.finish_usage(&record).await?;
        storage
            .record_virtual_request(
                &account.id,
                "fixture",
                "GET",
                &format!("/pagination/{index:03}"),
                200,
                1,
            )
            .await?;
        storage
            .record_desktop_diagnostic(&id, "pagination-fixture", Some(&account.id), 1, &json!([]))
            .await?;
    }
    storage.close().await;
    println!("Seeded review-consumer in disposable database");
    Ok(())
}
