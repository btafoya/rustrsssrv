use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::errors::Result;
use crate::handlers::AuthUser;
use crate::models::{User, UserUpdate};
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/v1/users/me",
    responses((status = 200, description = "Current user profile", body = User))
)]
pub async fn get_me(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<User>> {
    let user = state.users.get_by_id(user_id).await?;
    Ok(Json(user))
}

#[utoipa::path(
    patch,
    path = "/api/v1/users/me",
    request_body = UserUpdate,
    responses((status = 200, description = "Updated user", body = User))
)]
pub async fn patch_me(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<UserUpdate>,
) -> Result<Json<User>> {
    let user = state.users.update(user_id, req).await?;
    Ok(Json(user))
}

#[utoipa::path(
    delete,
    path = "/api/v1/users/me",
    responses((status = 204, description = "User deleted"))
)]
pub async fn delete_me(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<StatusCode> {
    state.users.delete(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
