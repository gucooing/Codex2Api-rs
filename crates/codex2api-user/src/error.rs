use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub(crate) type Result<T> = std::result::Result<T, UserError>;
pub(crate) struct UserError(pub StatusCode, pub &'static str, pub &'static str);
impl UserError {
    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "unauthorized", "请登录用户账户")
    }
    pub fn bad(message: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, "invalid_request", message)
    }
    pub fn missing() -> Self {
        Self(StatusCode::NOT_FOUND, "not_found", "记录不存在")
    }
}
impl IntoResponse for UserError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(serde_json::json!({"error":{"code":self.1,"message":self.2}})),
        )
            .into_response()
    }
}
impl From<codex2api_storage::StorageError> for UserError {
    fn from(error: codex2api_storage::StorageError) -> Self {
        use codex2api_storage::StorageError as E;
        match error {
            E::Checkout { code, message } => Self(StatusCode::UNPROCESSABLE_ENTITY, code, message),
            E::InvalidCredentials | E::SessionNotFound => Self::unauthorized(),
            E::AccountNotFound(_) => Self::missing(),
            E::InvalidAdminUpdate(message) => Self::bad(message),
            E::ProxyChanged => Self(
                StatusCode::CONFLICT,
                "revision_conflict",
                "数据已变更，请刷新后重试",
            ),
            _ => {
                tracing::error!(%error,"user operation failed");
                Self(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "operation_failed",
                    "操作失败，请稍后重试",
                )
            }
        }
    }
}
