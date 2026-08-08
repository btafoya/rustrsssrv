use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub timezone: String,
    #[serde(rename = "default_filter")]
    pub default_filter: String,
    #[serde(rename = "default_sort_order")]
    pub default_sort_order: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct CreateUserRequest {
    #[validate(email(message = "invalid email"))]
    pub email: String,
    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    pub password: String,
    #[validate(must_match(other = "password", message = "passwords do not match"))]
    pub password_confirmation: String,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct LoginRequest {
    #[validate(email(message = "invalid email"))]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UserUpdate {
    #[validate(email(message = "invalid email"))]
    pub email: Option<String>,
    pub timezone: Option<String>,
    pub default_filter: Option<String>,
    pub default_sort_order: Option<String>,
    pub current_password: Option<String>,
    #[validate(custom(function = "validate_password_complexity"))]
    pub new_password: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct PasswordChange {
    pub current_password: String,
    #[validate(custom(function = "validate_password_complexity"))]
    pub new_password: String,
}

fn validate_password_complexity(
    password: &str,
) -> std::result::Result<(), validator::ValidationError> {
    if password.len() < 8 {
        return Err(validator::ValidationError::new("password too short"));
    }
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());
    if has_upper && has_lower && has_digit && has_special {
        Ok(())
    } else {
        Err(validator::ValidationError::new(
            "password must contain uppercase, lowercase, digit, and special character",
        ))
    }
}
