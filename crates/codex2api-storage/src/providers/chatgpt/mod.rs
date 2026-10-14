use crate::{ListPage, ListQuery, Result, Storage, StorageError};
use serde_json::Value;

pub(crate) async fn catalog_operation_prices(
    connection: &mut sqlx::SqliteConnection,
    group: Option<&crate::ApiGroup>,
) -> Result<Vec<crate::api_groups::ApiOperationPrice>> {
    use codex2api_core::{CHATGPT, providers::chatgpt::SEARCH_OPERATION};
    let price_usd = if let Some(group) = group {
        group.search_price_usd.clone()
    } else {
        let price: Option<i64> = sqlx::query_scalar(
            "SELECT price_nano_usd FROM operation_prices WHERE provider_id=? AND operation=?",
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
        let source = match query.kind.as_str() {
            "details" => "supplier_info_cache c,json_each(c.response_json,'$.accounts') j",
            "credits" => "supplier_info_cache c,json_each(c.response_json,'$.credits') j",
            _ => return Err(StorageError::InvalidAdminUpdate("未知官方记录类别")),
        };
        self.read_list::<(String,)>(query, "j.value", source, "j.key", |q| {
            q.push(" AND c.account_id=")
                .push_bind(owner.to_owned())
                .push(" AND c.section=")
                .push_bind(query.kind.clone())
                .push(" AND j.type='object'");
        })
        .await?
        .try_map(|(text,)| Ok(serde_json::from_str(&text)?))
    }
}
