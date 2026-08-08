use base64::Engine;
use reqwest::{Client, Url};
use sqlx::SqlitePool;

use crate::errors::{AppError, Result};

const INLINE_SIZE_LIMIT: usize = 128 * 1024;

#[derive(Clone)]
pub struct MediaService {
    pool: SqlitePool,
    client: Client,
}

pub enum MediaOutcome {
    Inline { mime: String, data: Vec<u8> },
    Proxy { hash: String },
}

impl MediaService {
    pub fn new(pool: SqlitePool, client: Client) -> Self {
        Self { pool, client }
    }

    /// Fetch an asset by URL. Small assets are returned inline; large assets are
    /// stored in SQLite and referenced by BLAKE3 hash.
    pub async fn store_from_url(&self, url: &str) -> Result<MediaOutcome> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("media fetch: {}", e)))?;
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
            .or_else(|| guess_mime(url))
            .unwrap_or_else(|| "application/octet-stream".into());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AppError::Internal(format!("media body: {}", e)))?;
        let data = bytes.to_vec();

        if data.len() <= INLINE_SIZE_LIMIT {
            return Ok(MediaOutcome::Inline { mime, data });
        }

        let hash = blake3::hash(&data).to_hex().to_string();
        let size = data.len() as i64;
        sqlx::query!(
            r#"
            INSERT OR IGNORE INTO media (content_hash, origin_url, mime_type, size_bytes, data)
            VALUES (?, ?, ?, ?, ?)
            "#,
            hash,
            url,
            mime,
            size,
            data
        )
        .execute(&self.pool)
        .await?;

        Ok(MediaOutcome::Proxy { hash })
    }

    /// Look up a stored asset by its BLAKE3 hash.
    pub async fn get_by_hash(&self, hash: &str) -> Result<Option<(String, Vec<u8>)>> {
        let row = sqlx::query!(
            "SELECT mime_type, data FROM media WHERE content_hash = ?",
            hash
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| (r.mime_type, r.data)))
    }

    /// Rewrite Markdown image references. Relative URLs are resolved against the
    /// article URL. Small images become base64 data URIs; large images are
    /// replaced with `/api/v1/media/{hash}`.
    pub async fn rewrite_markdown(&self, markdown: &str, article_url: &str) -> Result<String> {
        let base_url = Url::parse(article_url)
            .map_err(|e| AppError::Internal(format!("bad article url: {}", e)))?;
        let mut out = String::with_capacity(markdown.len());
        let mut last_end = 0;

        // Match `![alt](url)` and `![alt](url "title")`.
        static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r#"!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#).unwrap()
        });

        for cap in RE.captures_iter(markdown) {
            let whole = cap.get(0).unwrap();
            out.push_str(&markdown[last_end..whole.start()]);
            last_end = whole.end();

            let alt = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let url = cap.get(2).map(|m| m.as_str()).unwrap_or("");

            if url.starts_with("data:") {
                out.push_str(whole.as_str());
                continue;
            }

            let resolved = resolve_url(url, &base_url);
            match self.store_from_url(&resolved).await {
                Ok(MediaOutcome::Inline { mime, data }) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    let data_url = format!("data:{};base64,{}", mime, b64);
                    out.push_str(&format!("![{}]({})", alt, data_url));
                }
                Ok(MediaOutcome::Proxy { hash }) => {
                    out.push_str(&format!("![{}](/api/v1/media/{})", alt, hash));
                }
                Err(e) => {
                    tracing::warn!("failed to process image {}: {}", resolved, e);
                    out.push_str(whole.as_str());
                }
            }
        }

        out.push_str(&markdown[last_end..]);
        Ok(out)
    }
}

fn guess_mime(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let path = parsed.path();
    let ext = path.rsplit('.').next()?;
    match ext.to_lowercase().as_str() {
        "png" => Some("image/png".into()),
        "jpg" | "jpeg" => Some("image/jpeg".into()),
        "gif" => Some("image/gif".into()),
        "webp" => Some("image/webp".into()),
        "svg" => Some("image/svg+xml".into()),
        "ico" => Some("image/x-icon".into()),
        "mp3" => Some("audio/mpeg".into()),
        "mp4" => Some("video/mp4".into()),
        "ogg" => Some("audio/ogg".into()),
        "webm" => Some("video/webm".into()),
        _ => None,
    }
}

fn resolve_url(url: &str, base: &Url) -> String {
    base.join(url)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| url.to_string())
}
