use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

use crate::error::ApiError;
use crate::AppState;

/// 通过 X-User-Id 头识别当前工作人员
pub struct AuthUser {
    pub id: i64,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "缺少有效的 X-User-Id 请求头",
                )
            })?;
        Ok(AuthUser { id })
    }
}

pub async fn require_role(
    state: &AppState,
    user_id: i64,
    allowed: &[&str],
) -> Result<(String, String), ApiError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT display_name, role FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;
    let (name, role) = row.ok_or_else(|| ApiError::unauthorized("用户不存在"))?;
    if !allowed.contains(&role.as_str()) {
        return Err(ApiError::forbidden(format!(
            "当前角色“{role}”无权执行该操作，需要: {}",
            allowed.join(" / ")
        )));
    }
    Ok((name, role))
}

#[allow(dead_code)]
pub fn json_error(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, axum::Json(json!({ "error": msg }))).into_response()
}
