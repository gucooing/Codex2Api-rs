use crate::{ListPage, ListQuery, Result, Storage, StorageError};
use serde_json::Value;

pub(crate) async fn catalog_operation_prices(
    connection: &mut sqlx::PgConnection,
    group: Option<&crate::ApiGroup>,
) -> Result<Vec<crate::api_groups::ApiOperationPrice>> {
    use codex2api_core::{CHATGPT, providers::chatgpt::SEARCH_OPERATION};
    let price_usd = if let Some(group) = group {
        group.search_price_usd.clone()
    } else {
        let price: Option<i64> = sqlx::query_scalar(
            "SELECT price_nano_usd FROM operation_prices WHERE provider_id=$1 AND operation=$2",
        )
        .bind(CHATGPT)
        .bind(SEARCH_OPERATION)
        .fetch_optional(connection)
        .await?
        .flatten();
        price.map(|price| crate::format_units(price, 9))
    };
    Ok(vec![crate::api_groups::ApiOperationPrice {
        provider_id: CHATGPT.into(),
        operation: SEARCH_OPERATION.into(),
        price_usd,
    }])
}

impl Storage {
    pub async fn chatgpt_official_page(
        &self,
        owner: &str,
        query: &ListQuery,
    ) -> Result<ListPage<Value>> {
        let field = match query.kind.as_str() {
            "details" => "accounts",
            "credits" => "credits",
            _ => return Err(StorageError::InvalidAdminUpdate("未知官方记录类别")),
        };
        let value = format!("c.response_json::jsonb->'{field}'");
        let source = format!("supplier_info_cache c CROSS JOIN LATERAL (
            SELECT key,value FROM jsonb_each(CASE WHEN jsonb_typeof({value})='object' THEN {value} ELSE '{{}}'::jsonb END)
            UNION ALL SELECT (ordinality-1)::text,value FROM jsonb_array_elements(CASE WHEN jsonb_typeof({value})='array' THEN {value} ELSE '[]'::jsonb END) WITH ORDINALITY) j");
        self.read_list::<(String,)>(query, "j.value::text", &source, "j.key", |q| {
            q.push(" AND c.account_id=")
                .push_bind(owner.to_owned())
                .push(" AND c.section=")
                .push_bind(query.kind.clone())
                .push(" AND jsonb_typeof(j.value)='object'");
        })
        .await?
        .try_map(|(text,)| Ok(serde_json::from_str(&text)?))
    }
}
