pub mod article;
pub mod assets;
pub mod auth;
pub mod feed;
pub mod health;
pub mod media;
pub mod setup;
pub mod user;
pub mod web;
pub mod web_article;

use axum::Router;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::{StatusCode, request::Parts};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::errors::AppError;
use crate::state::AppState;

pub fn build_app(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .route(
            "/users/me",
            get(user::get_me)
                .patch(user::patch_me)
                .delete(user::delete_me),
        )
        .route("/feeds", get(feed::list_feeds).post(feed::create_feed))
        .route("/feeds/discover", post(feed::discover_feeds))
        .route("/feeds/import/opml", post(feed::import_opml))
        .route("/feeds/export/opml", get(feed::export_opml))
        .route(
            "/feeds/:feedId",
            get(feed::get_feed)
                .patch(feed::patch_feed)
                .delete(feed::delete_feed),
        )
        .route("/feeds/:feedId/refresh", post(feed::refresh_feed))
        .route("/articles", get(article::list_articles))
        .route("/articles/:articleId", get(article::get_article))
        .route("/articles/:articleId/read", post(article::mark_read))
        .route("/articles/:articleId/unread", post(article::mark_unread))
        .route("/articles/:articleId/star", post(article::mark_starred))
        .route("/articles/:articleId/unstar", post(article::mark_unstarred))
        .route("/search", get(article::search_articles))
        .route("/media/:mediaHash", get(media::get_media))
        .with_state(state.clone());

    let api_v1 = Router::new().nest("/api/v1", api);

    let web = Router::new()
        .route("/", get(web::dashboard))
        .route("/login", get(web::login_page))
        .route("/setup", get(setup::setup_page).post(setup::setup_submit))
        .route("/articles", get(web_article::article_list_page))
        .route("/articles/:articleId", get(web_article::article_page))
        .route("/feeds", get(web::feeds_page))
        .route("/search", get(web::search_page))
        .route("/settings", get(web::settings_page))
        .with_state(state.clone());

    let static_routes = Router::new()
        .route("/static/*path", get(assets::static_handler))
        .with_state(state.clone());

    let open_api = ApiDoc::openapi();

    Router::new()
        .route("/health", get(health::health_check))
        .merge(api_v1)
        .merge(web)
        .merge(static_routes)
        .merge(SwaggerUi::new("/api-docs").url("/api-docs/openapi.json", open_api))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new()),
        )
        .with_state(state)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        auth::login,
        auth::refresh,
        auth::logout,
        user::get_me,
        user::patch_me,
        user::delete_me,
        feed::list_feeds,
        feed::create_feed,
        feed::discover_feeds,
        feed::get_feed,
        feed::patch_feed,
        feed::delete_feed,
        feed::refresh_feed,
        feed::import_opml,
        feed::export_opml,
        article::list_articles,
        article::get_article,
        article::mark_read,
        article::mark_unread,
        article::mark_starred,
        article::mark_unstarred,
        article::search_articles,
        media::get_media,
        health::health_check,
    ),
    components(schemas(
        crate::models::LoginRequest,
        crate::models::LoginResponse,
        crate::models::RefreshRequest,
        crate::models::RefreshResponse,
        crate::models::User,
        crate::models::CreateUserRequest,
        crate::models::UserUpdate,
        crate::models::Feed,
        crate::models::FeedPage,
        crate::models::CreateFeedRequest,
        crate::models::FeedUpdate,
        crate::models::DiscoverRequest,
        crate::models::DiscoverResponse,
        crate::models::DiscoveredFeed,
        crate::models::ImportResult,
        crate::models::ImportedFeed,
        crate::models::Article,
        crate::models::ArticlePage,
    )),
    security(("BearerAuth" = []))
)]
pub struct ApiDoc;

pub struct AuthUser(pub i64);

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: AsRef<crate::state::AppStateInner> + Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|header| {
                header
                    .strip_prefix("Bearer ")
                    .or_else(|| header.strip_prefix("bearer "))
            })
            .map(|t| t.to_string())
            .or_else(|| {
                parts
                    .headers
                    .get(axum::http::header::COOKIE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|cookies| {
                        cookies.split(';').find_map(|cookie| {
                            let mut kv = cookie.trim().splitn(2, '=');
                            match (kv.next(), kv.next()) {
                                (Some("access_token"), Some(value)) => Some(value.to_string()),
                                _ => None,
                            }
                        })
                    })
            });

        let token = token.ok_or(AppError::Unauthorized)?;
        let claims = state.as_ref().auth.validate_access_token(&token)?;
        Ok(AuthUser(claims.sub))
    }
}

pub async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Html("<h1>Not Found</h1>"))
}
