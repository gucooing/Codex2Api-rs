use crate::rest::{
    error::ApiError,
    supplier_transfer::{Sub2apiAccount, credential, sub2api_record},
};
use codex2api_common::version::antigravity as wire;
use codex2api_storage::{NewSupplierAccount, SupplierStatus, SupplierTransferAccount};
use codex2api_suppliers::antigravity::{AntigravityCredentials, AntigravityIdentity};
use serde_json::{Value, json};

pub(crate) fn to_sub2api(item: &SupplierTransferAccount) -> Result<Value, ApiError> {
    let tokens = item
        .tokens
        .as_ref()
        .ok_or_else(|| ApiError::bad("未授权账户无法导出为 Sub2api 格式"))?;
    let auth = AntigravityCredentials::from_supplier_tokens(tokens)
        .map_err(|_| ApiError::bad("Antigravity 凭据格式无效"))?;
    if auth.access_token.is_empty() {
        return Err(ApiError::bad("未授权账户无法导出为 Sub2api 格式"));
    }
    let mut value = json!({"access_token":auth.access_token,"refresh_token":auth.refresh_token,
        "expires_at":auth.expires_at.map(|t| t.to_string()),"token_type":"Bearer",
        "email":item.account.email,"project_id":auth.project_id,
        "plan_type":auth.tier.as_deref().map(wire::tier_display)});
    value
        .as_object_mut()
        .expect("credential object")
        .retain(|_, v| !v.is_null());
    Ok(value)
}

pub(crate) fn from_sub2api(source: &Sub2apiAccount) -> Result<SupplierTransferAccount, ApiError> {
    let c = &source.credentials;
    let access = credential(c, "access_token")
        .ok_or_else(|| ApiError::bad("缺少 Antigravity Access Token"))?;
    let email = credential(c, "email")
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| e.contains('@'))
        .ok_or_else(|| ApiError::bad("缺少 Antigravity 账户邮箱"))?;
    let expires_at = match c.get("expires_at").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(
            v.as_i64()
                .or_else(|| {
                    v.as_str().and_then(|s| {
                        s.parse::<i64>().ok().or_else(|| {
                            chrono::DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|t| t.timestamp())
                        })
                    })
                })
                .ok_or_else(|| ApiError::bad("Antigravity 凭据到期时间无效"))?,
        ),
    };
    let tier = credential(c, "plan_type").map(|plan| match plan.to_ascii_lowercase().as_str() {
        "pro" => wire::TIER_PRO.to_owned(),
        "ultra" => wire::TIER_ULTRA.to_owned(),
        "free" => wire::TIER_FREE.to_owned(),
        _ => plan,
    });
    let identity = AntigravityIdentity::generate();
    let mut account = NewSupplierAccount::pending_identity(
        &identity.installation_id,
        &identity.os_type,
        &identity.os_version,
        &identity.arch,
        identity
            .fingerprint_json()
            .map_err(|_| ApiError::bad("指纹无效"))?,
    );
    account.provider_id = codex2api_common::ANTIGRAVITY.into();
    account.id = Some(identity.account_id.clone());
    account.status = SupplierStatus::Active;
    account.display_name = Some(source.name.clone());
    account.chatgpt_account_id = Some(format!("google:{email}"));
    account.chatgpt_user_id = Some(email.clone());
    account.email = Some(email.clone());
    account.plan_type = tier.as_deref().map(|t| wire::tier_display(t).to_owned());
    let auth = AntigravityCredentials {
        access_token: access,
        refresh_token: credential(c, "refresh_token"),
        expires_at,
        project_id: credential(c, "project_id"),
        tier,
        profile: json!({"email":email}),
    };
    sub2api_record(
        source,
        account,
        auth.to_supplier_tokens(&identity.account_id),
    )
}

pub(crate) fn normalize(item: &mut SupplierTransferAccount) -> Result<(), ApiError> {
    let a = &mut item.account;
    let previous: AntigravityIdentity = serde_json::from_str(&a.http_fingerprint_json)
        .map_err(|_| ApiError::bad("Antigravity 指纹无效"))?;
    let identity = AntigravityIdentity::new(
        a.id.clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        a.installation_id.clone(),
        a.os_type.clone().unwrap_or_default(),
        a.os_version.clone().unwrap_or_default(),
        a.arch.clone().unwrap_or_default(),
        previous.timezone,
    );
    a.id = Some(identity.account_id.clone());
    a.http_fingerprint_json = identity
        .fingerprint_json()
        .map_err(|_| ApiError::bad("Antigravity 指纹无效"))?;
    if let Some(tokens) = &item.tokens {
        if tokens.auth_mode.as_deref() != Some(codex2api_common::ANTIGRAVITY) {
            return Err(ApiError::bad("Antigravity 凭据格式无效"));
        }
        let auth = AntigravityCredentials::from_supplier_tokens(tokens)
            .map_err(|_| ApiError::bad("Antigravity 凭据格式无效"))?;
        let email = a
            .chatgpt_user_id
            .as_deref()
            .filter(|e| e.contains('@'))
            .ok_or_else(|| ApiError::bad("缺少 Antigravity 账户邮箱"))?;
        if auth.access_token.trim().is_empty()
            || a.chatgpt_account_id.as_deref() != Some(&format!("google:{email}"))
        {
            return Err(ApiError::bad("Antigravity 凭据与账户身份不一致"));
        }
        let mut normalized = auth.to_supplier_tokens(&identity.account_id);
        normalized.last_refresh = tokens.last_refresh.clone();
        item.tokens = Some(normalized);
    }
    Ok(())
}
