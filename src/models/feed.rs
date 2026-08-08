use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Feed {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_url: Option<String>,
    pub fetch_interval_minutes: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct CreateFeedRequest {
    #[validate(url(message = "invalid URL"))]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct FeedUpdate {
    pub fetch_interval_minutes: Option<i64>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct DiscoverRequest {
    #[validate(url(message = "invalid URL"))]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiscoveredFeed {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiscoverResponse {
    pub feeds: Vec<DiscoveredFeed>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImportResult {
    pub total: usize,
    pub imported: usize,
    pub failed: usize,
    pub feeds: Vec<ImportedFeed>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImportedFeed {
    pub url: String,
    pub title: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FeedPage {
    pub items: Vec<Feed>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}
