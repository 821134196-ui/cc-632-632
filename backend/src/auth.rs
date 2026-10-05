use crate::db::Db;
use crate::error::{AppError, AppResult};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use sha2::{Digest, Sha256};

pub fn hash_password(pw: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"meet-salt:");
    h.update(pw.as_bytes());
    format!("{:x}", h.finalize())
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    hash_password(pw) == hash
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
}

/// 从请求中解析当前登录用户（需要 `Authorization: Bearer <token>`）
impl FromRequestParts<Db> for User {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Db,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| AppError::new(StatusCode::UNAUTHORIZED, "未登录"))?;

        let user = sqlx::query_as::<_, User>(
            "SELECT u.* FROM users u JOIN sessions s ON s.user_id = u.id WHERE s.token = ?",
        )
        .bind(token)
        .fetch_optional(state)
        .await?
        .ok_or_else(|| AppError::new(StatusCode::UNAUTHORIZED, "登录已失效"))?;

        Ok(user)
    }
}

impl User {
    pub fn require_role(&self, roles: &[&str]) -> AppResult<()> {
        if roles.contains(&self.role.as_str()) {
            Ok(())
        } else {
            Err(AppError::forbidden(format!(
                "当前角色「{}」无权执行此操作",
                self.display_name
            )))
        }
    }
}

pub fn new_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = Sha256::new();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    h.update(b"session-");
    h.update(nanos.to_le_bytes());
    h.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    format!("{:x}", h.finalize())
}
