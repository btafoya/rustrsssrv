use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use comrak::{Options, markdown_to_html};

use crate::errors::AppError;
use crate::handlers::{AuthUser, WebResult, strip_html_tags};
use crate::models::{ListArticlesQuery, UserUpdate};
use crate::state::AppState;

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ArticleListWebQuery {
    // String, not i64: the form's "All Feeds" option submits an empty string,
    // which Option<i64> can't parse.
    pub feed_id: Option<String>,
    pub filter: Option<String>,
    // Legacy bool parameters kept for backward compatibility with bookmarks/old links.
    pub is_read: Option<String>,
    pub is_starred: Option<String>,
    pub sort: Option<String>,
    pub cursor: Option<i64>,
    pub dir: Option<String>,
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
    feeds: Vec<FeedOption>,
    feed_id: Option<i64>,
    filter: String,
    sort: String,
    next_cursor: Option<i64>,
    prev_cursor: Option<i64>,
}

struct FeedOption {
    id: i64,
    label: String,
    selected: bool,
}

fn truncate_label(s: &str) -> String {
    if s.chars().count() <= 30 {
        return s.to_string();
    }
    s.chars().take(29).collect::<String>() + "…"
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

    let feed_id = match params.feed_id.as_deref() {
        Some("") => None,
        Some(s) => s.parse::<i64>().ok(),
        None => user.default_feed_id,
    };

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
        .clone()
        .unwrap_or_else(|| user.default_sort_order.clone());
    let limit = params.limit.unwrap_or(50);

    // Explicit filter/sort/feed submitted via the page's form becomes the
    // user's new default. Legacy is_read/is_starred bookmark links are
    // excluded on purpose.
    if params.filter.is_some() || params.sort.is_some() || params.feed_id.is_some() {
        let update = UserUpdate {
            email: None,
            timezone: None,
            default_filter: params.filter.clone(),
            default_sort_order: params.sort.clone(),
            current_password: None,
            new_password: None,
        };
        if let Err(e) = state.users.update(user_id, update).await {
            tracing::warn!(
                "failed to save default filter/sort for user {}: {}",
                user_id,
                e
            );
        }
        if params.feed_id.is_some()
            && let Err(e) = state.users.set_default_feed_id(user_id, feed_id).await
        {
            tracing::warn!("failed to save default feed for user {}: {}", user_id, e);
        }
    }

    let list_query = ListArticlesQuery {
        feed_id,
        is_read,
        is_starred,
        sort: Some(sort.clone()),
        cursor: params.cursor,
        direction: params.dir.clone(),
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

    let feed_page = state.feeds.list(user_id, None, 100).await?;
    let mut feeds: Vec<FeedOption> = feed_page
        .items
        .into_iter()
        .map(|f| FeedOption {
            id: f.id,
            label: truncate_label(&f.title.unwrap_or(f.url)),
            selected: Some(f.id) == feed_id,
        })
        .collect();
    feeds.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));

    let tpl = ArticleListTemplate {
        items: rows,
        feeds,
        feed_id,
        filter,
        sort,
        next_cursor: page.next_cursor,
        prev_cursor: page.prev_cursor,
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
