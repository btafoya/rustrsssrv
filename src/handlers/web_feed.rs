use askama::Template;
use axum::Form;
use axum::extract::{Multipart, Path, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use validator::Validate;

use crate::errors::AppError;
use crate::handlers::{AuthUser, WebError, WebResult};
use crate::models::{CreateFeedRequest, DiscoverRequest};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "feeds.html")]
pub struct FeedsTemplate {
    feeds: Vec<crate::models::Feed>,
    message: Option<String>,
    error: Option<String>,
    discovered: Vec<DiscoveredRow>,
    discover_url: String,
}

pub struct DiscoveredRow {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddFeedForm {
    #[validate(url(message = "invalid URL"))]
    pub url: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct DiscoverForm {
    #[validate(url(message = "invalid URL"))]
    pub url: String,
}

pub async fn feeds_page(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> WebResult<Response> {
    render_feeds(state, user_id, None, None, None, String::new()).await
}

pub async fn add_feed_submit(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Form(form): Form<AddFeedForm>,
) -> WebResult<Response> {
    if let Err(e) = form.validate() {
        return render_validation_error(state, user_id, e).await;
    }

    let req = CreateFeedRequest { url: form.url };
    match state.feeds.create(user_id, req).await {
        Ok(_) => Ok(Redirect::to("/feeds").into_response()),
        Err(AppError::Conflict(msg)) => render_feeds_error(state, user_id, &msg).await,
        Err(e) => Err(WebError(e)),
    }
}

pub async fn discover_feeds_submit(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Form(form): Form<DiscoverForm>,
) -> WebResult<Response> {
    if let Err(e) = form.validate() {
        return render_validation_error(state, user_id, e).await;
    }

    let req = DiscoverRequest {
        url: form.url.clone(),
    };
    match state.feeds.discover(req).await {
        Ok(resp) => {
            let discovered: Vec<DiscoveredRow> = resp
                .feeds
                .into_iter()
                .map(|f| DiscoveredRow {
                    url: f.url,
                    title: f.title,
                })
                .collect();
            render_feeds(state, user_id, None, None, Some(discovered), form.url).await
        }
        Err(e) => render_feeds_error(state, user_id, &e.to_string()).await,
    }
}

pub async fn add_discovered_feed_submit(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Form(form): Form<AddFeedForm>,
) -> WebResult<Response> {
    if let Err(e) = form.validate() {
        return render_validation_error(state, user_id, e).await;
    }

    let req = CreateFeedRequest { url: form.url };
    match state.feeds.create(user_id, req).await {
        Ok(_) => Ok(Redirect::to("/feeds").into_response()),
        Err(AppError::Conflict(msg)) => render_feeds_error(state, user_id, &msg).await,
        Err(e) => Err(WebError(e)),
    }
}

pub async fn import_opml_submit(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    mut multipart: Multipart,
) -> WebResult<Response> {
    let mut file_data: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        if field.name().map(|n| n == "file").unwrap_or(false) {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            file_data = Some(bytes.to_vec());
            break;
        }
    }
    let data = file_data.unwrap_or_default();
    match state.feeds.import_opml(user_id, &data).await {
        Ok(result) => {
            let message = format!(
                "Imported {} of {} feeds ({} failed).",
                result.imported, result.total, result.failed
            );
            render_feeds(state, user_id, Some(message), None, None, String::new()).await
        }
        Err(e) => render_feeds_error(state, user_id, &e.to_string()).await,
    }
}

pub async fn refresh_feed_submit(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(feed_id): Path<i64>,
) -> WebResult<Response> {
    match state.feeds.refresh(user_id, feed_id).await {
        Ok(_) => Ok(Redirect::to("/feeds").into_response()),
        Err(e) => Err(WebError(e)),
    }
}

pub async fn delete_feed_submit(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(feed_id): Path<i64>,
) -> WebResult<Response> {
    match state.feeds.delete(user_id, feed_id).await {
        Ok(_) => Ok(Redirect::to("/feeds").into_response()),
        Err(e) => Err(WebError(e)),
    }
}

async fn render_feeds(
    state: AppState,
    user_id: i64,
    message: Option<String>,
    error: Option<String>,
    discovered: Option<Vec<DiscoveredRow>>,
    discover_url: String,
) -> WebResult<Response> {
    let page = state
        .feeds
        .list(user_id, None, 1000)
        .await
        .map_err(WebError)?;
    let tpl = FeedsTemplate {
        feeds: page.items,
        message,
        error,
        discovered: discovered.unwrap_or_default(),
        discover_url,
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

async fn render_feeds_error(state: AppState, user_id: i64, message: &str) -> WebResult<Response> {
    render_feeds(
        state,
        user_id,
        None,
        Some(message.into()),
        None,
        String::new(),
    )
    .await
}

async fn render_validation_error(
    state: AppState,
    user_id: i64,
    errors: validator::ValidationErrors,
) -> WebResult<Response> {
    let message: String = errors
        .field_errors()
        .iter()
        .flat_map(|(_, errs)| {
            errs.iter().filter_map(|e| {
                e.message
                    .as_ref()
                    .map(|m| m.to_string())
                    .or_else(|| Some(e.code.to_string()))
            })
        })
        .collect::<Vec<_>>()
        .join("; ");
    render_feeds_error(state, user_id, &message).await
}
