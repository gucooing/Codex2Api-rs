use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, sqlite::SqliteRow};

/// Management list parameters. SQL identifiers and ordering are chosen by the repository.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListQuery {
    pub page: u32,
    pub page_size: Option<u32>,
    pub search: String,
    pub provider_id: String,
    pub status: String,
    pub kind: String,
    pub protocol: String,
    pub result: String,
    pub tag: String,
    pub user_id: String,
    pub plan_id: String,
    pub include_expired: bool,
    pub subscription: String,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: None,
            search: String::new(),
            provider_id: String::new(),
            status: String::new(),
            kind: String::new(),
            protocol: String::new(),
            result: String::new(),
            tag: String::new(),
            user_id: String::new(),
            plan_id: String::new(),
            include_expired: false,
            subscription: String::new(),
        }
    }
}

impl ListQuery {
    pub fn validate(&self) -> Result<i64> {
        if self.page == 0
            || [
                &self.search,
                &self.provider_id,
                &self.status,
                &self.kind,
                &self.protocol,
                &self.result,
                &self.tag,
                &self.user_id,
                &self.plan_id,
                &self.subscription,
            ]
            .iter()
            .any(|value| value.len() > 1024)
        {
            return Err(StorageError::InvalidAdminUpdate("列表查询参数无效"));
        }
        crate::table_page_size(self.page_size)
    }
}

#[derive(Debug, Serialize)]
pub struct ListPage<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: i64,
}

impl<T> ListPage<T> {
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> ListPage<U> {
        ListPage {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            page: self.page,
            page_size: self.page_size,
        }
    }
    pub fn try_map<U>(self, f: impl FnMut(T) -> Result<U>) -> Result<ListPage<U>> {
        Ok(ListPage {
            items: self.items.into_iter().map(f).collect::<Result<_>>()?,
            total: self.total,
            page: self.page,
            page_size: self.page_size,
        })
    }
}

pub(crate) fn exact(query: &mut QueryBuilder<'static, Sqlite>, column: &str, value: &str) {
    if !value.is_empty() {
        query
            .push(" AND ")
            .push(column)
            .push("=")
            .push_bind(value.to_owned());
    }
}

pub(crate) fn search(query: &mut QueryBuilder<'static, Sqlite>, columns: &[&str], value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    query.push(" AND (");
    for (index, column) in columns.iter().enumerate() {
        if index != 0 {
            query.push(" OR ");
        }
        query
            .push("instr(lower(COALESCE(")
            .push(column)
            .push(",'')),lower(")
            .push_bind(value.to_owned())
            .push("))>0");
    }
    query.push(")");
}

impl Storage {
    /// Count and rows share one predicate and read snapshot, with a stable final key.
    pub(crate) async fn read_list<T>(
        &self,
        request: &ListQuery,
        columns: &str,
        source: &str,
        order: &str,
        predicate: impl Fn(&mut QueryBuilder<'static, Sqlite>),
    ) -> Result<ListPage<T>>
    where
        T: for<'r> FromRow<'r, SqliteRow> + Send + Unpin,
    {
        let page_size = request.validate()?;
        let mut tx = self.pool().begin().await?;
        let mut count = QueryBuilder::new(format!("SELECT COUNT(*) FROM {source} WHERE 1=1"));
        predicate(&mut count);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *tx).await?;
        let page =
            i64::from(request.page).min((total.saturating_sub(1) / page_size + 1).max(1)) as u32;
        let mut rows = QueryBuilder::new(format!("SELECT {columns} FROM {source} WHERE 1=1"));
        predicate(&mut rows);
        rows.push(" ORDER BY ")
            .push(order)
            .push(" LIMIT ")
            .push_bind(page_size)
            .push(" OFFSET ")
            .push_bind(i64::from(page - 1) * page_size);
        let items = rows.build_query_as().fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(ListPage {
            items,
            total,
            page,
            page_size,
        })
    }
}
