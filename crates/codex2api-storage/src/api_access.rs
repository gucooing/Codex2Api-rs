//! Regular-user API credentials and ownership-bound management reads.
use crate::{
    ApiGroup, ApiListQuery, ApiPublicGroup, ListPage, Result, Storage, StorageError, UserStore,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection};

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
    pub key_prefix: String,
    pub enabled: bool,
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
    key_prefix: String,
    enabled: bool,
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
            key_prefix: self.key_prefix,
            enabled: self.enabled,
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
const KEY_COLUMNS:&str="k.id,k.name,k.group_id,g.name AS group_name,k.key_prefix,k.enabled,k.created_at,k.last_used_at,k.revision,k.user_id,u.username,
 (SELECT COUNT(*) FROM usage_records r WHERE r.key_id=k.id) AS total_requests,
 (SELECT CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(input_tokens)=COUNT(*) THEN SUM(input_tokens)::bigint END FROM usage_records r WHERE r.key_id=k.id) AS input_tokens,
 (SELECT CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(output_tokens)=COUNT(*) THEN SUM(output_tokens)::bigint END FROM usage_records r WHERE r.key_id=k.id) AS output_tokens,
 (SELECT CASE WHEN COUNT(*)=0 THEN 0 WHEN COUNT(cost_nano_usd)=COUNT(*) THEN SUM(cost_nano_usd)::bigint END FROM usage_records r WHERE r.key_id=k.id) AS cost_nano_usd,
 (SELECT COUNT(*) FROM usage_records r WHERE r.key_id=k.id AND r.cost_nano_usd IS NULL) AS unknown_cost_requests";

pub(crate) async fn principal_on(
    storage: &Storage,
    connection: &mut PgConnection,
    key: &str,
    hash: bool,
) -> Result<Option<ApiPrincipal>> {
    let selector = if hash { "k.token_hash=$1" } else { "k.id=$1" };
    let row:Option<(String,String,String,i64)>=sqlx::query_as(&format!("SELECT k.id,k.user_id,k.group_id,g.revision FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE {selector} AND k.enabled=TRUE AND k.deleted=FALSE AND u.enabled=TRUE AND g.enabled=TRUE AND g.deleted=FALSE"))
        .bind(key).fetch_optional(&mut *connection).await?;
    let Some((key_id, user_id, group_id, revision)) = row else {
        return Ok(None);
    };
    let Some(group) = storage
        .api_group_snapshot_on(connection, &group_id, revision)
        .await?
    else {
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
        let principal = principal_on(self, &mut tx, &crate::hash_token(token), true).await?;
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
        if !matches!(query.status.as_str(), "" | "enabled" | "disabled") {
            return Err(StorageError::InvalidAdminUpdate("API Key 状态筛选无效"));
        }
        let selected_owner = owner.unwrap_or(&query.user_id);
        // Deleted keys leave every list; their usage records keep the key's name.
        let condition = "k.deleted=FALSE AND ($1='' OR k.user_id=$2) AND ($3='' OR k.group_id=$4) AND ($5='' OR k.id=$6) AND ($7='' OR strpos(lower(k.name||' '||k.key_prefix||' '||u.username),lower($8))>0) AND ($9='' OR ($10='enabled' AND k.enabled=TRUE) OR ($11='disabled' AND k.enabled=FALSE))";
        let mut tx = self
            .pool()
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
        if let Some(owner) = owner {
            crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        }
        let total:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE {condition}"))
            .bind(selected_owner).bind(selected_owner).bind(&query.group_id).bind(&query.group_id).bind(&query.key_id).bind(&query.key_id)
            .bind(&query.search).bind(&query.search).bind(&query.status).bind(&query.status).bind(&query.status).fetch_one(&mut *tx).await?;
        let items=sqlx::query_as::<_,KeyRow>(&format!("SELECT {KEY_COLUMNS} FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE {condition} ORDER BY k.created_at DESC,k.id LIMIT $12 OFFSET $13"))
            .bind(selected_owner).bind(selected_owner).bind(&query.group_id).bind(&query.group_id).bind(&query.key_id).bind(&query.key_id)
            .bind(&query.search).bind(&query.search).bind(&query.status).bind(&query.status).bind(&query.status)
            .bind(i64::from(query.page_size)).bind(query.offset_for(total)).fetch_all(&mut *tx).await?.into_iter().map(|r|r.view(owner.is_none())).collect();
        tx.commit().await?;
        Ok(ListPage {
            items,
            total,
            page: query.page_for(total),
            page_size: i64::from(query.page_size),
        })
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
        let mut tx = self
            .storage
            .pool()
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
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
        // Search is a ChatGPT channel operation shown with the OpenAI catalog.
        let operations = if query.platform.is_empty()
            || query.platform == codex2api_common::model_platforms::OPENAI
        {
            crate::providers::chatgpt::catalog_operation_prices(&mut tx, group.as_ref()).await?
        } else {
            Vec::new()
        };
        tx.commit().await?;
        Ok(
            json!({"group":group.map(|g|g.public(total)),"items":items,"operations":operations,"total":total,"page":query.page_for(total),"page_size":query.page_size}),
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
        let mut tx = self.storage.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        require_group_available(&mut tx, owner, group_id).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let (secret, hash, prefix) = new_secret();
        sqlx::query("INSERT INTO api_keys(id,user_id,group_id,name,token_hash,key_prefix,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&id).bind(owner).bind(group_id).bind(name.trim()).bind(&hash).bind(&prefix).bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await?;
        let key = key_row_on(&mut tx, &id).await?;
        tx.commit().await?;
        Ok(CreatedApiKey { key, secret })
    }
    /// Renames, enables/disables and moves the key to another available API group.
    /// `Ok(false)` means the key is not this owner's current revision.
    pub async fn update_api_key(
        &self,
        owner: &str,
        id: &str,
        name: &str,
        group_id: &str,
        enabled: bool,
        revision: i64,
    ) -> Result<bool> {
        valid_key_name(name)?;
        let mut tx = self.storage.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        // Admission holds the same user lock while it checks the key's group and
        // inserts the usage record, so a group or status change cannot interleave.
        crate::storage::lock_user(&mut tx, owner).await?;
        let current: Option<(String, i64)> = sqlx::query_as(
            "SELECT group_id,revision FROM api_keys WHERE id=$1 AND user_id=$2 AND deleted=FALSE FOR UPDATE",
        )
        .bind(id)
        .bind(owner)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((current_group, current_revision)) = current else {
            return Ok(false);
        };
        if current_revision != revision {
            return Ok(false);
        }
        if current_group != group_id {
            require_group_available(&mut tx, owner, group_id).await?;
        }
        let updated = sqlx::query("UPDATE api_keys SET name=$1,group_id=$2,enabled=$3,revision=revision+1 WHERE id=$4 AND user_id=$5 AND revision=$6 AND EXISTS(SELECT 1 FROM regular_users WHERE id=$5 AND enabled=TRUE)")
            .bind(name.trim()).bind(group_id).bind(enabled).bind(id).bind(owner).bind(revision).execute(&mut *tx).await?.rows_affected()==1;
        tx.commit().await?;
        Ok(updated)
    }
    /// Replaces the secret of an existing key. Identity, group, name, usage history
    /// and the enabled flag stay; the previous secret stops authenticating at once.
    /// `Ok(None)` means the key is not this owner's current revision.
    pub async fn rotate_api_key(
        &self,
        owner: &str,
        id: &str,
        revision: i64,
    ) -> Result<Option<CreatedApiKey>> {
        let mut tx = self.storage.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        let (secret, hash, prefix) = new_secret();
        let rotated = sqlx::query("UPDATE api_keys SET token_hash=$1,key_prefix=$2,revision=revision+1 WHERE id=$3 AND user_id=$4 AND revision=$5 AND deleted=FALSE AND EXISTS(SELECT 1 FROM regular_users WHERE id=$4 AND enabled=TRUE)")
            .bind(&hash).bind(&prefix).bind(id).bind(owner).bind(revision).execute(&mut *tx).await?.rows_affected()==1;
        if !rotated {
            return Ok(None);
        }
        let key = key_row_on(&mut tx, id).await?;
        tx.commit().await?;
        Ok(Some(CreatedApiKey { key, secret }))
    }
    /// Deletes the key irreversibly: it leaves the owner's list and stops
    /// authenticating, while its usage records keep their owner and name.
    pub async fn delete_api_key(&self, owner: &str, id: &str, revision: i64) -> Result<bool> {
        let mut tx = self.storage.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        crate::storage::lock_user(&mut tx, owner).await?;
        let deleted = sqlx::query("UPDATE api_keys SET deleted=TRUE,enabled=FALSE,revision=revision+1 WHERE id=$1 AND user_id=$2 AND revision=$3 AND deleted=FALSE")
            .bind(id).bind(owner).bind(revision).execute(&mut *tx).await?.rows_affected()==1;
        tx.commit().await?;
        Ok(deleted)
    }
}

fn new_secret() -> (String, String, String) {
    let secret = format!("sk-c2a-{}", crate::oauth_secret());
    let hash = crate::hash_token(&secret);
    let prefix = secret[..15].to_owned();
    (secret, hash, prefix)
}

async fn require_group_available(
    connection: &mut PgConnection,
    owner: &str,
    group_id: &str,
) -> Result<()> {
    let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM regular_users u CROSS JOIN api_groups g WHERE u.id=$1 AND u.enabled=TRUE AND g.id=$2 AND g.enabled=TRUE AND g.deleted=FALSE)").bind(owner).bind(group_id).fetch_one(&mut *connection).await?;
    if available {
        Ok(())
    } else {
        Err(StorageError::Api {
            code: "api_group_unavailable",
            message: "用户或 API 组不可用",
        })
    }
}

async fn key_row_on(connection: &mut PgConnection, id: &str) -> Result<ApiKeyView> {
    Ok(sqlx::query_as::<_,KeyRow>(&format!("SELECT {KEY_COLUMNS} FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id WHERE k.id=$1"))
        .bind(id).fetch_one(&mut *connection).await?.view(false))
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
