use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::errors::Result;
use crate::models::{LoginRequest, RefreshRequest};
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Tokens issued", body = crate::models::LoginResponse),
        (status = 401, description = "Invalid credentials")
    )
)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<crate::models::LoginResponse>> {
    let resp = state.auth.login(req).await?;
    Ok(Json(resp))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "New access token", body = crate::models::RefreshResponse),
        (status = 401, description = "Invalid or expired refresh token")
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<crate::models::RefreshResponse>> {
    let resp = state.auth.refresh(req).await?;
    Ok(Json(resp))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    request_body = RefreshRequest,
    responses((status = 204, description = "Token revoked"))
)]
pub async fn logout(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<StatusCode> {
    state.auth.logout(req).await?;
    Ok(StatusCode::NO_CONTENT)
}
