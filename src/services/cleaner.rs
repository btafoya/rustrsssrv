use std::io::Cursor;
use std::sync::LazyLock;

use html_to_markdown_rs::{ConversionOptions, convert};
use readability::{ExtractOptions, extract};
use regex::Regex;
use reqwest::{Client, Url};

use crate::errors::{AppError, Result};

const TRUNCATED_WORD_THRESHOLD: usize = 50;
const TRUNCATED_CHAR_THRESHOLD: usize = 200;

#[derive(Clone)]
pub struct CleanerService {
    client: Client,
}

pub struct CleanedContent {
    pub raw_html: String,
    pub markdown: String,
}

impl CleanerService {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Convert HTML to Markdown after stripping scripts, styles, and common ad
    /// elements.
    pub fn clean(&self, html: &str) -> Result<CleanedContent> {
        let sanitized = sanitize_html(html);
        let markdown = convert(&sanitized, ConversionOptions::default())
            .map_err(|e| AppError::Internal(format!("html to markdown: {}", e)))?
            .content
            .unwrap_or_default();
        Ok(CleanedContent {
            raw_html: html.to_string(),
            markdown,
        })
    }

    /// Convert feed HTML to Markdown. If the result looks truncated, fetch the
    /// origin page and run it through readability, falling back to the feed
    /// content on failure.
    pub async fn clean_with_fallback(
        &self,
        article_url: &str,
        feed_html: String,
    ) -> Result<CleanedContent> {
        let feed_clean = self.clean(&feed_html)?;
        if !is_truncated(&feed_clean.markdown) {
            return Ok(feed_clean);
        }

        match self.fetch_origin(article_url).await {
            Ok(origin_html) => {
                let origin_clean = self.clean(&origin_html)?;
                Ok(CleanedContent {
                    raw_html: origin_html,
                    markdown: origin_clean.markdown,
                })
            }
            Err(e) => {
                tracing::warn!(
                    "origin fetch failed for {}; using truncated feed content: {}",
                    article_url,
                    e
                );
                Ok(feed_clean)
            }
        }
    }

    async fn fetch_origin(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("origin fetch: {}", e)))?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AppError::Internal(format!("origin body: {}", e)))?;
        let html = String::from_utf8_lossy(&bytes).to_string();

        let url = Url::parse(url).map_err(|e| AppError::Internal(format!("bad url: {}", e)))?;
        let mut cursor = Cursor::new(html.clone());
        let readable = extract(&mut cursor, &url, ExtractOptions::default())
            .map_err(|e| AppError::Internal(format!("readability: {}", e)))?;
        Ok(readable.content)
    }
}

fn is_truncated(markdown: &str) -> bool {
    let words: Vec<&str> = markdown.split_whitespace().collect();
    words.len() < TRUNCATED_WORD_THRESHOLD || markdown.len() < TRUNCATED_CHAR_THRESHOLD
}

static SCRIPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)<script\b[^>]*>.*?</script>"#).unwrap());
static STYLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)<style\b[^>]*>.*?</style>"#).unwrap());
static AD_DIV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<div\b[^>]*\bclass\s*=\s*["'][^"']*(?:\bad\b|advertisement|banner)[^"']*["'][^>]*>.*?</div>"#).unwrap()
});
static AD_INS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<ins\b[^>]*\bclass\s*=\s*["'][^"']*adsbygoogle[^"']*["'][^>]*>.*?</ins>"#)
        .unwrap()
});

/// Strip scripts, styles, and obvious ad elements before Markdown conversion.
fn sanitize_html(html: &str) -> String {
    let mut out = html.to_string();
    out = SCRIPT_RE.replace_all(&out, "").to_string();
    out = STYLE_RE.replace_all(&out, "").to_string();
    out = AD_DIV_RE.replace_all(&out, "").to_string();
    out = AD_INS_RE.replace_all(&out, "").to_string();
    out
}
