use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use comrak::{Options, markdown_to_html};

use crate::errors::AppError;
use crate::handlers::{AuthUser, WebResult, strip_html_tags};
use crate::models::ListArticlesQuery;
use crate::state::AppState;

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ArticleListWebQuery {
    pub feed_id: Option<i64>,
    pub filter: Option<String>,
    // Legacy bool parameters kept for backward compatibility with bookmarks/old links.
    pub is_read: Option<String>,
    pub is_starred: Option<String>,
    pub sort: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

impl ArticleListWebQuery {
    fn filter_from_legacy(&self) -> Option<String> {
        if let Some(v) = &self.is_read {
            return Some(match v.as_str() {
                "" | "all" => "all".into(),
                "true" => "read".into(),
                "false" => "unread".into(),
                _ => "all".into(),
            });
        }
        if self.is_starred.as_deref() == Some("true") {
            return Some("starred".into());
        }
        None
    }
}

#[derive(Template)]
#[template(path = "article.html")]
struct ArticleTemplate {
    id: i64,
    title: String,
    url: String,
    feed_title: Option<String>,
    published_at: Option<String>,
    html_content: String,
    is_starred: bool,
}

#[derive(Template)]
#[template(path = "article_list.html")]
struct ArticleListTemplate {
    items: Vec<ArticleRow>,
    filter: String,
    sort: String,
    next_cursor: Option<i64>,
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

pub async fn article_page(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(article_id): Path<i64>,
) -> WebResult<Response> {
    let article = state.articles.get(user_id, article_id).await?;
    if let Err(e) = state.articles.mark_read(user_id, article_id).await {
        tracing::warn!(
            "failed to mark article {} read for user {}: {}",
            article_id,
            user_id,
            e
        );
    }
    let html_content = markdown_to_html(&article.markdown_content, &Options::default());
    let published_at = article.published_at.map(|d| d.to_rfc2822());

    let tpl = ArticleTemplate {
        id: article.id,
        title: article.title,
        url: article.url,
        feed_title: article.feed_title,
        published_at,
        html_content,
        is_starred: article.is_starred,
    };

    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn article_list_page(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(params): Query<ArticleListWebQuery>,
) -> WebResult<Response> {
    let user = state.users.get_by_id(user_id).await?;

    let filter = params
        .filter
        .clone()
        .or_else(|| params.filter_from_legacy())
        .unwrap_or_else(|| user.default_filter.clone());
    let (is_read, is_starred) = match filter.as_str() {
        "all" => (None, None),
        "unread" => (Some(false), None),
        "read" => (Some(true), None),
        "starred" => (None, Some(true)),
        _ => (Some(false), None),
    };

    let sort = params
        .sort
        .unwrap_or_else(|| user.default_sort_order.clone());
    let limit = params.limit.unwrap_or(50);

    let list_query = ListArticlesQuery {
        feed_id: params.feed_id,
        is_read,
        is_starred,
        sort: Some(sort.clone()),
        cursor: params.cursor,
        limit: Some(limit),
    };

    let page = state.articles.list(user_id, list_query).await?;

    let rows: Vec<ArticleRow> = page
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

    let tpl = ArticleListTemplate {
        items: rows,
        filter,
        sort,
        next_cursor: page.next_cursor,
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn not_found_page() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Html("<h1 class=\"text-2xl font-bold\">Not Found</h1>"),
    )
}
