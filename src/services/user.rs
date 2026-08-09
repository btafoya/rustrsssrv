use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::errors::{AppError, Result};
use crate::models::{CreateUserRequest, User, UserUpdate};
use crate::services::auth::{hash_password, verify_password};
use validator::Validate;

#[derive(Clone)]
pub struct UserService {
    pool: SqlitePool,
}

impl UserService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateUserRequest) -> Result<User> {
        req.validate()?;
        if req.password != req.password_confirmation {
            return Err(AppError::BadRequest("passwords do not match".into()));
        }

        let now = Utc::now();
        let millis = now.timestamp_millis();
        let password_hash = hash_password(req.password)?;

        let id = sqlx::query!(
            r#"
            INSERT INTO users (email, password_hash, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
            req.email,
            password_hash,
            millis,
            millis
        )
        .execute(&self.pool)
        .await?
        .last_insert_rowid();

        Ok(map_user(
            id,
            req.email,
            "UTC".into(),
            "unread".into(),
            "oldest_first".into(),
            None,
            now,
        ))
    }

    pub async fn get_by_id(&self, id: i64) -> Result<User> {
        let row = sqlx::query!(
            r#"SELECT id as "id!", email, timezone, default_filter, default_sort_order, default_feed_id FROM users WHERE id = ?"#,
            id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(map_user(
            row.id,
            row.email,
            row.timezone,
            row.default_filter,
            row.default_sort_order,
            row.default_feed_id,
            Utc::now(),
        ))
    }

    pub async fn set_default_feed_id(&self, id: i64, feed_id: Option<i64>) -> Result<()> {
        sqlx::query!(
            "UPDATE users SET default_feed_id = ? WHERE id = ?",
            feed_id,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update(&self, id: i64, req: UserUpdate) -> Result<User> {
        req.validate()?;
        let user = self.get_by_id(id).await?;

        let mut email = user.email.clone();
        let mut timezone = user.timezone.clone();
        let mut default_filter = user.default_filter.clone();
        let mut default_sort_order = user.default_sort_order.clone();

        if let Some(v) = req.email {
            email = v;
            let existing = sqlx::query!(
                "SELECT id FROM users WHERE email = ? AND id != ?",
                email,
                id
            )
            .fetch_optional(&self.pool)
            .await?;
            if existing.is_some() {
                return Err(AppError::Conflict("email already in use".into()));
            }
        }
        if let Some(v) = req.timezone {
            timezone = v;
        }
        if let Some(v) = req.default_filter {
            if !matches!(v.as_str(), "all" | "unread" | "read" | "starred") {
                return Err(AppError::BadRequest(
                    "default_filter must be 'all', 'unread', 'read', or 'starred'".into(),
                ));
            }
            default_filter = v;
        }
        if let Some(v) = req.default_sort_order {
            if v != "oldest_first" && v != "newest_first" {
                return Err(AppError::BadRequest(
                    "default_sort_order must be 'oldest_first' or 'newest_first'".into(),
                ));
            }
            default_sort_order = v;
        }

        if let Some(new_password) = req.new_password {
            let current = req
                .current_password
                .ok_or_else(|| AppError::BadRequest("current_password required".into()))?;
            let row = sqlx::query!("SELECT password_hash FROM users WHERE id = ?", id)
                .fetch_one(&self.pool)
                .await?;
            if !verify_password(&current, &row.password_hash)? {
                return Err(AppError::Unauthorized);
            }
            let password_hash = hash_password(new_password)?;
            let updated_at = Utc::now().timestamp_millis();
            sqlx::query!(
                "UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?",
                password_hash,
                updated_at,
                id
            )
            .execute(&self.pool)
            .await?;
        }

        let now = Utc::now();
        let updated_at = now.timestamp_millis();
        sqlx::query!(
            "UPDATE users SET email = ?, timezone = ?, default_filter = ?, default_sort_order = ?, updated_at = ? WHERE id = ?",
            email,
            timezone,
            default_filter,
            default_sort_order,
            updated_at,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(map_user(
            id,
            email,
            timezone,
            default_filter,
            default_sort_order,
            user.default_feed_id,
            now,
        ))
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM users WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn count(&self) -> Result<i64> {
        let row = sqlx::query!(r#"SELECT COUNT(*) as "cnt!" FROM users"#)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.cnt)
    }
}

fn map_user(
    id: i64,
    email: String,
    timezone: String,
    default_filter: String,
    default_sort_order: String,
    default_feed_id: Option<i64>,
    now: DateTime<Utc>,
) -> User {
    User {
        id,
        email,
        timezone,
        default_filter,
        default_sort_order,
        default_feed_id,
        created_at: now,
        updated_at: now,
    }
}
