use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::{AppError, Result};
use crate::models::{LoginRequest, LoginResponse, RefreshRequest, RefreshResponse, User};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i64,
    pub email: String,
    pub iat: i64,
    pub exp: i64,
    pub typ: String,
}

#[derive(Clone)]
pub struct AuthService {
    pool: SqlitePool,
    jwt_secret: String,
}

impl AuthService {
    pub fn new(pool: SqlitePool, jwt_secret: String) -> Self {
        Self { pool, jwt_secret }
    }

    pub async fn register(&self, email: String, password: String) -> Result<User> {
        let password_hash = hash_password(password)?;
        let now = Utc::now();
        let millis = now.timestamp_millis();

        let id = sqlx::query!(
            r#"
            INSERT INTO users (email, password_hash, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
            email,
            password_hash,
            millis,
            millis
        )
        .execute(&self.pool)
        .await?
        .last_insert_rowid();

        Ok(User {
            id,
            email,
            timezone: "UTC".into(),
            default_filter: "unread".into(),
            default_sort_order: "oldest_first".into(),
            default_feed_id: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn login(&self, req: LoginRequest) -> Result<LoginResponse> {
        let row = sqlx::query!(
            r#"SELECT id as "id!", email, password_hash, timezone, default_filter, default_sort_order, default_feed_id FROM users WHERE email = ?"#,
            req.email
        )
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(AppError::Unauthorized)?;
        if !verify_password(&req.password, &row.password_hash)? {
            return Err(AppError::Unauthorized);
        }

        let user = User {
            id: row.id,
            email: row.email,
            timezone: row.timezone,
            default_filter: row.default_filter,
            default_sort_order: row.default_sort_order,
            default_feed_id: row.default_feed_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let access = self.create_access_token(&user)?;
        let refresh = self.create_refresh_token(row.id).await?;

        Ok(LoginResponse {
            access_token: access,
            refresh_token: refresh,
            token_type: "Bearer".into(),
            expires_in: 7 * 24 * 60 * 60,
        })
    }

    pub async fn refresh(&self, req: RefreshRequest) -> Result<RefreshResponse> {
        let hash = blake3_hash(&req.refresh_token);
        let token = sqlx::query!(
            "SELECT user_id, expires_at FROM refresh_tokens WHERE token_hash = ?",
            hash
        )
        .fetch_optional(&self.pool)
        .await?;

        let token = token.ok_or(AppError::Unauthorized)?;
        let now = Utc::now().timestamp_millis();
        if token.expires_at < now {
            return Err(AppError::Unauthorized);
        }

        let user = sqlx::query!(
            r#"SELECT id as "id!", email, timezone, default_filter, default_sort_order, default_feed_id FROM users WHERE id = ?"#,
            token.user_id
        )
        .fetch_one(&self.pool)
        .await?;

        let user = User {
            id: user.id,
            email: user.email,
            timezone: user.timezone,
            default_filter: user.default_filter,
            default_sort_order: user.default_sort_order,
            default_feed_id: user.default_feed_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let access = self.create_access_token(&user)?;
        Ok(RefreshResponse {
            access_token: access,
            token_type: "Bearer".into(),
            expires_in: 7 * 24 * 60 * 60,
        })
    }

    pub async fn logout(&self, req: RefreshRequest) -> Result<()> {
        let hash = blake3_hash(&req.refresh_token);
        sqlx::query!("DELETE FROM refresh_tokens WHERE token_hash = ?", hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub fn validate_access_token(&self, token: &str) -> Result<Claims> {
        let validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &validation,
        )?;
        if token_data.claims.typ != "access" {
            return Err(AppError::Unauthorized);
        }
        Ok(token_data.claims)
    }

    fn create_access_token(&self, user: &User) -> Result<String> {
        let now = Utc::now();
        let exp = now + Duration::days(7);
        let claims = Claims {
            sub: user.id,
            email: user.email.clone(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            typ: "access".into(),
        };
        let token = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?;
        Ok(token)
    }

    async fn create_refresh_token(&self, user_id: i64) -> Result<String> {
        let token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
        let hash = blake3_hash(&token);
        let expires_at = (Utc::now() + Duration::days(90)).timestamp_millis();

        sqlx::query!(
            "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES (?, ?, ?)",
            user_id,
            hash,
            expires_at
        )
        .execute(&self.pool)
        .await?;

        Ok(token)
    }

    pub async fn revoke_all_user_refresh_tokens(&self, user_id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE user_id = ?", user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

pub fn hash_password(password: String) -> Result<String> {
    bcrypt::hash(password, 12).map_err(AppError::from)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    bcrypt::verify(password, hash).map_err(AppError::from)
}

fn blake3_hash(input: &str) -> String {
    blake3::hash(input.as_bytes()).to_hex().to_string()
}
