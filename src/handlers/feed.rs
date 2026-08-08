use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use serde::Deserialize;

use crate::errors::Result;
use crate::handlers::AuthUser;
use crate::models::{CreateFeedRequest, DiscoverRequest, Feed, FeedPage, FeedUpdate, ImportResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListFeedsQuery {
    cursor: Option<i64>,
    limit: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/feeds",
    params(("cursor" = Option<i64>, Query, description = "Cursor for pagination"), ("limit" = Option<i64>, Query, description = "Page size")),
    responses((status = 200, description = "Paginated feed list", body = FeedPage))
)]
pub async fn list_feeds(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(q): Query<ListFeedsQuery>,
) -> Result<Json<FeedPage>> {
    let page = state
        .feeds
        .list(user_id, q.cursor, q.limit.unwrap_or(20))
        .await?;
    Ok(Json(page))
}

#[utoipa::path(
    post,
    path = "/api/v1/feeds",
    request_body = CreateFeedRequest,
    responses(
        (status = 201, description = "Subscription created", body = Feed),
        (status = 409, description = "Already subscribed")
    )
)]
pub async fn create_feed(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CreateFeedRequest>,
) -> Result<(StatusCode, Json<Feed>)> {
    let feed = state.feeds.create(user_id, req).await?;
    Ok((StatusCode::CREATED, Json(feed)))
}

#[utoipa::path(
    post,
    path = "/api/v1/feeds/discover",
    request_body = DiscoverRequest,
    responses((status = 200, description = "Candidate feeds", body = crate::models::DiscoverResponse))
)]
pub async fn discover_feeds(
    State(state): State<AppState>,
    AuthUser(_user_id): AuthUser,
    Json(req): Json<DiscoverRequest>,
) -> Result<Json<crate::models::DiscoverResponse>> {
    let resp = state.feeds.discover(req).await?;
    Ok(Json(resp))
}

#[utoipa::path(
    get,
    path = "/api/v1/feeds/{feedId}",
    params(("feedId" = i64, Path, description = "Feed ID")),
    responses((status = 200, description = "Feed details", body = Feed))
)]
pub async fn get_feed(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(feed_id): Path<i64>,
) -> Result<Json<Feed>> {
    let feed = state.feeds.get(user_id, feed_id).await?;
    Ok(Json(feed))
}

#[utoipa::path(
    patch,
    path = "/api/v1/feeds/{feedId}",
    params(("feedId" = i64, Path, description = "Feed ID")),
    request_body = FeedUpdate,
    responses((status = 200, description = "Updated feed", body = Feed))
)]
pub async fn patch_feed(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(feed_id): Path<i64>,
    Json(req): Json<FeedUpdate>,
) -> Result<Json<Feed>> {
    let feed = state.feeds.update(user_id, feed_id, req).await?;
    Ok(Json(feed))
}

#[utoipa::path(
    delete,
    path = "/api/v1/feeds/{feedId}",
    params(("feedId" = i64, Path, description = "Feed ID")),
    responses((status = 204, description = "Unsubscribed"))
)]
pub async fn delete_feed(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(feed_id): Path<i64>,
) -> Result<StatusCode> {
    state.feeds.delete(user_id, feed_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/feeds/{feedId}/refresh",
    params(("feedId" = i64, Path, description = "Feed ID")),
    responses((status = 202, description = "Refresh accepted"))
)]
pub async fn refresh_feed(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(feed_id): Path<i64>,
) -> Result<StatusCode> {
    state.feeds.refresh(user_id, feed_id).await?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    post,
    path = "/api/v1/feeds/import/opml",
    request_body(content_type = "multipart/form-data"),
    responses((status = 200, description = "Import result", body = ImportResult))
)]
pub async fn import_opml(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    body: axum::body::Bytes,
) -> Result<Json<ImportResult>> {
    let result = state.feeds.import_opml(user_id, &body).await?;
    Ok(Json(result))
}

#[utoipa::path(
    get,
    path = "/api/v1/feeds/export/opml",
    responses((status = 200, description = "OPML document", content_type = "application/xml"))
)]
pub async fn export_opml(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<(StatusCode, [(header::HeaderName, &'static str); 1], String)> {
    let xml = state.feeds.export_opml(user_id).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    ))
}
