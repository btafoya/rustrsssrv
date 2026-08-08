use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::errors::Result;
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
}

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Health check", body = HealthResponse))
)]
pub async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>> {
    let row = sqlx::query!(r#"SELECT 1 as "one!""#)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(HealthResponse {
        status: "ok".into(),
        database: if row.one == 1 {
            "ok".into()
        } else {
            "error".into()
        },
    }))
}
