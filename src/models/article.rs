use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Article {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub feed_id: i64,
    pub feed_title: Option<String>,
    pub is_read: bool,
    pub is_starred: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ArticlePage {
    pub items: Vec<Article>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ArticleInput {
    pub url: String,
    pub title: String,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub published_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListArticlesQuery {
    pub feed_id: Option<i64>,
    pub is_read: Option<bool>,
    pub is_starred: Option<bool>,
    pub sort: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

impl ListArticlesQuery {
    pub fn is_newest_first(&self) -> bool {
        self.sort
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("newest_first"))
            .unwrap_or(false)
    }
}
