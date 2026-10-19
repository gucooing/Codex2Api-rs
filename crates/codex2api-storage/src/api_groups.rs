//! API product policy and public prices. Supplier routing never enters user DTOs.
use crate::{ImagePrice, ListPage, ModelConfig, ModelPrice, Result, Storage, StorageError};
use codex2api_common::ModelRule;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// Supplier pools of any platform; execution picks a member that serves the model.
    pub tag_ids: Vec<String>,
    /// Manually granted models with their rules; the group's only model access.
    pub models: Vec<ModelRule>,
    pub multiplier: String,
    pub search_price_usd: Option<String>,
    pub concurrency_limit: u32,
    pub rpm_limit: u32,
    pub revision: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiPublicGroup {
    pub id: String,
    pub name: String,
    pub description: String,
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
    /// Model platform label filter for catalogs.
    pub platform: String,
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
            platform: String::new(),
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
                &self.platform,
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
    enabled: bool,
    tag_ids: sqlx::types::Json<Vec<String>>,
    models: sqlx::types::Json<Vec<ModelRule>>,
    multiplier_micros: i64,
    search_price_nano_usd: Option<i64>,
    #[sqlx(try_from = "i64")]
    concurrency_limit: u32,
    #[sqlx(try_from = "i64")]
    rpm_limit: u32,
    revision: i64,
}
impl TryFrom<GroupRow> for ApiGroup {
    type Error = StorageError;
    fn try_from(row: GroupRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            name: row.name,
            description: row.description,
            enabled: row.enabled,
            tag_ids: row.tag_ids.0,
            models: row.models.0,
            multiplier: crate::format_units(row.multiplier_micros, 6),
            search_price_usd: row
                .search_price_nano_usd
                .map(|price| crate::format_units(price, 9)),
            concurrency_limit: row.concurrency_limit,
            rpm_limit: row.rpm_limit,
            revision: row.revision,
        })
    }
}
const GROUP_COLUMNS: &str = "g.*,COALESCE((SELECT jsonb_agg(tag_id ORDER BY tag_id) FROM api_group_tags WHERE group_id=g.id),'[]'::jsonb) AS tag_ids";

pub fn multiplier_micros(value: &str) -> Result<i64> {
    crate::decimal_units(value, 6)
        .filter(|v| (0..=1_000_000_000).contains(v))
        .ok_or(StorageError::InvalidAdminUpdate(
            "倍率须为 0 到 1000，最多六位小数",
        ))
}

/// Shared by plans and API groups: names, limits and multipliers of a manual model list.
pub(crate) fn validate_model_rules(rules: &[ModelRule], limit: usize) -> Result<()> {
    if rules.len() > limit {
        return Err(StorageError::InvalidAdminUpdate("模型列表过长"));
    }
    let mut seen = BTreeSet::new();
    for rule in rules {
        if !codex2api_common::valid_model(&rule.model)
            || !seen.insert(rule.model.as_str())
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
    Ok(())
}

/// Every listed model must exist in the catalog; deleted tombstones cannot be granted.
pub(crate) async fn require_catalog_models(
    connection: &mut PgConnection,
    models: &[String],
) -> Result<()> {
    let models: Vec<&str> = models
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_catalog WHERE deleted=FALSE AND model=ANY($1)",
    )
    .bind(&models)
    .fetch_one(connection)
    .await?;
    if found != models.len() as i64 {
        return Err(StorageError::InvalidAdminUpdate("请先在计费配置中添加模型"));
    }
    Ok(())
}

pub(crate) async fn require_supplier_tags(
    connection: &mut PgConnection,
    tags: &[String],
) -> Result<()> {
    let tags: Vec<&str> = tags
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let found: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM supplier_tags WHERE id=ANY($1)")
        .bind(&tags)
        .fetch_one(connection)
        .await?;
    if found != tags.len() as i64 {
        return Err(StorageError::InvalidAdminUpdate("标签不存在"));
    }
    Ok(())
}

impl ApiGroup {
    pub fn model_rule(&self, model: &str) -> Option<&ModelRule> {
        self.models.iter().find(|r| r.model == model)
    }
    pub fn allows(&self, model: &str) -> bool {
        self.model_rule(model).is_some()
    }
    pub fn model_multiplier(&self, model: &str) -> &str {
        self.model_rule(model)
            .and_then(|r| r.multiplier.as_deref())
            .unwrap_or(&self.multiplier)
    }
    pub fn model_names(&self) -> Vec<String> {
        self.models.iter().map(|r| r.model.clone()).collect()
    }
    pub(crate) fn public(&self, models_count: i64) -> ApiPublicGroup {
        ApiPublicGroup {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            multiplier: self.multiplier.clone(),
            search_price_usd: self.search_price_usd.clone(),
            concurrency_limit: self.concurrency_limit,
            rpm_limit: self.rpm_limit,
            models_count,
        }
    }
}

pub(crate) async fn group_on(connection: &mut PgConnection, id: &str) -> Result<Option<ApiGroup>> {
    sqlx::query_as::<_, GroupRow>(&format!(
        "SELECT {GROUP_COLUMNS} FROM api_groups g WHERE g.id=$1 AND g.deleted=FALSE"
    ))
    .bind(id)
    .fetch_optional(connection)
    .await?
    .map(TryInto::try_into)
    .transpose()
}

impl Storage {
    pub(crate) async fn api_group_snapshot_on(
        &self,
        conn: &mut PgConnection,
        id: &str,
        revision: i64,
    ) -> Result<Option<ApiGroup>> {
        let key = format!("api-group:{id}");
        let group: Option<ApiGroup> = self
            .runtime
            .snapshots
            .read(key.clone(), async {
                Ok((
                    group_on(conn, id).await?,
                    std::time::Duration::from_secs(300),
                ))
            })
            .await?;
        if group.as_ref().is_some_and(|g| g.revision == revision) {
            return Ok(group);
        }
        self.runtime.snapshots.invalidate_key(&key);
        group_on(conn, id).await
    }
    pub async fn api_group(&self, id: &str) -> Result<Option<ApiGroup>> {
        group_on(&mut *self.pool().acquire().await?, id).await
    }
    pub async fn api_groups(&self) -> Result<Vec<ApiGroup>> {
        sqlx::query_as::<_, GroupRow>(&format!(
            "SELECT {GROUP_COLUMNS} FROM api_groups g WHERE g.deleted=FALSE ORDER BY g.name,g.id"
        ))
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
    pub async fn api_group_page(&self, query: &ApiListQuery) -> Result<ListPage<ApiGroup>> {
        query.validate()?;
        let condition =
            "g.deleted=FALSE AND ($1='' OR strpos(lower(g.name||' '||g.description),lower($2))>0)";
        let mut tx = self
            .pool()
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM api_groups g WHERE {condition}"
        ))
        .bind(&query.search)
        .bind(&query.search)
        .fetch_one(&mut *tx)
        .await?;
        let items=sqlx::query_as::<_, GroupRow>(&format!("SELECT {GROUP_COLUMNS} FROM api_groups g WHERE {condition} ORDER BY g.name,g.id LIMIT $3 OFFSET $4"))
            .bind(&query.search).bind(&query.search)
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
            || group.concurrency_limit > 1_000_000
            || group.rpm_limit > 1_000_000
        {
            return Err(StorageError::InvalidAdminUpdate(
                "API 组名称、介绍、标签或限制无效",
            ));
        }
        validate_model_rules(&group.models, 2048)?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(current_schema() || ':api-group:' || $1,0))")
            .bind(&group.id).execute(&mut *tx).await?;
        let current = group_on(&mut tx, &group.id).await?;
        if current.as_ref().map(|g| g.revision) != expected {
            return Ok(false);
        }
        if expected.is_none()
            && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM api_groups WHERE id=$1)")
                .bind(&group.id)
                .fetch_one(&mut *tx)
                .await?
        {
            return Ok(false);
        }
        require_supplier_tags(&mut tx, &group.tag_ids).await?;
        require_catalog_models(&mut tx, &group.model_names()).await?;
        sqlx::query("INSERT INTO api_groups(id,name,description,enabled,models,multiplier_micros,search_price_nano_usd,concurrency_limit,rpm_limit,created_at) VALUES($1,$2,$3,$4,$5::jsonb,$6,$7,$8,$9,$10) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,enabled=excluded.enabled,models=excluded.models,multiplier_micros=excluded.multiplier_micros,search_price_nano_usd=excluded.search_price_nano_usd,concurrency_limit=excluded.concurrency_limit,rpm_limit=excluded.rpm_limit,revision=api_groups.revision+1")
            .bind(&group.id).bind(group.name.trim()).bind(group.description.trim()).bind(group.enabled)
            .bind(serde_json::to_string(&group.models)?).bind(multiplier).bind(search_price).bind(i64::from(group.concurrency_limit)).bind(i64::from(group.rpm_limit))
            .bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM api_group_tags WHERE group_id=$1")
            .bind(&group.id)
            .execute(&mut *tx)
            .await?;
        for tag in group.tag_ids.iter().collect::<BTreeSet<_>>() {
            sqlx::query("INSERT INTO api_group_tags(group_id,tag_id) VALUES($1,$2)")
                .bind(&group.id)
                .bind(tag)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::ApiGroups]);
        Ok(true)
    }
    pub async fn delete_api_group(&self, id: &str, revision: i64) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let changed=sqlx::query("UPDATE api_groups SET deleted=TRUE,enabled=FALSE,revision=revision+1 WHERE id=$1 AND revision=$2 AND deleted=FALSE").bind(id).bind(revision).execute(&mut *tx).await?.rows_affected()==1;
        if changed {
            sqlx::query("DELETE FROM api_group_tags WHERE group_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::ApiGroups]);
        Ok(changed)
    }
    pub async fn api_group_models(&self, id: &str) -> Result<Vec<ApiCatalogModel>> {
        let mut tx = self
            .pool()
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
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
    pub standard_multiplier: Option<String>,
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
pub(crate) struct ApiOperationPrice {
    pub provider_id: String,
    pub operation: String,
    pub price_usd: Option<String>,
}

fn standard_price_multiplier(price: &ModelPrice, prices: &[ModelPrice]) -> Option<String> {
    if price.tier == "standard" {
        return None;
    }
    let base = prices
        .iter()
        .filter(|p| {
            p.model == price.model
                && p.tier == "standard"
                && p.min_input_tokens <= price.min_input_tokens
        })
        .max_by_key(|p| p.min_input_tokens)?;
    if let Some(end) = base.effective_max_input_tokens(prices)
        && price
            .effective_max_input_tokens(prices)
            .is_none_or(|max| max > end)
    {
        return None;
    }
    let mut ratio = None;
    for (base, value) in [
        (base.input_rate, price.input_rate),
        (base.cached_rate, price.cached_rate),
        (base.cache_write_rate, price.cache_write_rate),
        (base.output_rate, price.output_rate),
    ] {
        if base == 0 {
            if value != 0 {
                return None;
            }
            continue;
        }
        let scaled = i128::from(value) * 1_000_000;
        if scaled % i128::from(base) != 0 {
            return None;
        }
        let next = scaled / i128::from(base);
        if ratio.is_some_and(|previous| previous != next) {
            return None;
        }
        ratio = Some(next);
    }
    ratio.map(|value| scaled(value, 6))
}
#[derive(Clone, Debug, Serialize)]
pub struct ApiCatalogModel {
    pub platform: String,
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

/// One model policy for catalogs and admission. Tags select execution suppliers;
/// the enabled catalog and the group's manual model list grant model access.
pub(crate) fn model_catalog_query<'a>(
    projection: &'static str,
    group: Option<&'a ApiGroup>,
    filters: Option<&'a ApiListQuery>,
) -> Result<QueryBuilder<'a, Postgres>> {
    let mut query = QueryBuilder::new(format!(
        "SELECT {projection} FROM model_catalog m WHERE m.enabled=TRUE AND m.deleted=FALSE"
    ));
    if let Some(group) = group {
        query
            .push(" AND m.model = ANY(")
            .push_bind(group.model_names())
            .push(")");
    }
    if let Some(filters) = filters {
        if !filters.platform.is_empty() {
            query.push(" AND m.platform=").push_bind(&filters.platform);
        }
        if !filters.search.is_empty() {
            query
                .push(" AND strpos(lower(m.model),lower(")
                .push_bind(&filters.search)
                .push("))>0");
        }
    }
    Ok(query)
}

pub(crate) async fn catalog_on(
    connection: &mut PgConnection,
    group: Option<&ApiGroup>,
    query: Option<&ApiListQuery>,
) -> Result<(i64, Vec<ApiCatalogModel>)> {
    let total: i64 = model_catalog_query("COUNT(*)", group, query)?
        .build_query_scalar()
        .fetch_one(&mut *connection)
        .await?;
    let configs: Vec<ModelConfig> = model_catalog_query("m.*", group, query)?
        .push(" ORDER BY m.platform,m.model LIMIT ")
        .push_bind(query.map(|q| i64::from(q.page_size)))
        .push(" OFFSET ")
        .push_bind(query.map_or(0, |q| q.offset_for(total)))
        .build_query_as()
        .fetch_all(&mut *connection)
        .await?;
    let selected: Vec<String> = configs.iter().map(|m| m.model.clone()).collect();
    let prices: Vec<ModelPrice> = sqlx::query_as(
        "SELECT p.* FROM model_prices p WHERE p.model=ANY($1) ORDER BY p.model,p.tier,p.min_input_tokens",
    )
    .bind(&selected)
    .fetch_all(&mut *connection)
    .await?;
    let images: Vec<ImagePrice> = sqlx::query_as(
        "SELECT p.* FROM model_image_prices p WHERE p.model=ANY($1) ORDER BY p.model,p.resolution",
    )
    .bind(&selected)
    .fetch_all(&mut *connection)
    .await?;
    let mut items = Vec::new();
    for config in configs {
        let multiplier = group.map_or("1", |g| g.model_multiplier(&config.model));
        let factor = i128::from(multiplier_micros(multiplier)?);
        let rule = group.and_then(|g| g.model_rule(&config.model));
        let token_prices = prices
            .iter()
            .filter(|p| p.model == config.model)
            .map(|p| ApiTokenPrice {
                tier: p.tier.clone(),
                min_input_tokens: p.min_input_tokens,
                max_input_tokens: p.effective_max_input_tokens(&prices),
                standard_multiplier: standard_price_multiplier(p, &prices),
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
            .filter(|p| p.model == config.model)
            .map(|p| ApiImagePrice {
                resolution: p.resolution.clone(),
                base_price_usd: crate::format_units(p.price_nano_usd, 9),
                price_usd: scaled(i128::from(p.price_nano_usd) * factor, 15),
            })
            .collect();
        // Search is a ChatGPT protocol operation; it is displayed with that platform's text models.
        let search_price = (config.platform == codex2api_common::CHATGPT && config.kind == "text")
            .then(|| group.and_then(|g| g.search_price_usd.clone()))
            .flatten();
        items.push(ApiCatalogModel {
            platform: config.platform,
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
