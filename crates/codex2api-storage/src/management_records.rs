use crate::{ListPage, ListQuery, MissingEndpoint, RemoteServer, Result, Storage, VirtualDevice};
use serde_json::Value;

impl Storage {
    pub async fn client_state_rows(
        &self,
        owner: &str,
        key: &str,
        section: &str,
        entries: bool,
        filter: &ListQuery,
    ) -> Result<ListPage<Value>> {
        let size = filter.validate()?;
        if !section.starts_with('$') || section.len() > 1024 {
            return Err(crate::StorageError::InvalidAdminUpdate("记录路径无效"));
        }
        let mut tx = self.pool().begin().await?;
        let total:i64 = sqlx::query_scalar("SELECT COUNT(*) FROM virtual_client_state s,json_each(s.value_json,?) j WHERE s.virtual_account_id=? AND s.state_key=?")
            .bind(section).bind(owner).bind(key).fetch_one(&mut *tx).await?;
        let page = i64::from(filter.page).min((total.saturating_sub(1) / size + 1).max(1)) as u32;
        let rows:Vec<(String,)> = sqlx::query_as("SELECT CASE WHEN ? THEN json_array(j.key,json(CASE WHEN j.type IN('object','array') THEN j.value ELSE json_quote(j.value) END)) ELSE CASE WHEN j.type IN('object','array') THEN j.value ELSE json_quote(j.value) END END FROM virtual_client_state s,json_each(s.value_json,?) j WHERE s.virtual_account_id=? AND s.state_key=? ORDER BY j.key LIMIT ? OFFSET ?")
            .bind(entries).bind(section).bind(owner).bind(key).bind(size).bind(i64::from(page-1)*size).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(ListPage {
            items: rows
                .into_iter()
                .map(|(v,)| serde_json::from_str(&v).map_err(crate::StorageError::from))
                .collect::<Result<_>>()?,
            total,
            page,
            page_size: size,
        })
    }
    pub async fn device_page(
        &self,
        owner: &str,
        filter: &ListQuery,
    ) -> Result<ListPage<VirtualDevice>> {
        self.read_list(
            filter,
            "d.*",
            "virtual_devices d",
            "d.created_at DESC,d.id",
            |q| {
                q.push(" AND d.virtual_account_id=")
                    .push_bind(owner.to_owned());
            },
        )
        .await
    }

    pub async fn remote_server_page(
        &self,
        owner: &str,
        filter: &ListQuery,
    ) -> Result<ListPage<RemoteServer>> {
        self.read_list(
            filter,
            "s.*",
            "virtual_remote_servers s",
            "s.last_seen_at_ms DESC,s.id",
            |q| {
                q.push(" AND s.virtual_account_id=")
                    .push_bind(owner.to_owned());
            },
        )
        .await
    }

    pub async fn resource_record_page(
        &self,
        owner: &str,
        filter: &ListQuery,
    ) -> Result<ListPage<Value>> {
        let kind = match filter.kind.as_str() {
            "family_notices" => "family_graduation_notice",
            "" => "task",
            value => value,
        };
        let raw = matches!(
            kind,
            "family_graduation_notice" | "analytics" | "site_status"
        );
        let columns = if raw {
            "json(r.value_json)"
        } else {
            "json_object('id',r.id,'owner',r.virtual_account_id,'source',r.upstream_account_id,'value',json(r.value_json),'created_at_ms',r.created_at_ms,'updated_at_ms',r.updated_at_ms)"
        };
        let page = self
            .read_list::<(String,)>(
                filter,
                columns,
                "virtual_resources r",
                "r.updated_at_ms DESC,r.id",
                |q| {
                    q.push(" AND r.virtual_account_id=")
                        .push_bind(owner.to_owned())
                        .push(" AND r.kind=")
                        .push_bind(kind.to_owned());
                },
            )
            .await?;
        page.try_map(|(value,)| Ok(serde_json::from_str(&value)?))
    }

    pub async fn cloud_environment_page(
        &self,
        owner: &str,
        filter: &ListQuery,
    ) -> Result<ListPage<Value>> {
        let source = "(SELECT *,row_number() OVER(PARTITION BY virtual_account_id,json_extract(environment,'$.id') ORDER BY updated_at_ms DESC,id) AS position FROM (SELECT *,COALESCE(json_extract(value_json,'$.task.environment'),json_extract(value_json,'$.environment')) AS environment FROM virtual_resources WHERE kind='task')) r";
        let page = self
            .read_list::<(String,)>(
                filter,
                "json(r.environment)",
                source,
                "json_extract(r.environment,'$.id')",
                |q| {
                    q.push(" AND r.virtual_account_id=")
                        .push_bind(owner.to_owned())
                        .push(" AND r.environment IS NOT NULL AND r.position=1");
                },
            )
            .await?;
        for (raw,) in &page.items {
            let value: Value = serde_json::from_str(raw)?;
            if !value["id"].is_string()
                || !value["label"].is_string()
                || !value["repos"].is_array()
                || !value["repo_map"].is_object()
            {
                return Err(crate::StorageError::InvalidAdminUpdate(
                    "Saved cloud environment metadata is incomplete",
                ));
            }
        }
        page.try_map(|(value,)| Ok(serde_json::from_str(&value)?))
    }

    pub async fn desktop_resource_page(&self, filter: &ListQuery) -> Result<ListPage<Value>> {
        let page = self
            .read_list::<(String,)>(
                filter,
                "json_object('path',path,'bytes',length(content),'fetched_at_ms',fetched_at_ms)",
                "desktop_public_resources",
                "path",
                |_| {},
            )
            .await?;
        page.try_map(|(value,)| Ok(serde_json::from_str(&value)?))
    }

    pub async fn missing_endpoint_page(
        &self,
        filter: &ListQuery,
    ) -> Result<ListPage<MissingEndpoint>> {
        self.read_list(filter, "m.*,strftime('%Y-%m-%dT%H:%M:%f+00:00',s.at/1000.0,'unixepoch') AS last_success_at",
            "oauth_missing_endpoints m LEFT JOIN (SELECT method,path,MAX(at) AS at FROM (SELECT method,path,created_at_ms AS at FROM virtual_request_logs WHERE status BETWEEN 200 AND 299 UNION ALL SELECT 'GET','/api/oauth/chatgpt'||path,fetched_at_ms FROM desktop_public_resources) GROUP BY method,path) s ON s.method=m.method AND s.path=m.path",
            "m.last_seen_at DESC,m.method,m.path", |_| {}).await
    }
}
