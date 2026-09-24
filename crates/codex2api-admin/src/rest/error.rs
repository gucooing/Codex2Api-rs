use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
pub(crate) type ApiResult = Result<Json<Value>, ApiError>;
pub(crate) struct ApiError(pub StatusCode, pub String, pub String);
impl ApiError {
    pub fn unauthorized() -> Self {
        Self(
            StatusCode::UNAUTHORIZED,
            "unauthorized".into(),
            "请登录管理账户".into(),
        )
    }

    pub fn bad(message: impl Into<String>) -> Self {
        Self(
            StatusCode::BAD_REQUEST,
            "invalid_request".into(),
            message.into(),
        )
    }
    pub fn missing() -> Self {
        Self(
            StatusCode::NOT_FOUND,
            "not_found".into(),
            "记录不存在".into(),
        )
    }
    pub fn conflict() -> Self {
        Self(
            StatusCode::CONFLICT,
            "revision_conflict".into(),
            "记录已修改，请重新加载后再保存".into(),
        )
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self(StatusCode::FORBIDDEN, "forbidden".into(), message.into())
    }
    pub fn upstream(message: impl Into<String>) -> Self {
        Self(
            StatusCode::BAD_GATEWAY,
            "upstream_error".into(),
            message.into(),
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":{"code":self.1,"message":self.2}})),
        )
            .into_response()
    }
}
impl From<codex2api_storage::StorageError> for ApiError {
    fn from(e: codex2api_storage::StorageError) -> Self {
        use codex2api_storage::StorageError as E;
        match e {
            E::AccountNotFound(_) | E::ProxyNotFound => Self::missing(),
            E::InvalidAdminUpdate(m) => Self::bad(m),
            E::Constraint(_) => Self::bad("数据不符合业务约束或记录已存在"),
            E::InvalidCredentials => Self::bad("原用户名或原密码错误"),
            E::OAuthCredentialUnavailable => Self::bad("请选择同一提供商已完成授权的供应账户"),
            E::InvalidProxy => Self::bad("出站代理配置无效"),
            E::ProxyChanged => Self::conflict(),
            _ => {
                tracing::error!(error=%e,"admin storage operation failed");
                Self(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error".into(),
                    "操作失败，请稍后重试".into(),
                )
            }
        }
    }
}
pub(crate) fn ok() -> Json<Value> {
    Json(json!({"ok":true}))
}
