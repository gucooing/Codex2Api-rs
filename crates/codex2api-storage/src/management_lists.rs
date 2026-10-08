use crate::list_query::{exact, search};
use crate::{
    ListPage, ListQuery, ModelConfig, OutboundProxy, Result, Storage, SupplierTag, User,
    UserSubscription, VirtualPlan,
};
use sqlx::{QueryBuilder, Sqlite};

const SUPPLIERS: &str = "supplier_accounts a LEFT JOIN supplier_tokens k ON k.account_id=a.id LEFT JOIN supplier_health h ON h.account_id=a.id";
const SUPPLIER_STATUS: &str = "CASE WHEN a.status!='active' THEN 'disabled' WHEN COALESCE(k.access_token,'')='' OR h.rejected_auth_revision=a.auth_revision THEN 'error' WHEN h.payment_required_at IS NOT NULL THEN 'payment_required' WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>unixepoch() THEN 'quota_exhausted' ELSE 'active' END";

pub(crate) fn supplier_predicate(q: &mut QueryBuilder<'static, Sqlite>, filter: &ListQuery) {
    search(
        q,
        &["a.display_name", "a.email", "a.provider_id"],
        &filter.search,
    );
    exact(q, "a.provider_id", &filter.provider_id);
    exact(q, SUPPLIER_STATUS, &filter.status);
    match filter.tag.as_str() {
        "" => {}
        "__untagged__" => {
            q.push(" AND NOT EXISTS(SELECT 1 FROM supplier_tag_members m WHERE m.account_id=a.id)");
        }
        tag => {
            q.push(" AND EXISTS(SELECT 1 FROM supplier_tag_members m WHERE m.account_id=a.id AND m.tag_id=").push_bind(tag.to_owned()).push(")");
        }
    }
}

impl Storage {
    pub async fn plan_options(
        &self,
        provider: &str,
        paid_only: bool,
    ) -> Result<Vec<serde_json::Value>> {
        let rows: Vec<(String,String,String,String)> = sqlx::query_as("SELECT id,name,provider_id,plan_type FROM virtual_plans WHERE (?='' OR provider_id=?) AND (?=0 OR plan_type!='free') ORDER BY provider_id,name,id")
            .bind(provider).bind(provider).bind(paid_only).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|(id,name,provider,kind)|serde_json::json!({"id":id,"name":name,"provider_id":provider,"plan_type":kind})).collect())
    }

    pub async fn model_options(
        &self,
        provider: &str,
        search: &str,
        plan_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        if search.len() > 1024 || plan_id.len() > 128 {
            return Err(crate::StorageError::InvalidAdminUpdate("模型查询参数无效"));
        }
        let rows: Vec<(String,String,String,bool)> = sqlx::query_as("SELECT provider_id,model,kind,enabled FROM model_catalog m WHERE deleted=0 AND (?='' OR provider_id=?) AND instr(lower(model),lower(?))>0 AND (enabled=1 OR EXISTS(SELECT 1 FROM virtual_plans p,json_each(p.config,'$.models') s WHERE p.id=? AND json_extract(s.value,'$.provider_id')=m.provider_id AND json_extract(s.value,'$.model')=m.model)) ORDER BY provider_id,model")
            .bind(provider).bind(provider).bind(search.trim()).bind(plan_id).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|(provider,model,kind,enabled)|serde_json::json!({"provider_id":provider,"model":model,"kind":kind,"enabled":enabled})).collect())
    }

    pub async fn tag_options(&self, provider: &str) -> Result<Vec<SupplierTag>> {
        Ok(sqlx::query_as("SELECT t.*,(SELECT COUNT(*) FROM supplier_tag_members m WHERE m.tag_id=t.id) AS supplier_count,(SELECT COUNT(*) FROM execution_routes r WHERE r.tag_id=t.id) AS binding_count FROM supplier_tags t WHERE (?='' OR t.provider_id=?) ORDER BY t.provider_id,t.name,t.id")
            .bind(provider).bind(provider).fetch_all(self.pool()).await?)
    }

    pub async fn proxy_options(&self) -> Result<Vec<serde_json::Value>> {
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT id,name,url FROM outbound_proxies ORDER BY name,id")
                .fetch_all(self.pool())
                .await?;
        rows.into_iter()
            .map(|(id, name, url)| {
                let mut url = crate::parse_proxy_url(&url)?;
                let _ = url.set_username("");
                let _ = url.set_password(None);
                Ok(serde_json::json!({"id":id,"name":name,"display_url":url.to_string()}))
            })
            .collect()
    }

    pub async fn model_token_prices(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Vec<crate::ModelPrice>> {
        Ok(sqlx::query_as("SELECT * FROM model_prices WHERE provider_id=? AND model=? ORDER BY tier,min_input_tokens")
            .bind(provider).bind(model).fetch_all(self.pool()).await?)
    }

    pub async fn model_image_prices(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Vec<crate::ImagePrice>> {
        Ok(sqlx::query_as(
            "SELECT * FROM model_image_prices WHERE provider_id=? AND model=? ORDER BY resolution",
        )
        .bind(provider)
        .bind(model)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn outbound_proxy_account_count(&self, id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM supplier_accounts WHERE proxy_id=?")
                .bind(id)
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub async fn wallet_entry_page(
        &self,
        owner: &str,
        filter: &ListQuery,
    ) -> Result<ListPage<crate::WalletEntry>> {
        self.read_list(
            filter,
            "w.*",
            "wallet_entries w JOIN regular_users u ON u.id=w.user_id",
            "julianday(w.created_at) DESC,w.rowid DESC",
            |q| {
                q.push(" AND w.user_id=").push_bind(owner.to_owned());
            },
        )
        .await
    }

    pub async fn supplier_tag_labels(&self, account: &str) -> Result<Vec<(String, String)>> {
        Ok(sqlx::query_as("SELECT t.id,t.name FROM supplier_tags t JOIN supplier_tag_members m ON m.tag_id=t.id WHERE m.account_id=? ORDER BY t.id").bind(account).fetch_all(self.pool()).await?)
    }

    pub async fn supplier_page(&self, filter: &ListQuery) -> Result<ListPage<String>> {
        let page = self
            .read_list::<(String,)>(filter, "a.id", SUPPLIERS, "a.created_at,a.id", |q| {
                supplier_predicate(q, filter)
            })
            .await?;
        Ok(page.map(|(id,)| id))
    }

    pub async fn supplier_selection(&self, filter: &ListQuery) -> Result<Vec<serde_json::Value>> {
        filter.validate()?;
        let mut q = QueryBuilder::new(format!(
            "SELECT a.id,a.provider_id,(SELECT json_group_array(tag_id) FROM supplier_tag_members WHERE account_id=a.id) FROM {SUPPLIERS} WHERE 1=1"
        ));
        supplier_predicate(&mut q, filter);
        q.push(" ORDER BY a.created_at,a.id LIMIT 1001");
        let rows: Vec<(String, String, String)> = q.build_query_as().fetch_all(self.pool()).await?;
        if rows.len() > 1000 {
            return Err(crate::StorageError::InvalidAdminUpdate(
                "每次最多选择 1000 个供应账户，请缩小筛选范围",
            ));
        }
        rows.into_iter().map(|(id,provider,tags)| Ok(serde_json::json!({"id":id,"provider_id":provider,"tag_ids":serde_json::from_str::<Vec<String>>(&tags)?}))).collect()
    }

    pub async fn supplier_options(
        &self,
        search: &str,
        limit: u32,
        provider: Option<&str>,
        tag: Option<&str>,
        routing: bool,
    ) -> Result<Vec<serde_json::Value>> {
        let filter = ListQuery {
            search: search.into(),
            provider_id: provider.unwrap_or("").into(),
            tag: tag.unwrap_or("").into(),
            status: if routing {
                "active".into()
            } else {
                String::new()
            },
            ..ListQuery::default()
        };
        filter.validate()?;
        let mut q = QueryBuilder::new(format!(
            "SELECT a.id,a.display_name,a.email,a.provider_id FROM {SUPPLIERS} WHERE 1=1"
        ));
        supplier_predicate(&mut q, &filter);
        q.push(" ORDER BY a.created_at,a.id LIMIT ")
            .push_bind(i64::from(limit));
        let rows: Vec<(String, Option<String>, Option<String>, String)> =
            q.build_query_as().fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|(id,name,email,provider)|serde_json::json!({"id":id,"display_name":name,"email":email,"provider_id":provider})).collect())
    }

    pub async fn user_page(&self, filter: &ListQuery) -> Result<ListPage<User>> {
        self.read_list(
            filter,
            "u.*",
            "regular_users u",
            "u.created_at DESC,u.id",
            |q| {
                search(q, &["u.username", "u.name", "u.email"], &filter.search);
            },
        )
        .await
    }

    pub async fn plan_page(&self, filter: &ListQuery) -> Result<ListPage<VirtualPlan>> {
        self.read_list(filter, "p.*", "virtual_plans p", "p.rowid", |q| {
            search(
                q,
                &["p.name", "p.provider_id", "p.plan_type"],
                &filter.search,
            );
            exact(q, "p.provider_id", &filter.provider_id);
        })
        .await
    }

    pub async fn model_page(&self, filter: &ListQuery) -> Result<ListPage<ModelConfig>> {
        if !matches!(filter.status.as_str(), "" | "enabled" | "disabled")
            || !matches!(filter.kind.as_str(), "" | "text" | "image")
        {
            return Err(crate::StorageError::InvalidAdminUpdate("模型筛选条件无效"));
        }
        self.read_list(
            filter,
            "m.*",
            "model_catalog m",
            "m.provider_id,m.model",
            |q| {
                q.push(" AND m.deleted=0");
                search(q, &["m.model", "m.provider_id"], &filter.search);
                exact(q, "m.provider_id", &filter.provider_id);
                exact(q, "m.kind", &filter.kind);
                if !filter.status.is_empty() {
                    q.push(" AND m.enabled=")
                        .push_bind(filter.status == "enabled");
                }
            },
        )
        .await
    }

    pub async fn proxy_page(&self, filter: &ListQuery) -> Result<ListPage<OutboundProxy>> {
        if !matches!(
            filter.result.as_str(),
            "" | "success" | "failed" | "unchecked"
        ) || !matches!(
            filter.protocol.as_str(),
            "" | "http" | "https" | "socks5" | "socks5h"
        ) {
            return Err(crate::StorageError::InvalidAdminUpdate("代理筛选条件无效"));
        }
        // Credentials are excluded from searchable URL components.
        let source = "(SELECT p.*, CASE WHEN substr(authority,1,1)='[' THEN substr(authority,2,instr(authority,']')-2) WHEN instr(authority,':')>0 THEN substr(authority,1,instr(authority,':')-1) ELSE authority END AS host FROM (SELECT p.*, CASE WHEN instr(address,'@')>0 THEN substr(address,instr(address,'@')+1) ELSE address END AS authority FROM (SELECT p.*, substr(url,1,instr(url,'://')-1) AS protocol, rtrim(substr(url,instr(url,'://')+3),'/') AS address FROM outbound_proxies p) p) p) p";
        self.read_list(filter, "p.*", source, "p.created_at,p.id", |q| {
            search(q, &["p.name", "p.host"], &filter.search);
            exact(q, "p.protocol", &filter.protocol);
            match filter.result.as_str() {
                "unchecked" => {
                    q.push(" AND p.connection_ok IS NULL");
                }
                "success" => {
                    q.push(" AND p.connection_ok=1");
                }
                "failed" => {
                    q.push(" AND p.connection_ok=0");
                }
                _ => {}
            }
        })
        .await
    }

    pub async fn tag_page(&self, filter: &ListQuery) -> Result<ListPage<SupplierTag>> {
        self.read_list(filter, "t.*,(SELECT COUNT(*) FROM supplier_tag_members m WHERE m.tag_id=t.id) AS supplier_count,(SELECT COUNT(*) FROM execution_routes r WHERE r.tag_id=t.id) AS binding_count",
            "supplier_tags t", "t.provider_id,t.name,t.id", |q| {
                search(q, &["t.name"], &filter.search);
                exact(q, "t.provider_id", &filter.provider_id);
            }).await
    }

    pub async fn subscription_page(
        &self,
        filter: &ListQuery,
    ) -> Result<ListPage<UserSubscription>> {
        let now = chrono::Utc::now().timestamp();
        self.read_list(filter, "v.user_id,u.username,u.name,v.id AS virtual_account_id,v.provider_id,v.plan_id,p.name AS plan_name,v.plan_type,v.subscription_expires_at,v.enabled,v.revision,v.created_at",
            "platform_accounts v JOIN regular_users u ON u.id=v.user_id JOIN virtual_plans p ON p.id=v.plan_id",
            "v.created_at DESC,v.user_id,v.provider_id,v.id", |q| {
                exact(q, "v.user_id", &filter.user_id);
                exact(q, "v.plan_id", &filter.plan_id);
                exact(q, "v.provider_id", &filter.provider_id);
                if !filter.include_expired { q.push(" AND (v.subscription_expires_at IS NULL OR unixepoch(v.subscription_expires_at)>").push_bind(now).push(")"); }
            }).await
    }
}
