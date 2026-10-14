//! API product policy and public prices. Supplier routing never enters user DTOs.
use crate::{ImagePrice, ListPage, ModelConfig, ModelPrice, Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiModelRule {
    pub model: String,
    pub multiplier: Option<String>,
    #[serde(default)]
    pub concurrency_limit: u32,
    #[serde(default)]
    pub rpm_limit: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider_id: String,
    pub enabled: bool,
    pub tag_ids: Vec<String>,
    pub allowed_models: Vec<String>,
    pub multiplier: String,
    pub search_price_usd: Option<String>,
    pub concurrency_limit: u32,
    pub rpm_limit: u32,
    pub model_rules: Vec<ApiModelRule>,
    pub revision: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiPublicGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider_id: String,
    pub multiplier: String,
    pub search_price_usd: Option<String>,
    pub concurrency_limit: u32,
    pub rpm_limit: u32,
    pub models_count: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiListQuery {
    pub search: String,
    pub user_id: String,
    pub group_id: String,
    pub key_id: String,
    pub provider_id: String,
    pub status: String,
    pub page: u32,
    pub page_size: u32,
    pub from_ms: Option<i64>,
    pub until_ms: Option<i64>,
}
impl Default for ApiListQuery {
    fn default() -> Self {
        Self {
            search: String::new(),
            user_id: String::new(),
            group_id: String::new(),
            key_id: String::new(),
            provider_id: String::new(),
            status: String::new(),
            page: 1,
            page_size: 20,
            from_ms: None,
            until_ms: None,
        }
    }
}
impl ApiListQuery {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.page == 0
            || ![10, 20, 30, 50].contains(&self.page_size)
            || [
                &self.search,
                &self.user_id,
                &self.group_id,
                &self.key_id,
                &self.provider_id,
                &self.status,
            ]
            .iter()
            .any(|v| v.len() > 256)
            || matches!((self.from_ms,self.until_ms),(Some(a),Some(b)) if a>=b)
            || [self.from_ms, self.until_ms]
                .into_iter()
                .flatten()
                .any(|v| !(0..=253_402_300_799_000).contains(&v))
        {
            return Err(StorageError::InvalidAdminUpdate("API 列表筛选条件无效"));
        }
        Ok(())
    }
    pub(crate) fn page_for(&self, total: i64) -> u32 {
        self.page.min(
            ((total.max(1) - 1) / i64::from(self.page_size) + 1).min(i64::from(u32::MAX)) as u32,
        )
    }
    pub(crate) fn offset_for(&self, total: i64) -> i64 {
        i64::from(self.page_for(total) - 1) * i64::from(self.page_size)
    }
}

#[derive(FromRow)]
struct GroupRow {
    id: String,
    name: String,
    description: String,
    provider_id: String,
    enabled: bool,
    tag_ids: String,
    allowed_models: String,
    multiplier_micros: i64,
    search_price_nano_usd: Option<i64>,
    concurrency_limit: u32,
    rpm_limit: u32,
    model_rules: String,
    revision: i64,
}
impl TryFrom<GroupRow> for ApiGroup {
    type Error = StorageError;
    fn try_from(row: GroupRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            name: row.name,
            description: row.description,
            provider_id: row.provider_id,
            enabled: row.enabled,
            tag_ids: serde_json::from_str(&row.tag_ids)?,
            allowed_models: serde_json::from_str(&row.allowed_models)?,
            multiplier: crate::format_units(row.multiplier_micros, 6),
            search_price_usd: row
                .search_price_nano_usd
                .map(|price| crate::format_units(price, 9)),
            concurrency_limit: row.concurrency_limit,
            rpm_limit: row.rpm_limit,
            model_rules: serde_json::from_str(&row.model_rules)?,
            revision: row.revision,
        })
    }
}
const GROUP_COLUMNS: &str = "g.*,COALESCE((SELECT json_group_array(tag_id) FROM (SELECT tag_id FROM api_group_tags WHERE group_id=g.id ORDER BY tag_id)),'[]') AS tag_ids";

pub(crate) fn multiplier_micros(value: &str) -> Result<i64> {
    crate::decimal_units(value, 6)
        .filter(|v| (0..=1_000_000_000).contains(v))
        .ok_or(StorageError::InvalidAdminUpdate(
            "倍率须为 0 到 1000，最多六位小数",
        ))
}
impl ApiGroup {
    pub fn model_rule(&self, model: &str) -> Option<&ApiModelRule> {
        self.model_rules.iter().find(|r| r.model == model)
    }
    pub fn model_multiplier(&self, model: &str) -> &str {
        self.model_rule(model)
            .and_then(|r| r.multiplier.as_deref())
            .unwrap_or(&self.multiplier)
    }
    pub(crate) fn public(&self, models_count: i64) -> ApiPublicGroup {
        ApiPublicGroup {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            provider_id: self.provider_id.clone(),
            multiplier: self.multiplier.clone(),
            search_price_usd: self.search_price_usd.clone(),
            concurrency_limit: self.concurrency_limit,
            rpm_limit: self.rpm_limit,
            models_count,
        }
    }
}

pub(crate) async fn group_on(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<ApiGroup>> {
    sqlx::query_as::<_, GroupRow>(&format!(
        "SELECT {GROUP_COLUMNS} FROM api_groups g WHERE g.id=? AND g.deleted=0"
    ))
    .bind(id)
    .fetch_optional(connection)
    .await?
    .map(TryInto::try_into)
    .transpose()
}

impl Storage {
    pub async fn api_group(&self, id: &str) -> Result<Option<ApiGroup>> {
        group_on(&mut *self.pool().acquire().await?, id).await
    }
    pub async fn api_groups(&self) -> Result<Vec<ApiGroup>> {
        sqlx::query_as::<_, GroupRow>(&format!(
            "SELECT {GROUP_COLUMNS} FROM api_groups g WHERE g.deleted=0 ORDER BY g.name,g.id"
        ))
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
    pub async fn api_group_page(&self, query: &ApiListQuery) -> Result<ListPage<ApiGroup>> {
        query.validate()?;
        let condition = "g.deleted=0 AND (?='' OR g.provider_id=?) AND (?='' OR instr(lower(g.name||' '||g.description),lower(?))>0)";
        let mut tx = self.pool().begin().await?;
        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM api_groups g WHERE {condition}"
        ))
        .bind(&query.provider_id)
        .bind(&query.provider_id)
        .bind(&query.search)
        .bind(&query.search)
        .fetch_one(&mut *tx)
        .await?;
        let items=sqlx::query_as::<_, GroupRow>(&format!("SELECT {GROUP_COLUMNS} FROM api_groups g WHERE {condition} ORDER BY g.name,g.id LIMIT ? OFFSET ?"))
            .bind(&query.provider_id).bind(&query.provider_id).bind(&query.search).bind(&query.search)
            .bind(i64::from(query.page_size)).bind(query.offset_for(total)).fetch_all(&mut *tx).await?.into_iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(ListPage {
            items,
            total,
            page: query.page_for(total),
            page_size: i64::from(query.page_size),
        })
    }
    pub async fn save_api_group(&self, group: &ApiGroup, expected: Option<i64>) -> Result<bool> {
        let multiplier = multiplier_micros(&group.multiplier)?;
        if group.provider_id != codex2api_core::CHATGPT && group.search_price_usd.is_some() {
            return Err(StorageError::InvalidAdminUpdate(
                "只有 ChatGPT 平台可以配置搜索单价",
            ));
        }
        let search_price = group
            .search_price_usd
            .as_deref()
            .map(|price| {
                crate::decimal_units(price, 9).ok_or(StorageError::InvalidAdminUpdate(
                    "搜索单价须为非负美元金额，最多九位小数",
                ))
            })
            .transpose()?;
        if group.id.is_empty()
            || group.id.len() > 128
            || group.name.trim().is_empty()
            || group.name.len() > 160
            || group.description.len() > 2000
            || group.tag_ids.len() > 100
            || group.allowed_models.len() > 2048
            || group.model_rules.len() > 2048
            || group.concurrency_limit > 1_000_000
            || group.rpm_limit > 1_000_000
        {
            return Err(StorageError::InvalidAdminUpdate(
                "API 组名称、介绍、模型或限制无效",
            ));
        }
        let mut seen = BTreeSet::new();
        for model in &group.allowed_models {
            if !codex2api_core::valid_model(model) || !seen.insert(model) {
                return Err(StorageError::InvalidAdminUpdate("模型列表无效或重复"));
            }
        }
        seen.clear();
        for rule in &group.model_rules {
            if !codex2api_core::valid_model(&rule.model)
                || !seen.insert(&rule.model)
                || rule.concurrency_limit > 1_000_000
                || rule.rpm_limit > 1_000_000
            {
                return Err(StorageError::InvalidAdminUpdate(
                    "模型倍率或限制规则无效或重复",
                ));
            }
            if let Some(value) = &rule.multiplier {
                multiplier_micros(value)?;
            }
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let current = group_on(&mut tx, &group.id).await?;
        if current.as_ref().map(|g| g.revision) != expected {
            return Ok(false);
        }
        if expected.is_none()
            && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM api_groups WHERE id=?)")
                .bind(&group.id)
                .fetch_one(&mut *tx)
                .await?
        {
            return Ok(false);
        }
        if current
            .as_ref()
            .is_some_and(|g| g.provider_id != group.provider_id)
        {
            return Err(StorageError::InvalidAdminUpdate("API 组的平台不可更改"));
        }
        let valid_provider: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM providers WHERE id=?)")
                .bind(&group.provider_id)
                .fetch_one(&mut *tx)
                .await?;
        if !valid_provider {
            return Err(StorageError::InvalidAdminUpdate("平台无效"));
        }
        for tag in &group.tag_ids {
            let valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM supplier_tags WHERE id=? AND provider_id=?)",
            )
            .bind(tag)
            .bind(&group.provider_id)
            .fetch_one(&mut *tx)
            .await?;
            if !valid {
                return Err(StorageError::InvalidAdminUpdate("请选择同平台的标签"));
            }
        }
        for model in group
            .allowed_models
            .iter()
            .chain(group.model_rules.iter().map(|r| &r.model))
        {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_catalog WHERE provider_id=? AND model=? AND deleted=0)").bind(&group.provider_id).bind(model).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(StorageError::InvalidAdminUpdate(
                    "请先在计费配置中添加同平台模型",
                ));
            }
        }
        sqlx::query("INSERT INTO api_groups(id,name,description,provider_id,enabled,allowed_models,multiplier_micros,search_price_nano_usd,concurrency_limit,rpm_limit,model_rules,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,enabled=excluded.enabled,allowed_models=excluded.allowed_models,multiplier_micros=excluded.multiplier_micros,search_price_nano_usd=excluded.search_price_nano_usd,concurrency_limit=excluded.concurrency_limit,rpm_limit=excluded.rpm_limit,model_rules=excluded.model_rules,revision=api_groups.revision+1")
            .bind(&group.id).bind(group.name.trim()).bind(group.description.trim()).bind(&group.provider_id).bind(group.enabled)
            .bind(serde_json::to_string(&group.allowed_models)?).bind(multiplier).bind(search_price).bind(group.concurrency_limit).bind(group.rpm_limit)
            .bind(serde_json::to_string(&group.model_rules)?).bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM api_group_tags WHERE group_id=?")
            .bind(&group.id)
            .execute(&mut *tx)
            .await?;
        for tag in group.tag_ids.iter().collect::<BTreeSet<_>>() {
            sqlx::query("INSERT INTO api_group_tags(group_id,tag_id) VALUES(?,?)")
                .bind(&group.id)
                .bind(tag)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(true)
    }
    pub async fn delete_api_group(&self, id: &str, revision: i64) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let changed=sqlx::query("UPDATE api_groups SET deleted=1,enabled=0,revision=revision+1 WHERE id=? AND revision=? AND deleted=0").bind(id).bind(revision).execute(&mut *tx).await?.rows_affected()==1;
        if changed {
            sqlx::query("DELETE FROM api_group_tags WHERE group_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(changed)
    }
    pub async fn api_group_models(&self, id: &str) -> Result<Vec<ApiCatalogModel>> {
        let mut tx = self.pool().begin().await?;
        let group =
            group_on(&mut tx, id)
                .await?
                .filter(|g| g.enabled)
                .ok_or(StorageError::Api {
                    code: "api_group_unavailable",
                    message: "API 组不可用",
                })?;
        let (_, items) = catalog_on(&mut tx, Some(&group), None).await?;
        tx.commit().await?;
        Ok(items)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiTokenPrice {
    pub tier: String,
    pub min_input_tokens: i64,
    pub max_input_tokens: Option<i64>,
    pub input_usd_per_million: String,
    pub cached_usd_per_million: String,
    pub cache_write_usd_per_million: String,
    pub output_usd_per_million: String,
    pub base_input_usd_per_million: String,
    pub base_cached_usd_per_million: String,
    pub base_cache_write_usd_per_million: String,
    pub base_output_usd_per_million: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ApiImagePrice {
    pub resolution: String,
    pub base_price_usd: String,
    pub price_usd: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ApiCatalogModel {
    pub provider_id: String,
    pub model: String,
    pub kind: String,
    pub multiplier: String,
    pub group_concurrency_limit: u32,
    pub group_rpm_limit: u32,
    pub concurrency_limit: u32,
    pub rpm_limit: u32,
    pub prices: Vec<ApiTokenPrice>,
    pub image_prices: Vec<ApiImagePrice>,
    pub search_price: Option<String>,
}
fn scaled(value: i128, places: u32) -> String {
    let scale = 10_i128.pow(places);
    format!(
        "{}.{:0width$}",
        value / scale,
        value % scale,
        width = places as usize
    )
    .trim_end_matches('0')
    .trim_end_matches('.')
    .to_owned()
}

const CATALOG_CONDITION:&str="m.enabled=1 AND m.deleted=0 AND (?='' OR m.provider_id=?) AND (?='' OR instr(lower(m.model),lower(?))>0)
 AND (? IS NULL OR (m.provider_id=? AND EXISTS(SELECT 1 FROM api_group_tags gt JOIN supplier_tag_members tm ON tm.tag_id=gt.tag_id JOIN supplier_accounts s ON s.id=tm.account_id JOIN supplier_models sm ON sm.account_id=s.id WHERE gt.group_id=? AND s.provider_id=m.provider_id AND sm.model=m.model)
 AND (json_array_length(?)=0 OR m.model IN(SELECT value FROM json_each(?)))))";

pub(crate) async fn catalog_on(
    connection: &mut SqliteConnection,
    group: Option<&ApiGroup>,
    query: Option<&ApiListQuery>,
) -> Result<(i64, Vec<ApiCatalogModel>)> {
    let provider = query.map_or("", |q| q.provider_id.as_str());
    let search = query.map_or("", |q| q.search.as_str());
    let allowed =
        serde_json::to_string(&group.map(|g| g.allowed_models.as_slice()).unwrap_or(&[]))?;
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM model_catalog m WHERE {CATALOG_CONDITION}"
    ))
    .bind(provider)
    .bind(provider)
    .bind(search)
    .bind(search)
    .bind(group.map(|g| &g.id))
    .bind(group.map(|g| &g.provider_id))
    .bind(group.map(|g| &g.id))
    .bind(&allowed)
    .bind(&allowed)
    .fetch_one(&mut *connection)
    .await?;
    let configs:Vec<ModelConfig>=sqlx::query_as(&format!("SELECT m.* FROM model_catalog m WHERE {CATALOG_CONDITION} ORDER BY m.provider_id,m.model LIMIT ? OFFSET ?"))
        .bind(provider).bind(provider).bind(search).bind(search).bind(group.map(|g|&g.id)).bind(group.map(|g|&g.provider_id)).bind(group.map(|g|&g.id)).bind(&allowed).bind(&allowed)
        .bind(query.map_or(-1,|q|i64::from(q.page_size))).bind(query.map_or(0,|q|q.offset_for(total))).fetch_all(&mut *connection).await?;
    let selected = serde_json::to_string(
        &configs
            .iter()
            .map(|m| (&m.provider_id, &m.model))
            .collect::<Vec<_>>(),
    )?;
    let prices:Vec<ModelPrice>=sqlx::query_as("SELECT p.* FROM model_prices p JOIN json_each(?) j ON p.provider_id=json_extract(j.value,'$[0]') AND p.model=json_extract(j.value,'$[1]') ORDER BY p.provider_id,p.model,p.tier,p.min_input_tokens").bind(&selected).fetch_all(&mut *connection).await?;
    let images:Vec<ImagePrice>=sqlx::query_as("SELECT p.* FROM model_image_prices p JOIN json_each(?) j ON p.provider_id=json_extract(j.value,'$[0]') AND p.model=json_extract(j.value,'$[1]') ORDER BY p.provider_id,p.model,p.resolution").bind(&selected).fetch_all(&mut *connection).await?;
    let mut items = Vec::new();
    for config in configs {
        let multiplier = group.map_or("1", |g| g.model_multiplier(&config.model));
        let factor = i128::from(multiplier_micros(multiplier)?);
        let rule = group.and_then(|g| g.model_rule(&config.model));
        let token_prices = prices
            .iter()
            .filter(|p| p.provider_id == config.provider_id && p.model == config.model)
            .map(|p| ApiTokenPrice {
                tier: p.tier.clone(),
                min_input_tokens: p.min_input_tokens,
                max_input_tokens: p.max_input_tokens,
                input_usd_per_million: scaled(i128::from(p.input_rate) * factor, 12),
                cached_usd_per_million: scaled(i128::from(p.cached_rate) * factor, 12),
                cache_write_usd_per_million: scaled(i128::from(p.cache_write_rate) * factor, 12),
                output_usd_per_million: scaled(i128::from(p.output_rate) * factor, 12),
                base_input_usd_per_million: crate::format_units(p.input_rate, 6),
                base_cached_usd_per_million: crate::format_units(p.cached_rate, 6),
                base_cache_write_usd_per_million: crate::format_units(p.cache_write_rate, 6),
                base_output_usd_per_million: crate::format_units(p.output_rate, 6),
            })
            .collect();
        let image_prices = images
            .iter()
            .filter(|p| p.provider_id == config.provider_id && p.model == config.model)
            .map(|p| ApiImagePrice {
                resolution: p.resolution.clone(),
                base_price_usd: crate::format_units(p.price_nano_usd, 9),
                price_usd: scaled(i128::from(p.price_nano_usd) * factor, 15),
            })
            .collect();
        let search_price = (config.provider_id == codex2api_core::CHATGPT && config.kind == "text")
            .then(|| group.and_then(|g| g.search_price_usd.clone()))
            .flatten();
        items.push(ApiCatalogModel {
            provider_id: config.provider_id,
            model: config.model.clone(),
            kind: config.kind,
            multiplier: multiplier.into(),
            group_concurrency_limit: group.map_or(0, |g| g.concurrency_limit),
            group_rpm_limit: group.map_or(0, |g| g.rpm_limit),
            concurrency_limit: rule.map_or(0, |r| r.concurrency_limit),
            rpm_limit: rule.map_or(0, |r| r.rpm_limit),
            prices: token_prices,
            image_prices,
            search_price,
        });
    }
    Ok((total, items))
}
