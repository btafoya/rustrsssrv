use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::errors::Result;
use crate::handlers::AuthUser;
use crate::models::{
    Article, ArticlePage, BulkArticlesRequest, BulkArticlesResult, ListArticlesQuery,
};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/articles",
    params(
        ("feed_id" = Option<i64>, Query, description = "Filter by subscription feed id"),
        ("is_read" = Option<bool>, Query, description = "Filter by read state"),
        ("is_starred" = Option<bool>, Query, description = "Filter by starred state"),
        ("sort" = Option<String>, Query, description = "oldest_first or newest_first"),
        ("cursor" = Option<i64>, Query, description = "Cursor for pagination"),
        ("direction" = Option<String>, Query, description = "next (default) or prev, direction to page from cursor"),
        ("limit" = Option<i64>, Query, description = "Page size")
    ),
    responses((status = 200, description = "Paginated article list", body = ArticlePage))
)]
pub async fn list_articles(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(q): Query<ListArticlesQuery>,
) -> Result<Json<ArticlePage>> {
    let page = state.articles.list(user_id, q).await?;
    Ok(Json(page))
}

#[utoipa::path(
    get,
    path = "/api/v1/articles/{articleId}",
    params(("articleId" = i64, Path, description = "Article ID")),
    responses((status = 200, description = "Article details", body = Article))
)]
pub async fn get_article(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(article_id): Path<i64>,
) -> Result<(StatusCode, Json<Article>)> {
    let article = state.articles.get(user_id, article_id).await?;
    Ok((StatusCode::OK, Json(article)))
}

#[utoipa::path(
    post,
    path = "/api/v1/articles/{articleId}/read",
    params(
        ("articleId" = i64, Path, description = "Article ID"),
    ),
    responses(
        (status = 204, description = "Marked read"),
        (status = 404, description = "Article not found or not in subscription"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error"),
    ),
    tag = "Articles",
    security(
        ("bearer" = [])
    )
)]
pub async fn mark_read(
    State(state): State<AppState>,
    Path(article_id): Path<i64>,
    AuthUser(user_id): AuthUser,
) -> Result<StatusCode> {
    state.articles.mark_read(user_id, article_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/articles/{articleId}/unread",
    params(
        ("articleId" = i64, Path, description = "Article ID"),
    ),
    responses(
        (status = 204, description = "Marked unread"),
        (status = 404, description = "Article not found or not in subscription"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error"),
    ),
    tag = "Articles",
    security(
        ("bearer" = [])
    )
)]
pub async fn mark_unread(
    State(state): State<AppState>,
    Path(article_id): Path<i64>,
    AuthUser(user_id): AuthUser,
) -> Result<StatusCode> {
    state.articles.mark_unread(user_id, article_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/articles/{articleId}/star",
    params(
        ("articleId" = i64, Path, description = "Article ID"),
    ),
    responses(
        (status = 204, description = "Marked starred"),
        (status = 404, description = "Article not found or not in subscription"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error"),
    ),
    tag = "Articles",
    security(
        ("bearer" = [])
    )
)]
pub async fn mark_starred(
    State(state): State<AppState>,
    Path(article_id): Path<i64>,
    AuthUser(user_id): AuthUser,
) -> Result<StatusCode> {
    state.articles.mark_starred(user_id, article_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/articles/{articleId}/unstar",
    params(
        ("articleId" = i64, Path, description = "Article ID"),
    ),
    responses(
        (status = 204, description = "Marked unstarred"),
        (status = 404, description = "Article not found or not in subscription"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error"),
    ),
    tag = "Articles",
    security(
        ("bearer" = [])
    )
)]
pub async fn mark_unstarred(
    State(state): State<AppState>,
    Path(article_id): Path<i64>,
    AuthUser(user_id): AuthUser,
) -> Result<StatusCode> {
    state.articles.mark_unstarred(user_id, article_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/articles/bulk",
    request_body = BulkArticlesRequest,
    responses(
        (status = 200, description = "Bulk action applied", body = BulkArticlesResult),
        (status = 400, description = "Bad request — provide exactly one of article_ids or filter"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "An article was not found or not in subscription"),
        (status = 500, description = "Internal error"),
    ),
    tag = "Articles",
    security(
        ("bearer" = [])
    )
)]
pub async fn bulk_update_articles(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<BulkArticlesRequest>,
) -> Result<Json<BulkArticlesResult>> {
    let affected = state
        .articles
        .bulk_apply(user_id, body.action, body.article_ids, body.filter)
        .await?;
    Ok(Json(BulkArticlesResult { affected }))
}

#[utoipa::path(
    get,
    path = "/api/v1/search",
    params(
        ("q" = String, Query, description = "Search query"),
        ("limit" = Option<i64>, Query, description = "Max results (1-100, default 20)"),
    ),
    responses(
        (status = 200, description = "Search results", body = ArticlePage),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error"),
    ),
    tag = "Articles",
    security(
        ("bearer" = [])
    )
)]
pub async fn search_articles(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<(StatusCode, Json<ArticlePage>)> {
    let page = state
        .articles
        .search(user_id, &query.q, query.limit.unwrap_or(20))
        .await?;
    Ok((StatusCode::OK, Json(page)))
}
