use crate::rest::{
    error::ApiError,
    supplier_transfer::{Sub2apiAccount, credential, sub2api_record},
};
use codex2api_accounts::{
    AccountIdentity, AuthDotJson, HostRuntime, HttpFingerprint, TokenData, new_installation_id,
};
use codex2api_storage::{NewSupplierAccount, SupplierStatus, SupplierTransferAccount};
use serde_json::{Value, json};

pub(crate) fn to_sub2api(item: &SupplierTransferAccount) -> Result<Value, ApiError> {
    let tokens = item
        .tokens
        .as_ref()
        .ok_or_else(|| ApiError::bad("未授权账户无法导出为 Sub2api 格式"))?;
    let auth = AuthDotJson::from_supplier_tokens(tokens)
        .map_err(|_| ApiError::bad("ChatGPT 凭据格式无效"))?;
    let data = auth
        .tokens
        .filter(|t| !t.access_token.is_empty())
        .ok_or_else(|| ApiError::bad("未授权账户无法导出为 Sub2api 格式"))?;
    let mut value = json!({"access_token":data.access_token,"refresh_token":data.refresh_token,"id_token":data.id_token,
        "chatgpt_account_id":item.account.chatgpt_account_id,"chatgpt_user_id":item.account.chatgpt_user_id,
        "email":item.account.email,"plan_type":item.account.plan_type});
    if let Ok(Some(expires)) = codex2api_auth::parse_jwt_expiration(&data.access_token) {
        value["expires_at"] = expires.to_rfc3339().into();
    }
    value
        .as_object_mut()
        .expect("credential object")
        .retain(|_, v| !v.is_null());
    Ok(value)
}

pub(crate) fn from_sub2api(source: &Sub2apiAccount) -> Result<SupplierTransferAccount, ApiError> {
    let c = &source.credentials;
    let access =
        credential(c, "access_token").ok_or_else(|| ApiError::bad("缺少 ChatGPT Access Token"))?;
    let id_token = credential(c, "id_token").unwrap_or_default();
    let claims = codex2api_auth::parse_chatgpt_jwt_claims(&id_token)
        .ok()
        .or_else(|| codex2api_auth::parse_chatgpt_jwt_claims(&access).ok());
    let owner = credential(c, "chatgpt_user_id")
        .or_else(|| claims.as_ref().and_then(|c| c.chatgpt_user_id.clone()))
        .ok_or_else(|| ApiError::bad("缺少 ChatGPT 用户编号"))?;
    let workspace = credential(c, "chatgpt_account_id")
        .or_else(|| claims.as_ref().and_then(|c| c.chatgpt_account_id.clone()))
        .ok_or_else(|| ApiError::bad("缺少 ChatGPT 空间编号"))?;
    let identity = AccountIdentity::new(
        uuid::Uuid::new_v4().to_string(),
        new_installation_id(),
        HostRuntime::generate(),
    );
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
    account.id = Some(identity.account_id.clone());
    account.status = SupplierStatus::Active;
    account.display_name = Some(source.name.clone());
    account.chatgpt_account_id = Some(workspace.clone());
    account.chatgpt_user_id = Some(owner);
    account.email =
        credential(c, "email").or_else(|| claims.as_ref().and_then(|c| c.email.clone()));
    account.plan_type = credential(c, "plan_type")
        .or_else(|| claims.as_ref().and_then(|c| c.chatgpt_plan_type.clone()));
    let auth = AuthDotJson::chatgpt(
        TokenData {
            access_token: access,
            id_token,
            refresh_token: credential(c, "refresh_token").unwrap_or_default(),
            account_id: Some(workspace),
        },
        None,
    );
    sub2api_record(
        source,
        account,
        auth.to_supplier_tokens(&identity.account_id),
    )
}

pub(crate) fn normalize(item: &mut SupplierTransferAccount) -> Result<(), ApiError> {
    let a = &mut item.account;
    let mut fingerprint = HttpFingerprint::from_json(&a.http_fingerprint_json)
        .map_err(|_| ApiError::bad("ChatGPT 指纹无效"))?;
    let terminal = fingerprint
        .user_agent
        .split_once(") ")
        .map(|(_, t)| t)
        .filter(|t| !t.trim().is_empty() && t.len() <= 128 && !t.chars().any(char::is_control))
        .ok_or_else(|| ApiError::bad("ChatGPT 终端指纹无效"))?;
    a.originator = codex2api_version::DEFAULT_ORIGINATOR.into();
    a.user_agent =
        codex2api_version::official_user_agent(&a.os_type, &a.os_version, &a.arch, terminal);
    fingerprint.originator = a.originator.clone();
    fingerprint.user_agent = a.user_agent.clone();
    fingerprint.os_type = a.os_type.clone();
    fingerprint.os_version = a.os_version.clone();
    fingerprint.arch = a.arch.clone();
    fingerprint.installation_id = a.installation_id.clone();
    a.http_fingerprint_json = fingerprint
        .to_json()
        .map_err(|_| ApiError::bad("ChatGPT 指纹无效"))?;
    if let Some(tokens) = &item.tokens {
        let mut auth = AuthDotJson::from_supplier_tokens(tokens)
            .map_err(|_| ApiError::bad("ChatGPT 凭据格式无效"))?;
        let data = auth
            .tokens
            .as_mut()
            .filter(|t| !t.access_token.trim().is_empty())
            .ok_or_else(|| ApiError::bad("缺少 ChatGPT Access Token"))?;
        if auth.auth_mode.as_deref().is_some_and(|m| m != "chatgpt") {
            return Err(ApiError::bad("不支持的 ChatGPT 授权类型"));
        }
        let owner = a
            .chatgpt_user_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ApiError::bad("缺少 ChatGPT 用户编号"))?;
        let workspace = a
            .chatgpt_account_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ApiError::bad("缺少 ChatGPT 空间编号"))?;
        if data.account_id.as_deref().is_some_and(|s| s != workspace) {
            return Err(ApiError::bad("ChatGPT 凭据与账户身份不一致"));
        }
        for token in [&data.id_token, &data.access_token] {
            if let Ok(claims) = codex2api_auth::parse_chatgpt_jwt_claims(token)
                && (claims
                    .chatgpt_account_id
                    .as_deref()
                    .is_some_and(|s| s != workspace)
                    || claims
                        .chatgpt_user_id
                        .as_deref()
                        .is_some_and(|s| s != owner))
            {
                return Err(ApiError::bad("ChatGPT 凭据与账户身份不一致"));
            }
        }
        data.account_id = Some(workspace.into());
        auth.auth_mode = Some("chatgpt".into());
        item.tokens = Some(auth.to_supplier_tokens(a.id.as_deref().unwrap_or_default()));
    }
    Ok(())
}
