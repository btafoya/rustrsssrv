use askama::Template;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::errors::AppError;
use crate::handlers::{AuthUser, WebError, WebResult, strip_html_tags};
use crate::models::ListArticlesQuery;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    next: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    email: String,
    timezone: String,
    unread_count: i64,
    feed_count: i64,
    items: Vec<ArticleRow>,
}

struct ArticleRow {
    id: i64,
    title: String,
    summary: Option<String>,
    feed_title: Option<String>,
    published_at: Option<String>,
    is_read: bool,
    is_starred: bool,
}

#[derive(Template)]
#[template(path = "search.html")]
struct SearchTemplate {
    query: String,
    items: Vec<ArticleRow>,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    email: String,
    timezone: String,
    default_filter: String,
    default_sort_order: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SearchWebQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
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
    auth_user: Option<AuthUser>,
) -> WebResult<Response> {
    let user_count = state.users.count().await.map_err(WebError)?;
    if user_count == 0 {
        return Ok(Redirect::to("/setup").into_response());
    }

    let AuthUser(user_id) = auth_user.ok_or(AppError::Unauthorized)?;
    let user = state.users.get_by_id(user_id).await?;
    let feed_page = state.feeds.list(user_id, None, 1000).await?;
    let feed_count = feed_page.items.len() as i64;

    let unread_query = ListArticlesQuery {
        feed_id: None,
        is_read: Some(false),
        is_starred: None,
        sort: None,
        cursor: None,
        limit: Some(10),
    };
    let unread_page = state.articles.list(user_id, unread_query).await?;
    let unread_count = if unread_page.has_more {
        unread_page.items.len() as i64 + 1
    } else {
        unread_page.items.len() as i64
    };

    let items: Vec<ArticleRow> = unread_page
        .items
        .into_iter()
        .map(|a| ArticleRow {
            id: a.id,
            title: a.title,
            summary: a.summary.as_deref().map(strip_html_tags),
            feed_title: a.feed_title,
            published_at: a.published_at.map(|d| d.to_rfc2822()),
            is_read: a.is_read,
            is_starred: a.is_starred,
        })
        .collect();

    let tpl = DashboardTemplate {
        email: user.email,
        timezone: user.timezone,
        unread_count,
        feed_count,
        items,
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn search_page(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(query): Query<SearchWebQuery>,
) -> WebResult<Response> {
    let q = query.q.as_deref().unwrap_or("").trim();
    let items = if q.is_empty() {
        Vec::new()
    } else {
        let page = state
            .articles
            .search(user_id, q, query.limit.unwrap_or(50))
            .await?;
        page.items
            .into_iter()
            .map(|a| ArticleRow {
                id: a.id,
                title: a.title,
                summary: a.summary.as_deref().map(strip_html_tags),
                feed_title: a.feed_title,
                published_at: a.published_at.map(|d| d.to_rfc2822()),
                is_read: a.is_read,
                is_starred: a.is_starred,
            })
            .collect()
    };

    let tpl = SearchTemplate {
        query: query.q.unwrap_or_default(),
        items,
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn settings_page(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> WebResult<Response> {
    let user = state.users.get_by_id(user_id).await?;
    let tpl = SettingsTemplate {
        email: user.email,
        timezone: user.timezone,
        default_filter: user.default_filter,
        default_sort_order: user.default_sort_order,
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
