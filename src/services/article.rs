use chrono::Utc;
use sqlx::SqlitePool;

use crate::errors::{AppError, Result};
use crate::models::{Article, ArticlePage, ListArticlesQuery};

#[derive(sqlx::FromRow)]
struct SearchRow {
    id: i64,
    url: String,
    title: String,
    summary: Option<String>,
    markdown_content: String,
    published_at: Option<i64>,
    fetched_at: i64,
    feed_id: i64,
    feed_title: Option<String>,
    is_read: i32,
    is_starred: i32,
}

#[derive(Clone)]
pub struct ArticleService {
    pool: SqlitePool,
}

impl ArticleService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, user_id: i64, query: ListArticlesQuery) -> Result<ArticlePage> {
        let newest_first = query.is_newest_first() as i32;
        let limit = query.limit.unwrap_or(20).clamp(1, 100);
        let cursor = query
            .cursor
            .unwrap_or(if newest_first != 0 { i64::MAX } else { 0 });
        let page_size = limit + 1;

        let is_read_filter = match query.is_read {
            Some(true) => Some(1),
            Some(false) => Some(0),
            None => None,
        };
        let is_starred_filter = match query.is_starred {
            Some(true) => Some(1),
            Some(false) => Some(0),
            None => None,
        };

        let rows = sqlx::query!(
            r#"
            SELECT
                a.id as "id!",
                a.url,
                a.title,
                a.summary,
                a.markdown_content,
                a.published_at,
                a.fetched_at,
                af.feed_id as "feed_id!",
                f.title as feed_title,
                COALESCE(rs.is_read, 0) as "is_read!: i32",
                COALESCE(rs.is_starred, 0) as "is_starred!: i32"
            FROM articles a
            JOIN article_feeds af ON af.article_id = a.id
            JOIN subscriptions s ON s.feed_id = af.feed_id AND s.user_id = ?
            JOIN feeds f ON f.id = af.feed_id
            LEFT JOIN read_states rs ON rs.article_id = a.id AND rs.user_id = ?
            WHERE af.feed_id = (
                SELECT MIN(af2.feed_id)
                FROM article_feeds af2
                JOIN subscriptions s2 ON s2.feed_id = af2.feed_id AND s2.user_id = ?
                WHERE af2.article_id = a.id
            )
            AND (
                CASE WHEN ? = 1 THEN a.id < ? ELSE a.id > ? END
            )
            AND (? IS NULL OR af.feed_id = ?)
            AND (? IS NULL OR rs.is_read = ?)
            AND (? IS NULL OR rs.is_starred = ?)
            ORDER BY
                CASE WHEN ? = 1 THEN a.id END DESC,
                CASE WHEN ? = 0 THEN a.id END ASC
            LIMIT ?
            "#,
            user_id,
            user_id,
            user_id,
            newest_first,
            cursor,
            cursor,
            query.feed_id,
            query.feed_id,
            is_read_filter,
            is_read_filter,
            is_starred_filter,
            is_starred_filter,
            newest_first,
            newest_first,
            page_size
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items: Vec<Article> = rows
            .into_iter()
            .map(|r| Article {
                id: r.id,
                url: r.url,
                title: r.title,
                summary: r.summary,
                markdown_content: r.markdown_content,
                published_at: r
                    .published_at
                    .map(|ms| chrono::DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now)),
                fetched_at: chrono::DateTime::from_timestamp_millis(r.fetched_at)
                    .unwrap_or_else(Utc::now),
                feed_id: r.feed_id,
                feed_title: r.feed_title,
                is_read: r.is_read != 0,
                is_starred: r.is_starred != 0,
            })
            .collect();

        let has_more = items.len() > limit as usize;
        if has_more {
            items.pop();
        }
        let next_cursor = if has_more {
            items.last().map(|a| a.id)
        } else {
            None
        };

        Ok(ArticlePage {
            items,
            next_cursor,
            has_more,
        })
    }

    pub async fn get(&self, user_id: i64, article_id: i64) -> Result<Article> {
        let row = sqlx::query!(
            r#"
            SELECT
                a.id as "id!",
                a.url,
                a.title,
                a.summary,
                a.markdown_content,
                a.published_at,
                a.fetched_at,
                af.feed_id as "feed_id!",
                f.title as feed_title,
                COALESCE(rs.is_read, 0) as "is_read!: i32",
                COALESCE(rs.is_starred, 0) as "is_starred!: i32"
            FROM articles a
            JOIN article_feeds af ON af.article_id = a.id
            JOIN subscriptions s ON s.feed_id = af.feed_id AND s.user_id = ?
            JOIN feeds f ON f.id = af.feed_id
            LEFT JOIN read_states rs ON rs.article_id = a.id AND rs.user_id = ?
            WHERE a.id = ?
            AND af.feed_id = (
                SELECT MIN(af2.feed_id)
                FROM article_feeds af2
                JOIN subscriptions s2 ON s2.feed_id = af2.feed_id AND s2.user_id = ?
                WHERE af2.article_id = a.id
            )
            "#,
            user_id,
            user_id,
            article_id,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(AppError::NotFound)?;
        Ok(Article {
            id: row.id,
            url: row.url,
            title: row.title,
            summary: row.summary,
            markdown_content: row.markdown_content,
            published_at: row
                .published_at
                .map(|ms| chrono::DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now)),
            fetched_at: chrono::DateTime::from_timestamp_millis(row.fetched_at)
                .unwrap_or_else(Utc::now),
            feed_id: row.feed_id,
            feed_title: row.feed_title,
            is_read: row.is_read != 0,
            is_starred: row.is_starred != 0,
        })
    }

    async fn verify_subscription(&self, user_id: i64, article_id: i64) -> Result<(i64, i64)> {
        let row = sqlx::query!(
            r#"
            SELECT a.id as "id!", MIN(af.feed_id) as "feed_id!: i64"
            FROM articles a
            JOIN article_feeds af ON af.article_id = a.id
            JOIN subscriptions s ON s.feed_id = af.feed_id AND s.user_id = ?
            WHERE a.id = ?
            GROUP BY a.id
            "#,
            user_id,
            article_id
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| (r.id, r.feed_id)).ok_or(AppError::NotFound)
    }

    pub async fn mark_read(&self, user_id: i64, article_id: i64) -> Result<()> {
        self.verify_subscription(user_id, article_id).await?;
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
            INSERT INTO read_states (user_id, article_id, is_read, read_at, created_at, updated_at)
            VALUES (?, ?, 1, ?, ?, ?)
            ON CONFLICT(user_id, article_id) DO UPDATE SET
                is_read = 1,
                read_at = excluded.read_at,
                updated_at = excluded.updated_at
            "#,
            user_id,
            article_id,
            now,
            now,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_unread(&self, user_id: i64, article_id: i64) -> Result<()> {
        self.verify_subscription(user_id, article_id).await?;
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
            INSERT INTO read_states (user_id, article_id, is_read, read_at, created_at, updated_at)
            VALUES (?, ?, 0, NULL, ?, ?)
            ON CONFLICT(user_id, article_id) DO UPDATE SET
                is_read = 0,
                read_at = NULL,
                updated_at = excluded.updated_at
            "#,
            user_id,
            article_id,
            now,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_starred(&self, user_id: i64, article_id: i64) -> Result<()> {
        self.verify_subscription(user_id, article_id).await?;
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
            INSERT INTO read_states (user_id, article_id, is_starred, starred_at, created_at, updated_at)
            VALUES (?, ?, 1, ?, ?, ?)
            ON CONFLICT(user_id, article_id) DO UPDATE SET
                is_starred = 1,
                starred_at = excluded.starred_at,
                updated_at = excluded.updated_at
            "#,
            user_id,
            article_id,
            now,
            now,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_unstarred(&self, user_id: i64, article_id: i64) -> Result<()> {
        self.verify_subscription(user_id, article_id).await?;
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
            INSERT INTO read_states (user_id, article_id, is_starred, starred_at, created_at, updated_at)
            VALUES (?, ?, 0, NULL, ?, ?)
            ON CONFLICT(user_id, article_id) DO UPDATE SET
                is_starred = 0,
                starred_at = NULL,
                updated_at = excluded.updated_at
            "#,
            user_id,
            article_id,
            now,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn search(&self, user_id: i64, q: &str, limit: i64) -> Result<ArticlePage> {
        let limit = limit.clamp(1, 100);
        // The sqlx compile-time checked macro returns empty rows when MATCH is
        // parameterized against the FTS5 virtual table, while the identical raw
        // query returns results. We keep search as a single raw query rather than
        // fight the macro/FTS5 interaction.
        let rows: Vec<SearchRow> = sqlx::query_as(
            r#"
            SELECT
                a.id,
                a.url,
                a.title,
                a.summary,
                a.markdown_content,
                a.published_at,
                a.fetched_at,
                af.feed_id,
                f.title as feed_title,
                COALESCE(rs.is_read, 0) as is_read,
                COALESCE(rs.is_starred, 0) as is_starred
            FROM articles_fts fts
            JOIN articles a ON a.id = fts.rowid
            JOIN article_feeds af ON af.article_id = a.id
            JOIN subscriptions s ON s.feed_id = af.feed_id AND s.user_id = ?
            JOIN feeds f ON f.id = af.feed_id
            LEFT JOIN read_states rs ON rs.article_id = a.id AND rs.user_id = ?
            WHERE articles_fts MATCH ?
            AND af.feed_id = (
                SELECT MIN(af2.feed_id)
                FROM article_feeds af2
                JOIN subscriptions s2 ON s2.feed_id = af2.feed_id AND s2.user_id = ?
                WHERE af2.article_id = a.id
            )
            ORDER BY rank
            LIMIT ?
            "#,
        )
        .bind(user_id)
        .bind(user_id)
        .bind(q)
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let items: Vec<Article> = rows
            .into_iter()
            .map(|r| Article {
                id: r.id,
                url: r.url,
                title: r.title,
                summary: r.summary,
                markdown_content: r.markdown_content,
                published_at: r
                    .published_at
                    .map(|ms| chrono::DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now)),
                fetched_at: chrono::DateTime::from_timestamp_millis(r.fetched_at)
                    .unwrap_or_else(Utc::now),
                feed_id: r.feed_id,
                feed_title: r.feed_title,
                is_read: r.is_read != 0,
                is_starred: r.is_starred != 0,
            })
            .collect();

        Ok(ArticlePage {
            items,
            next_cursor: None,
            has_more: false,
        })
    }
}
