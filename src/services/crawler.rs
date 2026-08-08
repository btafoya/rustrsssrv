use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use reqwest::{Client, Url};
use sqlx::SqlitePool;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time;

use crate::errors::{AppError, Result};
use crate::models::ArticleInput;

#[derive(Clone)]
pub struct CrawlerService {
    pool: SqlitePool,
    client: Client,
}

pub struct FeedRow {
    pub id: i64,
    pub url: String,
    pub last_etag: Option<String>,
    pub last_modified: Option<String>,
}

const MAX_CONCURRENT: usize = 4;
const POLITENESS_MS: u64 = 500;
const DAILY_MS: i64 = 24 * 60 * 60 * 1000;

impl CrawlerService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("build http client"),
        }
    }

    pub async fn run(&self) {
        loop {
            self.run_once().await;
            time::sleep(Duration::from_secs(60)).await;
        }
    }

    pub async fn run_once(&self) {
        let due = match self.due_feeds().await {
            Ok(due) => due,
            Err(e) => {
                tracing::error!("failed to fetch due feeds: {}", e);
                return;
            }
        };

        if due.is_empty() {
            return;
        }

        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
        let politeness = Arc::new(Mutex::new(Instant::now()));
        let mut set = JoinSet::new();

        for feed in due {
            let permit = semaphore.clone().acquire_owned().await.unwrap_or_else(|e| {
                tracing::error!("semaphore closed: {}", e);
                panic!("semaphore closed")
            });
            let svc = self.clone();
            let p = politeness.clone();
            set.spawn(async move {
                let _permit = permit;
                let wait = {
                    let mut last = p.lock().await;
                    let now = Instant::now();
                    let wait = last.saturating_duration_since(now);
                    *last = now + Duration::from_millis(POLITENESS_MS) + wait;
                    wait
                };
                let feed_id = feed.id;
                if wait > Duration::ZERO {
                    time::sleep(wait).await;
                }
                if let Err(e) = svc.fetch_feed(feed).await {
                    tracing::warn!("feed {} fetch failed: {}", feed_id, e);
                }
            });
        }

        while set.join_next().await.is_some() {}
    }

    async fn due_feeds(&self) -> Result<Vec<FeedRow>> {
        let now = Utc::now().timestamp_millis();
        let rows = sqlx::query!(
            r#"
            SELECT id as "id!", url, last_etag, last_modified
            FROM feeds
            WHERE next_fetch_at <= ?
            ORDER BY next_fetch_at ASC
            LIMIT 100
            "#,
            now
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| FeedRow {
                id: r.id,
                url: r.url,
                last_etag: r.last_etag,
                last_modified: r.last_modified,
            })
            .collect())
    }

    pub async fn fetch_feed(&self, feed: FeedRow) -> Result<()> {
        let feed_id = feed.id;
        let feed_url = feed.url.clone();

        let mut request = self.client.get(&feed_url);
        if let Some(etag) = &feed.last_etag {
            request = request.header("If-None-Match", etag);
        }
        if let Some(last_modified) = &feed.last_modified {
            request = request.header("If-Modified-Since", last_modified);
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                return self
                    .record_failure(feed_id, &format!("fetch failed: {}", e))
                    .await;
            }
        };

        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return self.record_success(feed_id, 0, None, None).await;
        }

        let final_url = response.url().clone();
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return self
                    .record_failure(feed_id, &format!("read body failed: {}", e))
                    .await;
            }
        };

        if status.is_client_error() || status.is_server_error() {
            return self
                .record_failure(feed_id, &format!("http status {}", status.as_u16()))
                .await;
        }

        let text = String::from_utf8_lossy(&bytes);
        let content_type = content_type_hint(&text);

        let articles = match content_type {
            FeedType::Rss => parse_rss(&bytes, &feed_url, &final_url),
            FeedType::Atom => parse_atom(&bytes, &feed_url, &final_url),
            FeedType::Json => parse_json(&bytes, &feed_url, &final_url),
            FeedType::Unknown => {
                // Try each format.
                if let Ok(a) = parse_rss(&bytes, &feed_url, &final_url) {
                    Ok(a)
                } else if let Ok(a) = parse_atom(&bytes, &feed_url, &final_url) {
                    Ok(a)
                } else {
                    parse_json(&bytes, &feed_url, &final_url)
                }
            }
        };

        let articles = match articles {
            Ok(a) => a,
            Err(e) => {
                return self
                    .record_failure(feed_id, &format!("parse failed: {}", e))
                    .await;
            }
        };

        let count = articles.len();
        let now_ms = Utc::now().timestamp_millis();
        for input in articles {
            if let Err(e) = self.upsert_article(feed_id, input, now_ms).await {
                tracing::warn!("failed to upsert article for feed {}: {}", feed_id, e);
            }
        }

        self.record_success(feed_id, count, etag, last_modified)
            .await
    }

    async fn upsert_article(&self, feed_id: i64, input: ArticleInput, now_ms: i64) -> Result<()> {
        let markdown = input.summary.clone().unwrap_or_default();
        let raw_html = input.content_html;

        let article_id = sqlx::query!(
            r#"
            INSERT INTO articles (url, title, summary, raw_html, markdown_content, published_at, fetched_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                summary = excluded.summary,
                raw_html = excluded.raw_html,
                markdown_content = excluded.markdown_content,
                fetched_at = excluded.fetched_at,
                updated_at = excluded.updated_at
            RETURNING id
            "#,
            input.url,
            input.title,
            input.summary,
            raw_html,
            markdown,
            input.published_at,
            now_ms,
            now_ms
        )
        .fetch_one(&self.pool)
        .await?
        .id;

        sqlx::query!(
            "INSERT OR IGNORE INTO article_feeds (article_id, feed_id, first_seen_at) VALUES (?, ?, ?)",
            article_id,
            feed_id,
            now_ms
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
            INSERT OR IGNORE INTO read_states (user_id, article_id, created_at, updated_at)
            SELECT user_id, ?, ?, ? FROM subscriptions WHERE feed_id = ?
            "#,
            article_id,
            now_ms,
            now_ms,
            feed_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn record_success(
        &self,
        feed_id: i64,
        articles_found: usize,
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<()> {
        let now_ms = Utc::now().timestamp_millis();
        let row = sqlx::query!(
            "SELECT fetch_interval_minutes FROM feeds WHERE id = ?",
            feed_id
        )
        .fetch_one(&self.pool)
        .await?;
        let interval_ms = row.fetch_interval_minutes * 60 * 1000;
        let next_fetch_at = now_ms + interval_ms;

        sqlx::query!(
            r#"
            UPDATE feeds
            SET last_fetched_at = ?,
                next_fetch_at = ?,
                consecutive_failures = 0,
                backoff_until = NULL,
                last_etag = ?,
                last_modified = ?,
                updated_at = ?
            WHERE id = ?
            "#,
            now_ms,
            next_fetch_at,
            etag,
            last_modified,
            now_ms,
            feed_id
        )
        .execute(&self.pool)
        .await?;

        let articles_found_i64 = articles_found as i64;
        sqlx::query!(
            "INSERT INTO feed_fetch_logs (feed_id, status, articles_found) VALUES (?, 'ok', ?)",
            feed_id,
            articles_found_i64
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn record_failure(&self, feed_id: i64, message: &str) -> Result<()> {
        let now_ms = Utc::now().timestamp_millis();
        let row = sqlx::query!(
            "SELECT consecutive_failures FROM feeds WHERE id = ?",
            feed_id
        )
        .fetch_one(&self.pool)
        .await?;
        let failures = row.consecutive_failures + 1;
        let failures_u32: u32 = failures.try_into().unwrap_or(u32::MAX);
        let backoff_secs = 2_u64.saturating_pow(failures_u32).saturating_mul(300);
        let backoff_ms = Duration::from_secs(backoff_secs).as_millis() as i64;
        let backoff_ms = backoff_ms.min(DAILY_MS);
        let backoff_until = now_ms + backoff_ms;
        let next_fetch_at = backoff_until;

        sqlx::query!(
            r#"
            UPDATE feeds
            SET consecutive_failures = ?,
                backoff_until = ?,
                next_fetch_at = ?,
                updated_at = ?
            WHERE id = ?
            "#,
            failures,
            backoff_until,
            next_fetch_at,
            now_ms,
            feed_id
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            "INSERT INTO feed_fetch_logs (feed_id, status, error_message) VALUES (?, 'error', ?)",
            feed_id,
            message
        )
        .execute(&self.pool)
        .await?;

        Err(AppError::Internal(message.into()))
    }
}

#[derive(Debug, Clone, Copy)]
enum FeedType {
    Rss,
    Atom,
    Json,
    Unknown,
}

fn content_type_hint(text: &str) -> FeedType {
    let trimmed = text.trim_start();
    if trimmed.starts_with("<?xml") {
        if trimmed.contains("<rss")
            || trimmed.contains("<channel")
            || trimmed.contains("http://www.w3.org/2005/Atom")
        {
            return FeedType::Rss;
        }
        if trimmed.contains("<feed") {
            return FeedType::Atom;
        }
    }
    if trimmed.starts_with("<rss") || trimmed.starts_with("<channel") {
        return FeedType::Rss;
    }
    if trimmed.starts_with("<feed") {
        return FeedType::Atom;
    }
    if trimmed.starts_with('{') {
        return FeedType::Json;
    }
    FeedType::Unknown
}

fn parse_rss(bytes: &[u8], feed_url: &str, final_url: &Url) -> Result<Vec<ArticleInput>> {
    let channel = rss::Channel::read_from(bytes)
        .map_err(|e| AppError::Internal(format!("rss parse: {}", e)))?;
    let base = if channel.link().is_empty() {
        feed_url.to_string()
    } else {
        channel.link().to_string()
    };
    let base_url = Url::parse(&base).unwrap_or_else(|_| final_url.clone());
    let mut out = Vec::new();
    for item in channel.items() {
        let url = item
            .link()
            .map(|s| s.to_string())
            .or_else(|| {
                item.guid()
                    .and_then(|g| g.is_permalink().then(|| g.value().to_string()))
            })
            .unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let url = resolve_url(&url, &base_url);
        let title = item.title().unwrap_or("Untitled").to_string();
        let summary = item.description().map(|s| s.to_string());
        let content_html = item.content().map(|s| s.to_string());
        let published_at = item
            .pub_date()
            .and_then(|d| chrono::DateTime::parse_from_rfc2822(d).ok())
            .map(|d| d.timestamp_millis());
        out.push(ArticleInput {
            url,
            title,
            summary,
            content_html,
            published_at,
        });
    }
    Ok(out)
}

fn parse_atom(bytes: &[u8], _feed_url: &str, final_url: &Url) -> Result<Vec<ArticleInput>> {
    let feed = atom_syndication::Feed::read_from(bytes)
        .map_err(|e| AppError::Internal(format!("atom parse: {}", e)))?;
    let base_url = feed
        .links()
        .iter()
        .find(|l| l.rel() == "alternate")
        .map(|l| l.href().to_string())
        .and_then(|h| Url::parse(&h).ok())
        .unwrap_or_else(|| final_url.clone());
    let mut out = Vec::new();
    for entry in feed.entries() {
        let url = entry
            .links()
            .iter()
            .find(|l| l.rel() == "alternate")
            .map(|l| l.href().to_string())
            .or_else(|| entry.links().first().map(|l| l.href().to_string()))
            .unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let url = resolve_url(&url, &base_url);
        let title = entry.title().value.clone();
        let summary = entry
            .summary()
            .map(|s| s.value.clone())
            .or_else(|| entry.content().and_then(|c| c.value.clone()));
        let content_html = entry.content().and_then(|c| c.value.clone());
        let published_at = entry
            .published()
            .map(|d| d.timestamp_millis())
            .or_else(|| Some(entry.updated().timestamp_millis()));
        out.push(ArticleInput {
            url,
            title,
            summary,
            content_html,
            published_at,
        });
    }
    Ok(out)
}

fn parse_json(bytes: &[u8], _feed_url: &str, final_url: &Url) -> Result<Vec<ArticleInput>> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| AppError::Internal(format!("utf8: {}", e)))?;
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AppError::Internal(format!("json parse: {}", e)))?;
    let feed_url = json
        .get("feed_url")
        .and_then(|v| v.as_str())
        .unwrap_or(final_url.as_str());
    let base_url = Url::parse(feed_url).unwrap_or_else(|_| final_url.clone());
    let items = json
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for item in items {
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if url.is_empty() {
            continue;
        }
        let url = resolve_url(&url, &base_url);
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();
        let summary = item
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let content_html = item
            .get("content_html")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                item.get("content_text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
        let published_at = item
            .get("date_published")
            .and_then(|v| v.as_str())
            .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
            .map(|d| d.timestamp_millis());
        out.push(ArticleInput {
            url,
            title,
            summary,
            content_html,
            published_at,
        });
    }
    Ok(out)
}

fn resolve_url(url: &str, base: &Url) -> String {
    base.join(url)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| url.to_string())
}
