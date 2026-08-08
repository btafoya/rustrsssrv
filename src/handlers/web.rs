use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};

use crate::errors::{AppError, Result};
use crate::handlers::AuthUser;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    next: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    user_id: i64,
}

pub async fn login_page() -> impl IntoResponse {
    Html(
        LoginTemplate { next: "/".into() }
            .render()
            .unwrap_or_default(),
    )
}

pub async fn dashboard(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Response> {
    let _user = state.users.get_by_id(user_id).await?;
    let tpl = DashboardTemplate { user_id };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
