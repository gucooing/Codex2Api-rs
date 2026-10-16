use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    routing::post,
};
use codex2api_storage::{SupplierTransferAccount, SupplierTransferProxy};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/suppliers/export", post(export))
        .route("/suppliers/import", post(import))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportInput {
    #[serde(default)]
    format: Format,
    account_ids: Vec<String>,
    include_proxies: bool,
}
async fn export(State(s): State<AdminState>, Json(input): Json<ExportInput>) -> ApiResult {
    if input.account_ids.is_empty() || input.account_ids.len() > 10_000 {
        return Err(ApiError::bad("请选择 1 到 10000 个供应账户"));
    }
    let mut accounts = Vec::new();
    for id in input
        .account_ids
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        accounts.push(
            s.storage
                .export_supplier(&id, input.include_proxies)
                .await?,
        );
    }
    let data = match input.format {
        Format::Native => json!({"type":"codex2api-suppliers","version":1,
            "exported_at":chrono::Utc::now().to_rfc3339(),"accounts":accounts}),
        Format::Sub2api => export_sub2api(&accounts)?,
    };
    // Every downloaded file must fit the import upload limit, including pretty printing.
    if serde_json::to_vec_pretty(&data)
        .map_err(|_| ApiError::bad("无法导出账户"))?
        .len()
        > 19 * 1024 * 1024
    {
        return Err(ApiError::bad("导出文件超过 19 MB，请减少所选账户"));
    }
    Ok(Json(data))
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Format {
    #[default]
    Native,
    Sub2api,
}

fn export_sub2api(accounts: &[SupplierTransferAccount]) -> Result<Value, ApiError> {
    let mut proxies = Vec::new();
    let mut proxy_keys = std::collections::HashMap::<String, String>::new();
    let mut rows = Vec::new();
    for item in accounts {
        let (platform, mut credentials) = match item.account.provider_id.as_str() {
            codex2api_common::CHATGPT => (
                "openai",
                crate::providers::chatgpt::transfer::to_sub2api(item)?,
            ),
            codex2api_common::GROK => ("grok", crate::providers::grok::transfer::to_sub2api(item)?),
            _ => return Err(ApiError::bad("该平台不支持 Sub2api 导出")),
        };
        credentials["model_mapping"] = json!(
            item.models
                .iter()
                .map(|m| (m.model.clone(), m.model.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
        let proxy_key = if let Some(proxy) = &item.proxy {
            if let Some(key) = proxy_keys.get(&proxy.url) {
                Some(key.clone())
            } else {
                let url = codex2api_storage::parse_proxy_url(&proxy.url)?;
                let username = urlencoding::decode(url.username())
                    .map_err(|_| ApiError::bad("代理用户名无效"))?;
                let password = urlencoding::decode(url.password().unwrap_or_default())
                    .map_err(|_| ApiError::bad("代理密码无效"))?;
                let host = url
                    .host_str()
                    .ok_or_else(|| ApiError::bad("代理主机无效"))?;
                let port = url
                    .port_or_known_default()
                    .or_else(|| matches!(url.scheme(), "socks5" | "socks5h").then_some(1080))
                    .ok_or_else(|| ApiError::bad("代理缺少端口"))?;
                let key = format!(
                    "{}|{}|{}|{}|{}",
                    url.scheme(),
                    host,
                    port,
                    username,
                    password
                );
                proxies.push(
                    json!({"proxy_key":key,"name":proxy.name,"protocol":url.scheme(),"host":host,
                    "port":port,"username":username,"password":password,"status":"active"}),
                );
                proxy_keys.insert(proxy.url.clone(), key.clone());
                Some(key)
            }
        } else {
            None
        };
        let name = [
            &item.account.display_name,
            &item.account.email,
            &item.account.id,
        ]
        .into_iter()
        .filter_map(|name| name.as_deref())
        .find(|name| !name.trim().is_empty())
        .unwrap_or("供应账户");
        let mut row = json!({"name":name,"platform":platform,"type":"oauth",
            "credentials":credentials,"concurrency":item.scheduling.concurrency,"priority":item.scheduling.priority});
        if let Some(key) = proxy_key {
            row["proxy_key"] = key.into();
        }
        rows.push(row);
    }
    Ok(
        json!({"type":"sub2api-data","version":1,"exported_at":chrono::Utc::now().to_rfc3339(),"accounts":rows,"proxies":proxies}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportInput {
    format: Format,
    include_proxies: bool,
    data: Value,
}

/// Sub2api's administrator account_data.go DataAccount, with only supported fields.
#[derive(Deserialize)]
pub(crate) struct Sub2apiAccount {
    pub name: String,
    pub credentials: Value,
    #[serde(default)]
    pub proxy_key: Option<String>,
    pub concurrency: u32,
    pub priority: i32,
}
pub(crate) fn credential(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}
pub(crate) fn sub2api_record(
    source: &Sub2apiAccount,
    account: codex2api_storage::NewSupplierAccount,
    tokens: codex2api_storage::SupplierTokens,
) -> Result<SupplierTransferAccount, ApiError> {
    let mut models = std::collections::BTreeSet::new();
    if let Some(mapping) = source
        .credentials
        .get("model_mapping")
        .filter(|v| !v.is_null())
    {
        let mapping = mapping
            .as_object()
            .ok_or_else(|| ApiError::bad("模型映射格式无效"))?;
        // Mapping targets are the model names actually sent to the supplier.
        for value in mapping.values() {
            let model = value
                .as_str()
                .filter(|m| codex2api_common::valid_model(m))
                .ok_or_else(|| ApiError::bad("模型映射包含无法导入的名称"))?;
            models.insert(model.to_owned());
        }
    }
    Ok(SupplierTransferAccount {
        account,
        tokens: Some(tokens),
        proxy: None,
        models: models
            .into_iter()
            .map(|model| codex2api_storage::SupplierModel {
                model,
                source: "custom".into(),
                observed_at: None,
            })
            .collect(),
        scheduling: codex2api_storage::SupplierScheduling {
            concurrency: source.concurrency,
            priority: source.priority,
            ..Default::default()
        },
        tags: vec![],
    })
}

fn sub2api_proxy(data: &Value, key: &str) -> Result<SupplierTransferProxy, ApiError> {
    let proxy = data["proxies"]
        .as_array()
        .and_then(|items| items.iter().find(|p| p["proxy_key"].as_str() == Some(key)))
        .ok_or_else(|| ApiError::bad("找不到账户引用的代理"))?;
    let scheme = proxy["protocol"]
        .as_str()
        .filter(|s| ["http", "https", "socks5", "socks5h"].contains(s))
        .ok_or_else(|| ApiError::bad("代理协议无效"))?;
    let host = proxy["host"]
        .as_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| ApiError::bad("代理主机无效"))?;
    let port = proxy["port"]
        .as_u64()
        .filter(|p| (1..=65535).contains(p))
        .ok_or_else(|| ApiError::bad("代理端口无效"))?;
    let mut url =
        url::Url::parse(&format!("{scheme}://localhost")).map_err(|_| ApiError::bad("代理无效"))?;
    url.set_host(Some(host))
        .map_err(|_| ApiError::bad("代理主机无效"))?;
    url.set_port(Some(port as u16))
        .map_err(|_| ApiError::bad("代理端口无效"))?;
    url.set_username(&urlencoding::encode(
        proxy["username"].as_str().unwrap_or_default(),
    ))
    .map_err(|_| ApiError::bad("代理用户名无效"))?;
    if let Some(password) = credential(proxy, "password") {
        url.set_password(Some(&urlencoding::encode(&password)))
            .map_err(|_| ApiError::bad("代理密码无效"))?;
    }
    codex2api_storage::parse_proxy_url(url.as_str()).map_err(|_| ApiError::bad("代理配置无效"))?;
    Ok(SupplierTransferProxy {
        name: credential(proxy, "name").unwrap_or_else(|| "导入代理".into()),
        url: url.to_string(),
    })
}

fn parse_account(
    format: &Format,
    data: &Value,
    row: &Value,
    include_proxies: bool,
) -> Result<Option<SupplierTransferAccount>, ApiError> {
    let mut item = match format {
        Format::Native => {
            if !row["provider_id"]
                .as_str()
                .is_some_and(codex2api_common::supported_provider)
            {
                return Ok(None);
            }
            serde_json::from_value::<SupplierTransferAccount>(row.clone())
                .map_err(|_| ApiError::bad("账户数据格式无效"))?
        }
        Format::Sub2api => {
            let platform = row["platform"].as_str().unwrap_or_default();
            if !["openai", "grok"].contains(&platform) || row["type"] != "oauth" {
                return Ok(None);
            }
            let source: Sub2apiAccount = serde_json::from_value(row.clone())
                .map_err(|_| ApiError::bad("Sub2api 账户数据格式无效"))?;
            let mut item = match platform {
                "openai" => crate::providers::chatgpt::transfer::from_sub2api(&source)?,
                "grok" => crate::providers::grok::transfer::from_sub2api(&source)?,
                _ => unreachable!(),
            };
            if include_proxies && let Some(key) = source.proxy_key.as_deref() {
                item.proxy = Some(sub2api_proxy(data, key)?);
            }
            item
        }
    };
    if !include_proxies {
        item.proxy = None;
    }
    let a = &item.account;
    if uuid::Uuid::parse_str(&a.installation_id).is_err()
        || a.id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
        || [&a.os_type, &a.os_version, &a.arch]
            .iter()
            .any(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        || a.display_name.as_ref().is_some_and(|s| s.len() > 1024)
    {
        return Err(ApiError::bad("账户身份或指纹无效"));
    }
    match a.provider_id.as_str() {
        codex2api_common::CHATGPT => crate::providers::chatgpt::transfer::normalize(&mut item)?,
        codex2api_common::GROK => crate::providers::grok::transfer::normalize(&mut item)?,
        _ => return Ok(None),
    }
    Ok(Some(item))
}
async fn import(State(s): State<AdminState>, Json(input): Json<ImportInput>) -> ApiResult {
    let data = &input.data;
    match input.format {
        Format::Native if data["type"] != "codex2api-suppliers" || data["version"] != 1 => {
            return Err(ApiError::bad("请选择 Codex2API 导出的 JSON 文件"));
        }
        Format::Sub2api
            if data
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| !["", "sub2api-data", "sub2api-bundle"].contains(&t))
                || data.get("version").is_some_and(|v| v != 0 && v != 1) =>
        {
            return Err(ApiError::bad("不支持的 Sub2api 文件版本"));
        }
        _ => {}
    }
    let accounts = data["accounts"]
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 10_000)
        .ok_or_else(|| ApiError::bad("文件须包含 1 到 10000 个账户"))?;
    let (mut imported, mut skipped, mut failed) = (0, 0, 0);
    let mut results = Vec::new();
    for (index, row) in accounts.iter().enumerate() {
        let mut name = match input.format {
            Format::Native => credential(row, "email"),
            Format::Sub2api => credential(&row["credentials"], "email"),
        };
        let result = match parse_account(&input.format, data, row, input.include_proxies) {
            Ok(Some(item)) => {
                name = item.account.email.clone().or(name);
                s.storage
                    .import_supplier(&item)
                    .await
                    .map(|created| {
                        if created {
                            "已导入"
                        } else {
                            "账户已存在，已跳过"
                        }
                    })
                    .map_err(|_| ApiError::bad("账户数据或代理配置无效，未导入"))
            }
            Ok(None) => Ok("平台或授权类型不支持，已跳过"),
            Err(e) => Err(e),
        };
        let (status, message) = match result {
            Ok("已导入") => {
                imported += 1;
                ("imported", "已导入".into())
            }
            Ok(message) => {
                skipped += 1;
                ("skipped", message.to_owned())
            }
            Err(error) => {
                failed += 1;
                ("failed", error.2)
            }
        };
        results.push(json!({"row":index+1,"name":name,"status":status,"message":message}));
    }
    Ok(Json(
        json!({"imported":imported,"skipped":skipped,"failed":failed,"results":results}),
    ))
}
