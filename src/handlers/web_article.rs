use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use comrak::{Options, markdown_to_html};

use crate::errors::{AppError, Result};
use crate::handlers::AuthUser;
use crate::models::ListArticlesQuery;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "article.html")]
struct ArticleTemplate {
    title: String,
    feed_title: Option<String>,
    published_at: Option<String>,
    html_content: String,
}

#[derive(Template)]
#[template(path = "article_list.html")]
struct ArticleListTemplate {
    items: Vec<ArticleRow>,
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
) -> Result<Response> {
    let article = state.articles.get(user_id, article_id).await?;
    let html_content = markdown_to_html(&article.markdown_content, &Options::default());
    let published_at = article.published_at.map(|d| d.to_rfc2822());

    let tpl = ArticleTemplate {
        title: article.title,
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
) -> Result<Response> {
    let query = ListArticlesQuery {
        feed_id: None,
        is_read: Some(false),
        is_starred: None,
        sort: None,
        cursor: None,
        limit: Some(50),
    };
    let page = state.articles.list(user_id, query).await?;
    let rows: Vec<ArticleRow> = page
        .items
        .into_iter()
        .map(|a| ArticleRow {
            id: a.id,
            title: a.title,
            summary: a.summary,
            feed_title: a.feed_title,
            published_at: a.published_at.map(|d| d.to_rfc2822()),
            is_read: a.is_read,
        })
        .collect();

    let tpl = ArticleListTemplate { items: rows };
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
