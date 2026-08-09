use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use comrak::{Options, markdown_to_html};
use regex::Regex;
use std::sync::LazyLock;

use crate::errors::AppError;
use crate::handlers::{AuthUser, WebResult};
use crate::models::ListArticlesQuery;
use crate::state::AppState;

static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());

fn strip_html_tags(html: &str) -> String {
    HTML_TAG_RE
        .replace_all(html, "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Template)]
#[template(path = "article.html")]
struct ArticleTemplate {
    title: String,
    url: String,
    feed_title: Option<String>,
    published_at: Option<String>,
    html_content: String,
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
        title: article.title,
        url: article.url,
        feed_title: article.feed_title,
        published_at,
        html_content,
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
    Query(mut params): Query<ListArticlesQuery>,
) -> WebResult<Response> {
    let user = state.users.get_by_id(user_id).await?;

    if params.is_read.is_none() {
        params.is_read = match user.default_filter.as_str() {
            "all" => None,
            _ => Some(false),
        };
    }
    if params.sort.is_none() {
        params.sort = Some(user.default_sort_order.clone());
    }
    if params.limit.is_none() {
        params.limit = Some(50);
    }

    let page = state.articles.list(user_id, params.clone()).await?;

    let filter = match params.is_read {
        None => "all".to_string(),
        Some(true) => "read".to_string(),
        Some(false) => "unread".to_string(),
    };
    let sort = params.sort.unwrap_or_else(|| "oldest_first".to_string());

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
