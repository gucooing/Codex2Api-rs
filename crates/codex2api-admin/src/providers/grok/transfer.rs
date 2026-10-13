use crate::rest::{
    error::ApiError,
    supplier_transfer::{Sub2apiAccount, credential, sub2api_record},
};
use codex2api_accounts::providers::grok::GrokIdentity;
use codex2api_auth::providers::grok::GrokCredentials;
use codex2api_storage::{NewSupplierAccount, SupplierStatus, SupplierTransferAccount};
use serde_json::{Value, json};

pub(crate) fn to_sub2api(item: &SupplierTransferAccount) -> Result<Value, ApiError> {
    let tokens = item
        .tokens
        .as_ref()
        .ok_or_else(|| ApiError::bad("未授权账户无法导出为 Sub2api 格式"))?;
    let auth = GrokCredentials::from_supplier_tokens(tokens)
        .map_err(|_| ApiError::bad("Grok 凭据格式无效"))?;
    if auth.access_token.is_empty() {
        return Err(ApiError::bad("未授权账户无法导出为 Sub2api 格式"));
    }
    let team = item
        .account
        .chatgpt_account_id
        .as_deref()
        .and_then(|id| id.strip_prefix("team:"));
    let mut value = json!({"access_token":auth.access_token,"refresh_token":auth.refresh_token,"id_token":auth.id_token,
        "expires_at":auth.expires_at,"sub":item.account.chatgpt_user_id,"team_id":team,"email":item.account.email,
        "subscription_tier":credential(&auth.profile, "subscriptionTier").or_else(||item.account.plan_type.clone()),
        "client_id":codex2api_version::grok::CLIENT_ID});
    value
        .as_object_mut()
        .expect("credential object")
        .retain(|_, v| !v.is_null());
    Ok(value)
}

pub(crate) fn from_sub2api(source: &Sub2apiAccount) -> Result<SupplierTransferAccount, ApiError> {
    let c = &source.credentials;
    let access =
        credential(c, "access_token").ok_or_else(|| ApiError::bad("缺少 Grok Access Token"))?;
    let user = credential(c, "sub").ok_or_else(|| ApiError::bad("缺少 Grok 用户编号"))?;
    let team = credential(c, "team_id");
    let email = credential(c, "email");
    let tier = credential(c, "subscription_tier");
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
                .ok_or_else(|| ApiError::bad("Grok 凭据到期时间无效"))?,
        ),
    };
    let identity = GrokIdentity::generate();
    let mut account = NewSupplierAccount::pending_identity(
        &identity.installation_id,
        &identity.originator,
        &identity.user_agent,
        &identity.os_type,
        &identity.os_version,
        &identity.arch,
        "",
        identity
            .fingerprint_json()
            .map_err(|_| ApiError::bad("指纹无效"))?,
    );
    account.provider_id = codex2api_core::GROK.into();
    account.id = Some(identity.account_id.clone());
    account.status = SupplierStatus::Active;
    account.display_name = Some(source.name.clone());
    account.chatgpt_account_id = Some(
        team.as_ref()
            .map_or_else(|| format!("user:{user}"), |t| format!("team:{t}")),
    );
    account.chatgpt_user_id = Some(user.clone());
    account.email = email.clone();
    account.plan_type = tier.clone();
    let auth = GrokCredentials {
        access_token: access,
        refresh_token: credential(c, "refresh_token"),
        id_token: credential(c, "id_token"),
        expires_at,
        profile: json!({"userId":user,"principalType":if team.is_some(){"Team"}else{"User"},
            "teamId":team,"email":email,"subscriptionTier":tier}),
    };
    sub2api_record(
        source,
        account,
        auth.to_supplier_tokens(&identity.account_id),
    )
}

pub(crate) fn normalize(item: &mut SupplierTransferAccount) -> Result<(), ApiError> {
    let a = &mut item.account;
    let previous: GrokIdentity = serde_json::from_str(&a.http_fingerprint_json)
        .map_err(|_| ApiError::bad("Grok 指纹无效"))?;
    let identity = GrokIdentity::new(
        a.id.clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        a.installation_id.clone(),
        a.os_type.clone(),
        a.os_version.clone(),
        a.arch.clone(),
        previous.timezone,
    );
    a.id = Some(identity.account_id.clone());
    a.originator = identity.originator.clone();
    a.user_agent = identity.user_agent.clone();
    a.http_fingerprint_json = identity
        .fingerprint_json()
        .map_err(|_| ApiError::bad("Grok 指纹无效"))?;
    if let Some(tokens) = &item.tokens {
        if tokens.auth_mode.as_deref() != Some("grok") {
            return Err(ApiError::bad("Grok 凭据格式无效"));
        }
        let auth = GrokCredentials::from_supplier_tokens(tokens)
            .map_err(|_| ApiError::bad("Grok 凭据格式无效"))?;
        let user = credential(&auth.profile, "userId")
            .ok_or_else(|| ApiError::bad("缺少 Grok 用户编号"))?;
        let principal = if auth.profile["principalType"] == "Team" {
            let team = credential(&auth.profile, "teamId")
                .or_else(|| credential(&auth.profile, "principalId"))
                .ok_or_else(|| ApiError::bad("缺少 Grok 团队编号"))?;
            format!("team:{team}")
        } else {
            format!("user:{user}")
        };
        if auth.access_token.trim().is_empty()
            || a.chatgpt_user_id.as_deref() != Some(&user)
            || a.chatgpt_account_id.as_deref() != Some(&principal)
        {
            return Err(ApiError::bad("Grok 凭据与账户身份不一致"));
        }
        let mut normalized = auth.to_supplier_tokens(&identity.account_id);
        normalized.last_refresh = tokens.last_refresh.clone();
        item.tokens = Some(normalized);
    }
    Ok(())
}
