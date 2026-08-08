pub mod article;
pub mod assets;
pub mod auth;
pub mod feed;
pub mod health;
pub mod setup;
pub mod user;
pub mod web;

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
        .with_state(state.clone());

    let api_v1 = Router::new().nest("/api/v1", api);

    let web = Router::new()
        .route("/", get(web::dashboard))
        .route("/login", get(web::login_page))
        .route("/setup", get(setup::setup_page).post(setup::setup_submit))
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
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or(AppError::Unauthorized)?;

        let claims = state.as_ref().auth.validate_access_token(token)?;
        Ok(AuthUser(claims.sub))
    }
}

pub async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Html("<h1>Not Found</h1>"))
}
