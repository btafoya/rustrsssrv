use askama::Template;
use axum::Form;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use validator::Validate;

use crate::errors::{AppError, Result};
use crate::models::CreateUserRequest;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "setup.html")]
struct SetupTemplate {
    error: Option<String>,
    email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SetupForm {
    #[validate(email(message = "invalid email"))]
    pub email: String,
    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    pub password: String,
    pub password_confirmation: String,
}

pub async fn setup_page(State(state): State<AppState>) -> Result<Response> {
    let count = state.users.count().await?;
    if count > 0 {
        return Ok(Redirect::to("/").into_response());
    }
    let tpl = SetupTemplate {
        error: None,
        email: String::new(),
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn setup_submit(
    State(state): State<AppState>,
    Form(form): Form<SetupForm>,
) -> Result<Response> {
    let count = state.users.count().await?;
    if count > 0 {
        return Ok(Redirect::to("/").into_response());
    }

    form.validate()?;
    if form.password != form.password_confirmation {
        return render_setup_error(&form.email, "passwords do not match");
    }

    let req = CreateUserRequest {
        email: form.email.clone(),
        password: form.password,
        password_confirmation: form.password_confirmation,
    };

    match state.users.create(req).await {
        Ok(_) => Ok(Redirect::to("/login").into_response()),
        Err(AppError::Database(sqlx::Error::Database(db))) if db.is_unique_violation() => {
            render_setup_error(&form.email, "email already exists")
        }
        Err(e) => Err(e),
    }
}

fn render_setup_error(email: &str, message: &str) -> Result<Response> {
    let tpl = SetupTemplate {
        error: Some(message.into()),
        email: email.into(),
    };
    Ok((
        StatusCode::BAD_REQUEST,
        Html(
            tpl.render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        ),
    )
        .into_response())
}
