use chrono::Utc;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use sqlx::SqlitePool;
use validator::Validate;

use crate::errors::{AppError, Result};
use crate::models::{
    CreateFeedRequest, DiscoverRequest, DiscoverResponse, DiscoveredFeed, Feed, FeedPage,
    FeedUpdate, ImportResult, ImportedFeed,
};

#[derive(Clone)]
pub struct FeedService {
    pool: SqlitePool,
    client: Client,
}

impl FeedService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    pub async fn create(&self, user_id: i64, req: CreateFeedRequest) -> Result<Feed> {
        req.validate()?;
        let now = Utc::now();
        let millis = now.timestamp_millis();

        // Ensure feed exists.
        let feed_id = match self.find_feed_by_url(&req.url).await? {
            Some(id) => id,
            None => {
                sqlx::query!(
                    "INSERT INTO feeds (url, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
                    req.url,
                    15,
                    millis,
                    millis,
                    millis
                )
                .execute(&self.pool)
                .await?
                .last_insert_rowid()
            }
        };

        // Subscribe user if not already.
        sqlx::query!(
            "INSERT OR IGNORE INTO subscriptions (user_id, feed_id, created_at) VALUES (?, ?, ?)",
            user_id,
            feed_id,
            millis
        )
        .execute(&self.pool)
        .await?;

        self.get(user_id, feed_id).await
    }

    pub async fn list(&self, user_id: i64, cursor: Option<i64>, limit: i64) -> Result<FeedPage> {
        let limit = limit.clamp(1, 100);
        let cursor = cursor.unwrap_or(0);
        let page_size = limit + 1;
        let rows = sqlx::query!(
            r#"
            SELECT f.id as "id!", f.url, f.title, f.description, f.site_url, f.fetch_interval_minutes, f.consecutive_failures, f.backoff_until
            FROM feeds f
            JOIN subscriptions s ON s.feed_id = f.id
            WHERE s.user_id = ? AND f.id > ?
            ORDER BY f.id ASC
            LIMIT ?
            "#,
            user_id,
            cursor,
            page_size
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items: Vec<Feed> = rows
            .into_iter()
            .map(|r| Feed {
                id: r.id,
                url: r.url,
                title: r.title,
                description: r.description,
                site_url: r.site_url,
                fetch_interval_minutes: r.fetch_interval_minutes,
                status: compute_status(r.consecutive_failures, r.backoff_until),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .collect();

        let has_more = items.len() > limit as usize;
        if has_more {
            items.pop();
        }
        let next_cursor = if has_more {
            items.last().map(|f| f.id)
        } else {
            None
        };

        Ok(FeedPage {
            items,
            next_cursor,
            has_more,
        })
    }

    pub async fn get(&self, user_id: i64, feed_id: i64) -> Result<Feed> {
        let row = sqlx::query!(
            r#"
            SELECT f.id as "id!", f.url, f.title, f.description, f.site_url, f.fetch_interval_minutes, f.consecutive_failures, f.backoff_until
            FROM feeds f
            JOIN subscriptions s ON s.feed_id = f.id
            WHERE s.user_id = ? AND f.id = ?
            "#,
            user_id,
            feed_id
        )
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(AppError::NotFound)?;
        Ok(Feed {
            id: row.id,
            url: row.url,
            title: row.title,
            description: row.description,
            site_url: row.site_url,
            fetch_interval_minutes: row.fetch_interval_minutes,
            status: compute_status(row.consecutive_failures, row.backoff_until),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    pub async fn update(&self, user_id: i64, feed_id: i64, req: FeedUpdate) -> Result<Feed> {
        // Verify subscription.
        self.get(user_id, feed_id).await?;

        let mut interval: i64 = sqlx::query!(
            "SELECT fetch_interval_minutes FROM feeds WHERE id = ?",
            feed_id
        )
        .fetch_one(&self.pool)
        .await?
        .fetch_interval_minutes;
        let mut title = sqlx::query!("SELECT title FROM feeds WHERE id = ?", feed_id)
            .fetch_one(&self.pool)
            .await?
            .title;
        let mut description = sqlx::query!("SELECT description FROM feeds WHERE id = ?", feed_id)
            .fetch_one(&self.pool)
            .await?
            .description;
        let mut site_url = sqlx::query!("SELECT site_url FROM feeds WHERE id = ?", feed_id)
            .fetch_one(&self.pool)
            .await?
            .site_url;

        if let Some(v) = req.fetch_interval_minutes {
            let allowed: [i64; 8] = [5, 15, 30, 60, 120, 240, 720, 1440];
            if !allowed.contains(&v) {
                return Err(AppError::BadRequest("invalid fetch interval".into()));
            }
            interval = v;
        }
        if req.title.is_some() {
            title = req.title;
        }
        if req.description.is_some() {
            description = req.description;
        }
        if req.site_url.is_some() {
            site_url = req.site_url;
        }

        let updated_at = Utc::now().timestamp_millis();
        sqlx::query!(
            "UPDATE feeds SET fetch_interval_minutes = ?, title = ?, description = ?, site_url = ?, updated_at = ? WHERE id = ?",
            interval,
            title,
            description,
            site_url,
            updated_at,
            feed_id
        )
        .execute(&self.pool)
        .await?;

        self.get(user_id, feed_id).await
    }

    pub async fn delete(&self, user_id: i64, feed_id: i64) -> Result<()> {
        sqlx::query!(
            "DELETE FROM subscriptions WHERE user_id = ? AND feed_id = ?",
            user_id,
            feed_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn refresh(&self, user_id: i64, feed_id: i64) -> Result<()> {
        self.get(user_id, feed_id).await?;
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            "UPDATE feeds SET next_fetch_at = ?, cache_until = NULL WHERE id = ?",
            now,
            feed_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn discover(&self, req: DiscoverRequest) -> Result<DiscoverResponse> {
        req.validate()?;
        let html = self
            .client
            .get(req.url.clone())
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("fetch failed: {}", e)))?
            .text()
            .await
            .map_err(|e| AppError::Internal(format!("read failed: {}", e)))?;

        let seen = {
            let base_url = Url::parse(&req.url)
                .map_err(|e| AppError::BadRequest(format!("invalid URL: {}", e)))?;
            let document = Html::parse_document(&html);
            let selector = Selector::parse("link[rel=alternate]")
                .map_err(|e| AppError::Internal(format!("selector: {:?}", e)))?;

            let mut candidates = Vec::new();
            for el in document.select(&selector) {
                let type_attr = el.value().attr("type").unwrap_or("");
                let href = el.value().attr("href").unwrap_or("");
                if !is_feed_type(type_attr) || href.is_empty() {
                    continue;
                }
                if let Ok(abs) = base_url.join(href) {
                    candidates.push(abs.to_string());
                }
            }
            candidates
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        };

        let mut feeds = Vec::new();
        for url in seen {
            if let Ok(title) = self.fetch_feed_title(&url).await {
                feeds.push(DiscoveredFeed { url, title });
            }
        }

        Ok(DiscoverResponse { feeds })
    }

    pub async fn import_opml(&self, user_id: i64, data: &[u8]) -> Result<ImportResult> {
        let text = std::str::from_utf8(data)
            .map_err(|e| AppError::BadRequest(format!("invalid UTF-8: {}", e)))?;
        let opml = opml::OPML::from_str(text)
            .map_err(|e| AppError::BadRequest(format!("invalid OPML: {}", e)))?;

        let mut total = 0;
        let mut imported = 0;
        let mut failed = 0;
        let mut feeds = Vec::new();

        for outline in &opml.body.outlines {
            total += 1;
            let url = outline
                .xml_url
                .clone()
                .or_else(|| outline.html_url.clone())
                .unwrap_or_default();
            if url.is_empty() {
                failed += 1;
                feeds.push(ImportedFeed {
                    url,
                    title: Some(outline.text.clone()),
                    status: "no_url".into(),
                });
                continue;
            }
            let title = Some(outline.text.clone()).or_else(|| outline.title.clone());
            match self
                .create_with_title(user_id, &url, title.as_deref())
                .await
            {
                Ok(feed) => {
                    imported += 1;
                    feeds.push(ImportedFeed {
                        url: feed.url,
                        title: feed.title,
                        status: "imported".into(),
                    });
                }
                Err(_) => {
                    failed += 1;
                    feeds.push(ImportedFeed {
                        url,
                        title,
                        status: "failed".into(),
                    });
                }
            }
        }

        Ok(ImportResult {
            total,
            imported,
            failed,
            feeds,
        })
    }

    pub async fn export_opml(&self, user_id: i64) -> Result<String> {
        let rows = sqlx::query!(
            r#"
            SELECT f.url, f.title, f.site_url
            FROM feeds f
            JOIN subscriptions s ON s.feed_id = f.id
            WHERE s.user_id = ?
            ORDER BY f.id ASC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<opml version=\"2.0\">\n");
        xml.push_str("  <head><title>Subscriptions</title></head>\n");
        xml.push_str("  <body>\n");
        for r in rows {
            let title = r.title.unwrap_or_else(|| r.url.clone());
            let escaped_title = quick_xml::escape::escape(&title);
            let escaped_url = quick_xml::escape::escape(&r.url);
            let html = r.site_url.as_deref().unwrap_or("");
            let escaped_html = quick_xml::escape::escape(html);
            xml.push_str(&format!(
                "    <outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\" />\n",
                escaped_title, escaped_title, escaped_url, escaped_html
            ));
        }
        xml.push_str("  </body>\n");
        xml.push_str("</opml>\n");
        Ok(xml)
    }

    async fn find_feed_by_url(&self, url: &str) -> Result<Option<i64>> {
        let row = sqlx::query!("SELECT id FROM feeds WHERE url = ?", url)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| r.id))
    }

    async fn create_with_title(
        &self,
        user_id: i64,
        url: &str,
        title: Option<&str>,
    ) -> Result<Feed> {
        let now = Utc::now();
        let millis = now.timestamp_millis();

        let feed_id = match self.find_feed_by_url(url).await? {
            Some(id) => id,
            None => {
                let title = title.map(|s| s.to_string());
                sqlx::query!(
                    "INSERT INTO feeds (url, title, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                    url,
                    title,
                    15,
                    millis,
                    millis,
                    millis
                )
                .execute(&self.pool)
                .await?
                .last_insert_rowid()
            }
        };

        sqlx::query!(
            "INSERT OR IGNORE INTO subscriptions (user_id, feed_id, created_at) VALUES (?, ?, ?)",
            user_id,
            feed_id,
            millis
        )
        .execute(&self.pool)
        .await?;

        self.get(user_id, feed_id).await
    }

    async fn fetch_feed_title(&self, url: &str) -> Result<Option<String>> {
        let bytes = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("fetch failed: {}", e)))?
            .bytes()
            .await
            .map_err(|e| AppError::Internal(format!("read failed: {}", e)))?;

        let text = String::from_utf8_lossy(&bytes);
        if text.trim_start().starts_with("<?xml")
            || text.trim_start().starts_with("<feed")
            || text.trim_start().starts_with("<rss")
        {
            // Try RSS.
            if let Ok(channel) = rss::Channel::read_from(&bytes[..]) {
                let title = channel.title().to_string();
                return Ok(Some(title).filter(|s| !s.is_empty()));
            }
            // Try Atom.
            if let Ok(feed) = atom_syndication::Feed::read_from(&bytes[..]) {
                let title = feed.title().value.to_string();
                return Ok(Some(title).filter(|s| !s.is_empty()));
            }
        }

        // Try JSON Feed.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(title) = json.get("title").and_then(|v| v.as_str())
        {
            return Ok(Some(title.to_string()).filter(|s| !s.is_empty()));
        }

        Ok(None)
    }
}

fn is_feed_type(t: &str) -> bool {
    matches!(
        t,
        "application/rss+xml"
            | "application/atom+xml"
            | "application/feed+json"
            | "application/json"
            | "text/rss"
            | "text/atom"
    )
}

fn compute_status(failures: i64, backoff_until: Option<i64>) -> String {
    let now = Utc::now().timestamp_millis();
    if let Some(until) = backoff_until
        && until > now
    {
        return "backoff".into();
    }
    if failures > 0 {
        return "error".into();
    }
    "ok".into()
}
