use crate::{ListPage, ListQuery, Result, Storage, StorageError};
use serde_json::Value;

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
