use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

use crate::errors::Result;
use crate::models::{LoginRequest, RefreshRequest};
use crate::state::AppState;

const ACCESS_TOKEN_MAX_AGE: i64 = 7 * 24 * 60 * 60;

fn access_token_cookie(token: &str, max_age: i64) -> String {
    format!("access_token={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}")
}

fn clear_access_token_cookie() -> String {
    "access_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0".to_string()
}

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
) -> Result<impl IntoResponse> {
    let resp = state.auth.login(req).await?;
    let cookie = access_token_cookie(&resp.access_token, ACCESS_TOKEN_MAX_AGE);
    Ok(([(header::SET_COOKIE, cookie)], Json(resp)))
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
) -> Result<impl IntoResponse> {
    state.auth.logout(req).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_access_token_cookie())],
    ))
}
