//! Portable API-key accounts: sub2api's `apikey` rows of the `openai` and
//! `anthropic` platforms carry the same Base URL and key, with sub2api's
//! `api_protocol` deciding between Responses and Chat Completions.
use crate::rest::{
    error::ApiError,
    supplier_transfer::{Sub2apiAccount, credential, sub2api_record},
};
use codex2api_common::providers::api as channel;
use codex2api_storage::{NewSupplierAccount, SupplierStatus, SupplierTransferAccount};
use codex2api_suppliers::api::ApiCredentials;
use serde_json::{Value, json};

/// sub2api platform and credentials of an account; the wire decides the platform.
pub(crate) fn to_sub2api(
    item: &SupplierTransferAccount,
) -> Result<(&'static str, Value), ApiError> {
    let tokens = item
        .tokens
        .as_ref()
        .ok_or_else(|| ApiError::bad("没有 API Key 的账户无法导出为 Sub2api 格式"))?;
    let credentials = ApiCredentials::from_supplier_tokens(tokens)
        .map_err(|_| ApiError::bad("API 账户凭据格式无效"))?;
    let (platform, protocol) = match credentials.wire.as_str() {
        channel::WIRE_MESSAGES => ("anthropic", None),
        channel::WIRE_CHAT_COMPLETIONS => ("openai", Some("chat_completions")),
        _ => ("openai", Some("responses")),
    };
    let mut value = json!({"api_key":credentials.api_key,"base_url":credentials.base_url,"api_protocol":protocol});
    value
        .as_object_mut()
        .expect("credential object")
        .retain(|_, v| !v.is_null());
    Ok((platform, value))
}

pub(crate) fn from_sub2api(
    source: &Sub2apiAccount,
    platform: &str,
) -> Result<SupplierTransferAccount, ApiError> {
    let c = &source.credentials;
    let api_key = credential(c, "api_key").ok_or_else(|| ApiError::bad("缺少 API Key"))?;
    let wire = match platform {
        "anthropic" => channel::WIRE_MESSAGES,
        _ => match credential(c, "api_protocol").as_deref() {
            Some("chat_completions") | Some("adaptive") => channel::WIRE_CHAT_COMPLETIONS,
            _ => channel::WIRE_OPENAI_RESPONSES,
        },
    };
    let mut credentials = ApiCredentials {
        api_key,
        base_url: credential(c, "base_url")
            .unwrap_or_else(|| channel::default_base_url(wire).to_owned()),
        wire: wire.to_owned(),
        auth_scheme: channel::default_auth_scheme(wire).to_owned(),
        retry_statuses: Vec::new(),
    };
    credentials
        .validate()
        .map_err(|e| ApiError::bad(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut account = NewSupplierAccount::without_identity(
        codex2api_common::API,
        uuid::Uuid::new_v4().to_string(),
    );
    account.id = Some(id.clone());
    account.status = SupplierStatus::Active;
    account.display_name = Some(source.name.clone());
    account.chatgpt_account_id = Some(credentials.base_url.clone());
    account.chatgpt_user_id = Some(credentials.key_fingerprint());
    sub2api_record(source, account, credentials.to_supplier_tokens(&id))
}

/// A native export row: credentials decide the identity columns and no fingerprint exists.
pub(crate) fn normalize(item: &mut SupplierTransferAccount) -> Result<(), ApiError> {
    let tokens = item
        .tokens
        .as_ref()
        .ok_or_else(|| ApiError::bad("API 账户缺少凭据"))?;
    if tokens.auth_mode.as_deref() != Some(codex2api_common::API) {
        return Err(ApiError::bad("API 账户凭据格式无效"));
    }
    let credentials = ApiCredentials::from_supplier_tokens(tokens)
        .map_err(|_| ApiError::bad("API 账户凭据格式无效"))?;
    let a = &mut item.account;
    let id =
        a.id.clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    a.id = Some(id.clone());
    a.os_type = None;
    a.os_version = None;
    a.arch = None;
    a.http_fingerprint_json = "{}".into();
    a.chatgpt_account_id = Some(credentials.base_url.clone());
    a.chatgpt_user_id = Some(credentials.key_fingerprint());
    let mut normalized = credentials.to_supplier_tokens(&id);
    normalized.last_refresh = tokens.last_refresh.clone();
    item.tokens = Some(normalized);
    Ok(())
}
