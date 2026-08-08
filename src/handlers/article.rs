use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use crate::errors::Result;
use crate::handlers::AuthUser;
use crate::models::{Article, ArticlePage, ListArticlesQuery};
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/v1/articles",
    params(
        ("feed_id" = Option<i64>, Query, description = "Filter by subscription feed id"),
        ("is_read" = Option<bool>, Query, description = "Filter by read state"),
        ("is_starred" = Option<bool>, Query, description = "Filter by starred state"),
        ("sort" = Option<String>, Query, description = "oldest_first or newest_first"),
        ("cursor" = Option<i64>, Query, description = "Cursor for pagination"),
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
