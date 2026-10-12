//! Regular-user API credentials and ownership-bound management reads.
use crate::{
    ApiGroup, ApiListQuery, ApiPublicGroup, ListPage, Result, Storage, StorageError, UserStore,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, SqliteConnection};

#[derive(Clone, Debug)]
pub struct ApiPrincipal {
    pub user_id: String,
    pub key_id: String,
    pub group: ApiGroup,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyView {
    pub id: String,
    pub name: String,
    pub group_id: String,
    pub group_name: String,
    pub provider_id: String,
    pub key_prefix: String,
    pub enabled: bool,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revision: i64,
    pub total_requests: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_nano_usd: Option<i64>,
    pub unknown_cost_requests: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}
#[derive(Serialize)]
pub struct CreatedApiKey {
    pub key: ApiKeyView,
    pub secret: String,
}
#[derive(FromRow)]
struct KeyRow {
    id: String,
    name: String,
    group_id: String,
    group_name: String,
    provider_id: String,
    key_prefix: String,
    enabled: bool,
    revoked_at: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    revision: i64,
    total_requests: i64,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cost_nano_usd: Option<i64>,
    unknown_cost_requests: i64,
    user_id: String,
    username: String,
}
impl KeyRow {
    fn view(self, admin: bool) -> ApiKeyView {
        ApiKeyView {
            id: self.id,
            name: self.name,
            group_id: self.group_id,
            group_name: self.group_name,
            provider_id: self.provider_id,
            key_prefix: self.key_prefix,
            enabled: self.enabled,
            revoked_at: self.revoked_at,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
            revision: self.revision,
            total_requests: self.total_requests,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cost_nano_usd: self.cost_nano_usd,
            unknown_cost_requests: self.unknown_cost_requests,
            user_id: admin.then_some(self.user_id),
            username: admin.then_some(self.username),
        }
    }
}
const KEY_COLUMNS:&str="k.id,k.name,k.group_id,g.name AS group_name,g.provider_id,k.key_prefix,k.enabled,k.revoked_at,k.created_at,k.last_used_at,k.revision,k.user_id,u.username,
 (SELECT COUNT(*) FROM api_usage_records r WHERE r.key_id=k.id) AS total_requests,
 (SELECT CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(input_tokens)=COUNT(*) THEN SUM(input_tokens) END FROM api_usage_records r WHERE r.key_id=k.id) AS input_tokens,
 (SELECT CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(output_tokens)=COUNT(*) THEN SUM(output_tokens) END FROM api_usage_records r WHERE r.key_id=k.id) AS output_tokens,
 (SELECT CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(cost_nano_usd)=COUNT(*) THEN SUM(cost_nano_usd) END FROM api_usage_records r WHERE r.key_id=k.id) AS cost_nano_usd,
 (SELECT COUNT(*) FROM api_usage_records r WHERE r.key_id=k.id AND r.cost_nano_usd IS NULL) AS unknown_cost_requests";

pub(crate) async fn principal_on(
    connection: &mut SqliteConnection,
    key: &str,
    hash: bool,
) -> Result<Option<ApiPrincipal>> {
    let selector = if hash { "k.token_hash=?" } else { "k.id=?" };
    let row:Option<(String,String,String)>=sqlx::query_as(&format!("SELECT k.id,k.user_id,k.group_id FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE {selector} AND k.enabled=1 AND k.revoked_at IS NULL AND u.enabled=1 AND g.enabled=1 AND g.deleted=0"))
        .bind(key).fetch_optional(&mut *connection).await?;
    let Some((key_id, user_id, group_id)) = row else {
        return Ok(None);
    };
    let Some(group) = crate::api_groups::group_on(connection, &group_id).await? else {
        return Ok(None);
    };
    Ok(Some(ApiPrincipal {
        user_id,
        key_id,
        group,
    }))
}
impl Storage {
    pub async fn authenticate_api_key(&self, token: &str) -> Result<Option<ApiPrincipal>> {
        if !token.starts_with("sk-c2a-") || token.len() != 71 {
            return Ok(None);
        }
        let mut tx = self.pool().begin().await?;
        let principal = principal_on(&mut tx, &crate::hash_token(token), true).await?;
        tx.commit().await?;
        Ok(principal)
    }
    pub async fn admin_api_keys(&self, query: &ApiListQuery) -> Result<ListPage<ApiKeyView>> {
        self.api_key_page(None, query).await
    }
    async fn api_key_page(
        &self,
        owner: Option<&str>,
        query: &ApiListQuery,
    ) -> Result<ListPage<ApiKeyView>> {
        query.validate()?;
        let selected_owner = owner.unwrap_or(&query.user_id);
        let condition = "(?='' OR k.user_id=?) AND (?='' OR k.group_id=?) AND (?='' OR k.id=?) AND (?='' OR g.provider_id=?) AND (?='' OR instr(lower(k.name||' '||k.key_prefix||' '||u.username),lower(?))>0) AND (?='' OR (? IN ('active','enabled') AND k.enabled=1 AND k.revoked_at IS NULL) OR (?='disabled' AND k.enabled=0 AND k.revoked_at IS NULL) OR (?='revoked' AND k.revoked_at IS NOT NULL))";
        let mut tx = self.pool().begin().await?;
        if let Some(owner) = owner {
            crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        }
        let total:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE {condition}"))
            .bind(selected_owner).bind(selected_owner).bind(&query.group_id).bind(&query.group_id).bind(&query.key_id).bind(&query.key_id).bind(&query.provider_id).bind(&query.provider_id)
            .bind(&query.search).bind(&query.search).bind(&query.status).bind(&query.status).bind(&query.status).bind(&query.status).fetch_one(&mut *tx).await?;
        let items=sqlx::query_as::<_,KeyRow>(&format!("SELECT {KEY_COLUMNS} FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE {condition} ORDER BY k.created_at DESC,k.id LIMIT ? OFFSET ?"))
            .bind(selected_owner).bind(selected_owner).bind(&query.group_id).bind(&query.group_id).bind(&query.key_id).bind(&query.key_id).bind(&query.provider_id).bind(&query.provider_id)
            .bind(&query.search).bind(&query.search).bind(&query.status).bind(&query.status).bind(&query.status).bind(&query.status)
            .bind(i64::from(query.page_size)).bind(query.offset_for(total)).fetch_all(&mut *tx).await?.into_iter().map(|r|r.view(owner.is_none())).collect();
        tx.commit().await?;
        Ok(ListPage {
            items,
            total,
            page: query.page_for(total),
            page_size: i64::from(query.page_size),
        })
    }
    pub async fn admin_api_usage(&self, query: &ApiListQuery) -> Result<Value> {
        self.api_usage_page(None, query).await
    }
    async fn api_usage_page(&self, owner: Option<&str>, query: &ApiListQuery) -> Result<Value> {
        query.validate()?;
        let selected_owner = owner.unwrap_or(&query.user_id);
        let status = if query.status == "in_progress" {
            "started"
        } else {
            query.status.as_str()
        };
        let condition = "(?='' OR r.user_id=?) AND (?='' OR r.group_id=?) AND (?='' OR r.key_id=?) AND (?='' OR r.provider_id=?) AND (?='' OR r.status=?) AND (?='' OR instr(lower(r.model||' '||k.name||' '||u.username),lower(?))>0) AND (? IS NULL OR r.requested_at_ms>=?) AND (? IS NULL OR r.requested_at_ms<?)";
        let from = "api_usage_records r JOIN regular_users u ON u.id=r.user_id JOIN api_keys k ON k.id=r.key_id";
        let mut tx = self.pool().begin().await?;
        if let Some(owner) = owner {
            crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        }
        let summary:ApiUsageSummary=sqlx::query_as(&format!("SELECT COUNT(*) AS requests,CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(input_tokens)=COUNT(*) THEN SUM(input_tokens) END AS input_tokens,CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(output_tokens)=COUNT(*) THEN SUM(output_tokens) END AS output_tokens,CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(cost_nano_usd)=COUNT(*) THEN SUM(cost_nano_usd) END AS cost_nano_usd,COALESCE(SUM(cost_nano_usd IS NULL),0) AS unknown_cost_requests FROM {from} WHERE {condition}"))
            .bind(selected_owner).bind(selected_owner).bind(&query.group_id).bind(&query.group_id).bind(&query.key_id).bind(&query.key_id).bind(&query.provider_id).bind(&query.provider_id)
            .bind(status).bind(status).bind(&query.search).bind(&query.search).bind(query.from_ms).bind(query.from_ms).bind(query.until_ms).bind(query.until_ms).fetch_one(&mut *tx).await?;
        let rows:Vec<ApiUsageRow>=sqlx::query_as(&format!("SELECT r.*,k.name AS key_name,k.key_prefix,u.username FROM {from} WHERE {condition} ORDER BY r.requested_at_ms DESC,r.id LIMIT ? OFFSET ?"))
            .bind(selected_owner).bind(selected_owner).bind(&query.group_id).bind(&query.group_id).bind(&query.key_id).bind(&query.key_id).bind(&query.provider_id).bind(&query.provider_id)
            .bind(status).bind(status).bind(&query.search).bind(&query.search).bind(query.from_ms).bind(query.from_ms).bind(query.until_ms).bind(query.until_ms)
            .bind(i64::from(query.page_size)).bind(query.offset_for(summary.requests)).fetch_all(&mut *tx).await?;
        let pending:i64=sqlx::query_scalar("SELECT COALESCE(SUM(s.pending_nano_usd),0) FROM api_wallet_state s JOIN regular_users u ON u.id=s.user_id WHERE ?='' OR s.user_id=?").bind(selected_owner).bind(selected_owner).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(
            json!({"items":rows.into_iter().map(|r|r.view(owner.is_none())).collect::<Vec<_>>(),"total":summary.requests,"page":query.page_for(summary.requests),"page_size":query.page_size,"summary":summary,"wallet_pending_nano_usd":pending}),
        )
    }
}

impl UserStore {
    pub async fn api_groups(&self, owner: &str) -> Result<Vec<ApiPublicGroup>> {
        self.storage
            .require_account_scope(owner, crate::AccountScope::User)
            .await?;
        let groups = self.storage.api_groups().await?;
        let mut items = Vec::new();
        let mut tx = self.storage.pool().begin().await?;
        for group in groups.into_iter().filter(|g| g.enabled) {
            let (total, _) = crate::api_groups::catalog_on(
                &mut tx,
                Some(&group),
                Some(&ApiListQuery::default()),
            )
            .await?;
            items.push(group.public(total));
        }
        tx.commit().await?;
        Ok(items)
    }
    pub async fn api_catalog(&self, owner: &str, query: &ApiListQuery) -> Result<Value> {
        query.validate()?;
        let mut tx = self.storage.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        let group = if query.group_id.is_empty() {
            None
        } else {
            Some(
                crate::api_groups::group_on(&mut tx, &query.group_id)
                    .await?
                    .filter(|g| g.enabled)
                    .ok_or(StorageError::Api {
                        code: "api_group_unavailable",
                        message: "API 组不可用",
                    })?,
            )
        };
        let (total, items) =
            crate::api_groups::catalog_on(&mut tx, group.as_ref(), Some(query)).await?;
        tx.commit().await?;
        Ok(
            json!({"group":group.map(|g|g.public(total)),"items":items,"total":total,"page":query.page_for(total),"page_size":query.page_size}),
        )
    }
    pub async fn api_keys(
        &self,
        owner: &str,
        query: &ApiListQuery,
    ) -> Result<ListPage<ApiKeyView>> {
        self.storage.api_key_page(Some(owner), query).await
    }
    pub async fn create_api_key(
        &self,
        owner: &str,
        name: &str,
        group_id: &str,
    ) -> Result<CreatedApiKey> {
        valid_key_name(name)?;
        let mut tx = self.storage.pool().begin_with("BEGIN IMMEDIATE").await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM regular_users u CROSS JOIN api_groups g WHERE u.id=? AND u.enabled=1 AND g.id=? AND g.enabled=1 AND g.deleted=0)").bind(owner).bind(group_id).fetch_one(&mut *tx).await?;
        if !available {
            return Err(StorageError::Api {
                code: "api_group_unavailable",
                message: "用户或 API 组不可用",
            });
        }
        let id = uuid::Uuid::new_v4().to_string();
        let secret = format!("sk-c2a-{}", crate::oauth_secret());
        let prefix = &secret[..15];
        sqlx::query("INSERT INTO api_keys(id,user_id,group_id,name,token_hash,key_prefix,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(group_id).bind(name.trim()).bind(crate::hash_token(&secret)).bind(prefix).bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await?;
        let key=sqlx::query_as::<_,KeyRow>(&format!("SELECT {KEY_COLUMNS} FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE k.id=?"))
            .bind(&id).fetch_one(&mut *tx).await?.view(false);
        tx.commit().await?;
        Ok(CreatedApiKey { key, secret })
    }
    pub async fn update_api_key(
        &self,
        owner: &str,
        id: &str,
        name: &str,
        enabled: bool,
        revision: i64,
    ) -> Result<bool> {
        valid_key_name(name)?;
        self.storage
            .require_account_scope(owner, crate::AccountScope::User)
            .await?;
        Ok(sqlx::query("UPDATE api_keys SET name=?,enabled=?,revision=revision+1 WHERE id=? AND user_id=? AND revision=? AND revoked_at IS NULL AND EXISTS(SELECT 1 FROM regular_users WHERE id=? AND enabled=1)")
            .bind(name.trim()).bind(enabled).bind(id).bind(owner).bind(revision).bind(owner).execute(self.storage.pool()).await?.rows_affected()==1)
    }
    pub async fn revoke_api_key(&self, owner: &str, id: &str, revision: i64) -> Result<bool> {
        self.storage
            .require_account_scope(owner, crate::AccountScope::User)
            .await?;
        Ok(sqlx::query("UPDATE api_keys SET enabled=0,revoked_at=?,revision=revision+1 WHERE id=? AND user_id=? AND revision=? AND revoked_at IS NULL")
            .bind(chrono::Utc::now().to_rfc3339()).bind(id).bind(owner).bind(revision).execute(self.storage.pool()).await?.rows_affected()==1)
    }
    pub async fn api_usage(&self, owner: &str, query: &ApiListQuery) -> Result<Value> {
        self.storage.api_usage_page(Some(owner), query).await
    }
}
fn valid_key_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 160 {
        Err(StorageError::InvalidAdminUpdate(
            "API Key 名称须为 1 到 160 字节",
        ))
    } else {
        Ok(())
    }
}

#[derive(FromRow, Serialize)]
struct ApiUsageSummary {
    requests: i64,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cost_nano_usd: Option<i64>,
    unknown_cost_requests: i64,
}
#[derive(FromRow)]
struct ApiUsageRow {
    id: String,
    user_id: String,
    username: String,
    key_id: String,
    key_name: String,
    key_prefix: String,
    group_id: String,
    group_name: String,
    provider_id: String,
    endpoint: String,
    model: String,
    actual_model: Option<String>,
    multiplier_micros: i64,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cached_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    image_size: Option<String>,
    image_count: Option<i64>,
    requested_at_ms: i64,
    completed_at_ms: Option<i64>,
    status: String,
    http_status: Option<i64>,
    cost_nano_usd: Option<i64>,
    billing_status: String,
    wallet_debit_cents: i64,
}
impl ApiUsageRow {
    fn view(self, admin: bool) -> Value {
        let status = if self.status == "started" {
            "in_progress"
        } else {
            &self.status
        };
        let mut value = json!({"id":self.id,"key_id":self.key_id,"key_name":self.key_name,"key_prefix":self.key_prefix,"group_id":self.group_id,"group_name":self.group_name,"provider_id":self.provider_id,"endpoint":self.endpoint,"model":self.model,"actual_model":self.actual_model,"multiplier":crate::format_units(self.multiplier_micros,6),"input_tokens":self.input_tokens,"output_tokens":self.output_tokens,"cached_tokens":self.cached_tokens,"cache_write_tokens":self.cache_write_tokens,"reasoning_tokens":self.reasoning_tokens,"image_size":self.image_size,"image_count":self.image_count,"requested_at_ms":self.requested_at_ms,"completed_at_ms":self.completed_at_ms,"status":status,"http_status":self.http_status,"cost_nano_usd":self.cost_nano_usd,"billing_status":self.billing_status,"wallet_debit_cents":self.wallet_debit_cents});
        if admin {
            value["user_id"] = json!(self.user_id);
            value["username"] = json!(self.username);
        }
        value
    }
}
