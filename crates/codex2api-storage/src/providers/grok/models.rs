use crate::{Result, Storage};
use serde_json::{Value, json};
impl Storage {
    pub async fn grok_consumer_models(&self, owner: &str) -> Result<Vec<Value>> {
        let allowed = self
            .available_virtual_models(owner, codex2api_core::GROK)
            .await?;
        let rows:Vec<(String,)>=sqlx::query_as("SELECT sm.descriptor_json FROM supplier_models sm JOIN supplier_accounts a ON a.id=sm.account_id JOIN supplier_tag_members m ON m.account_id=a.id JOIN effective_execution_routes r ON r.tag_id=m.tag_id WHERE r.virtual_account_id=$1 AND a.provider_id='grok' ORDER BY sm.observed_at DESC,a.id").bind(owner).fetch_all(self.pool()).await?;
        let descriptors: Vec<Value> = rows
            .into_iter()
            .filter_map(|(s,)| serde_json::from_str(&s).ok())
            .collect();
        Ok(allowed.into_iter().filter(|m|m.kind=="text").map(|m|{
            descriptors.iter().find(|d|d["model"]==m.model).cloned().unwrap_or_else(||json!({"id":m.model,"model":m.model,"name":m.model,"object":"model","api_backend":"responses"}))
        }).collect())
    }
}
