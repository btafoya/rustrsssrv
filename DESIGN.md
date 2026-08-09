# Rust RSS Server — Design

Derived from `REQUIREMENTS.md`. This document is the blueprint for `/sc:implement`.

## 1. Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                     Rust binary (Axum)                       │
│  ┌──────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Web routes  │  │  API routes │  │   Static assets     │  │
│  │  (Askama)    │  │  (/api/v1)  │  │   (rust-embed)      │  │
│  └──────┬───────┘  └──────┬──────┘  └─────────────────────┘  │
│         └─────────────────┬────────────────┘                   │
│                           │                                    │
│         ┌─────────────────┴────────────────┐                   │
│         │   Services (Auth, Feed, Article, │                   │
│         │   Media, Search, Crawler)          │                   │
│         └─────────────────┬────────────────┘                   │
│                           │                                    │
│         ┌─────────────────┴────────────────┐                   │
│         │         sqlx + raw SQL           │                   │
│         └─────────────────┬────────────────┘                   │
└───────────────────────────┼─────────────────────────────────┘
                            │
                    ┌───────┴────────┐
                    │     SQLite     │
                    │  (single file: │
                    │ data/rustrss   │
                    │    srv.db)     │
                    └────────────────┘
```

### Components

| Component | Responsibility | Key crates |
|---|---|---|
| **Web server** | Serve TailAdmin-styled pages and embedded static assets | `axum`, `askama`, `rust-embed`, `tower-http` |
| **API server** | Versioned REST JSON API, OpenAPI via `utoipa` | `axum`, `utoipa`, `utoipa-swagger-ui`, `serde` |
| **Auth service** | Bcrypt passwords, JWT access/refresh tokens | `bcrypt`, `jsonwebtoken` |
| **Feed service** | Add/remove/discover/import/export subscriptions | `rss`, `atom_syndication`, `serde_json`, `quick-xml` |
| **Crawler** | Background polling, politeness, backoff, redirect handling, Cache-Control/Expires | `reqwest`, `tokio`, `tokio` interval |
| **Logging** | Rotating file logs and stdout with env-filter | `tracing`, `tracing-subscriber`, `tracing-appender` |
| **Content pipeline** | Normalize articles, clean HTML → Markdown, scrape origin fallback | `html-to-markdown-rs`, `readability-rs`, `comrak` (web) |
| **Media service** | Fetch, hash (BLAKE3), store BLOB, serve authenticated proxy | `blake3`, `reqwest`, `sqlx` |
| **Search service** | SQLite FTS5 full-text search over article titles, summary, and content | `sqlx` + FTS5 virtual table |

## 2. Database Schema

Uses SQLite with `INTEGER PRIMARY KEY` autoincrement, foreign keys enabled, WAL journal mode, and Unix millisecond timestamps stored as `INTEGER` in UTC.

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    default_filter TEXT NOT NULL DEFAULT 'unread'
        CHECK (default_filter IN ('all', 'unread', 'read', 'starred')),
    default_sort_order TEXT NOT NULL DEFAULT 'oldest_first'
        CHECK (default_sort_order IN ('oldest_first', 'newest_first')),
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE refresh_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE feeds (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    title TEXT,
    description TEXT,
    site_url TEXT,
    fetch_interval_minutes INTEGER NOT NULL DEFAULT 15
        CHECK (fetch_interval_minutes IN (5, 15, 30, 60, 120, 240, 720, 1440)),
    last_fetched_at INTEGER,
    next_fetch_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    cache_until INTEGER,
    last_etag TEXT,
    last_modified TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    backoff_until INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    UNIQUE (user_id, feed_id)
);

CREATE TABLE articles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    summary TEXT,
    raw_html TEXT,
    markdown_content TEXT NOT NULL,
    published_at INTEGER,
    fetched_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE article_feeds (
    article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    first_seen_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    PRIMARY KEY (article_id, feed_id)
);

CREATE TABLE read_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    is_read INTEGER NOT NULL DEFAULT 0,
    read_at INTEGER,
    is_starred INTEGER NOT NULL DEFAULT 0,
    starred_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    UNIQUE (user_id, article_id)
);

CREATE TABLE media (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_hash TEXT NOT NULL UNIQUE,
    origin_url TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    data BLOB NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE feed_fetch_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    fetched_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    status TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    error_message TEXT,
    articles_found INTEGER
);

-- Indexes
CREATE INDEX idx_feeds_next_fetch ON feeds(next_fetch_at);
CREATE INDEX idx_subscriptions_user ON subscriptions(user_id);
CREATE INDEX idx_subscriptions_feed ON subscriptions(feed_id);
CREATE INDEX idx_article_feeds_feed ON article_feeds(feed_id);
CREATE INDEX idx_read_states_user ON read_states(user_id);
CREATE INDEX idx_read_states_user_read ON read_states(user_id, is_read);
CREATE INDEX idx_articles_published ON articles(published_at);
CREATE INDEX idx_articles_fetched ON articles(fetched_at);
CREATE INDEX idx_articles_url ON articles(url);

-- Full-text search (FTS5)
CREATE VIRTUAL TABLE articles_fts USING fts5(
    title,
    summary,
    markdown_content,
    content='articles',
    content_rowid='id'
);

-- Triggers to keep FTS5 index in sync
CREATE TRIGGER articles_fts_insert AFTER INSERT ON articles BEGIN
    INSERT INTO articles_fts(rowid, title, summary, markdown_content)
    VALUES (new.id, new.title, new.summary, new.markdown_content);
END;

CREATE TRIGGER articles_fts_delete AFTER DELETE ON articles BEGIN
    INSERT INTO articles_fts(articles_fts, rowid, title, summary, markdown_content)
    VALUES ('delete', old.id, old.title, old.summary, old.markdown_content);
END;

CREATE TRIGGER articles_fts_update AFTER UPDATE ON articles BEGIN
    INSERT INTO articles_fts(articles_fts, rowid, title, summary, markdown_content)
    VALUES ('delete', old.id, old.title, old.summary, old.markdown_content);
    INSERT INTO articles_fts(rowid, title, summary, markdown_content)
    VALUES (new.id, new.title, new.summary, new.markdown_content);
END;
```

### Notes

- `articles.url` is unique globally so the same article is never stored twice.
- `article_feeds` records which feeds an article belongs to; a user sees an article if any of their subscribed feeds links to it.
- When an article is normalized for the API, the primary `feed_id`/`feed_title` are the first subscribed feed (by feed id) among the user's subscriptions that links to the article.
- Media is stored as SQLite BLOBs in the `media` table; deleting a `media` row removes the bytes immediately.
- Media deduplication uses `INSERT OR IGNORE` keyed by `content_hash`.
- Duplicate articles are upserted by URL using `INSERT ... ON CONFLICT(url) DO UPDATE`, updating content and `fetched_at` but preserving `published_at`.
- `Feed.status` in API responses is computed on read:
  - `backoff` if `backoff_until > current Unix millis`,
  - `error` if `consecutive_failures > 0`,
  - otherwise `ok`.
- Retention cleanup runs daily in three explicit phases:
  1. Delete non-starred read states older than 30 days for each user.
  2. Delete articles older than 30 days that have no starring read states.
  3. Delete `media` rows whose `content_hash` is no longer referenced by any article.

## 3. API Design

See `openapi.yaml` for the complete machine-readable spec.

### Authentication

- Access token: JWT, 7-day fixed lifetime, sent in `Authorization: Bearer <token>`.
- Refresh token: opaque random string hashed in `refresh_tokens`, 90-day fixed lifetime.
- `POST /api/v1/auth/login` returns both tokens.
- `POST /api/v1/auth/refresh` accepts a refresh token and returns a new access token.
- `POST /api/v1/auth/logout` revokes the supplied refresh token.

### Pagination

Cursor-based. Article/feed list responses include:

```json
{
  "items": [...],
  "next_cursor": 123,
  "prev_cursor": null,
  "has_more": true
}
```

`cursor` is the ID of the last item on the current page; the server orders by `published_at` then `id` and returns items after that cursor. `has_more` reflects `next_cursor` regardless of paging direction.

Article lists additionally accept `direction` (`next`, the default, or `prev`) to page backward from `cursor` using `prev_cursor`. Keyset pagination only — no offset/page-number support.

### Validation

- Passwords must be at least 8 characters and contain at least one uppercase letter, one lowercase letter, one digit, and one special (non-alphanumeric) character.
- Feed and website URLs must use the `http` or `https` scheme.
- OPML import files are limited to 5 MB.
- `fetch_interval_minutes` must be one of 5, 15, 30, 60, 120, 240, 720, 1440.
- All input is validated with `validator` before reaching the database.

### Article list query parameters

- `feed_id` — filter to one subscription.
- `is_read` — `true`/`false`.
- `is_starred` — `true`/`false`.
- `sort` — `oldest_first` (default) or `newest_first`.
- `cursor` / `limit` (default 20, max 100).
- `direction` — `next` (default) or `prev`.

### Web routes (server-side pages)

- `GET /setup` — first-user creation form (disabled once a user exists).
- `GET /login` — login page.
- `GET /` — dashboard / unified article stream.
- `GET /feeds` — subscription list.
- `GET /feeds/:id` — feed detail and its articles.
- `GET /articles` — article list with filter/sort and Previous/Next pagination.
- `GET /articles/:id` — article reader.
- `GET /search` — search results.
- `GET /settings` — user preferences.

Static assets are served under `/static/` from `rust-embed`.

### Health

- `GET /health` — returns `200 OK` when the server can query SQLite and the database file is writable.

## 4. Content & Media Pipeline

1. Fetch feed XML/JSON with `reqwest` (per-domain UA from a hardcoded pool of 5 browser strings selected by `crc32` of the domain, global 500 ms `Mutex<Instant>` throttle, 30-second timeout, follow redirects up to a limit, respect `Cache-Control`/`Expires` and `ETag`/`Last-Modified`, update stored URL on 301/308).
2. Parse with format-specific crate and normalize into internal `ArticleInput`. Article URLs are resolved relative to the feed/site URL and canonicalized before deduplication.
3. For each article:
   - If full HTML in feed, convert to Markdown with `html-to-markdown-rs`.
   - Else scrape origin with `reqwest` + `readability-rs` (10-second timeout, any `reqwest`-supported scheme), then convert.
   - On scrape failure, fall back to the feed-provided content. If `html-to-markdown-rs` fails or returns empty, store the raw HTML as Markdown.
4. Extract image/enclosure URLs from Markdown/HTML.
   - Fetch each media asset (30-second timeout, max 10 MB).
   - Compute BLAKE3 hash.
   - If ≤ 256 KB, base64-encode and inline in Markdown.
   - Else `INSERT OR IGNORE` the bytes into `media(data)` and rewrite Markdown reference to `/api/v1/media/<hash>`.
5. Upsert article by URL (`INSERT ... ON CONFLICT(url) DO UPDATE` updating content and `fetched_at`, preserving `published_at`), update `article_feeds` relation, and create per-user read state rows for current subscribers.
6. Web view renders the stored Markdown with `comrak`; API returns raw Markdown.

## 5. Security

- Bcrypt for passwords with cost factor 12.
- Passwords must be at least 8 characters and contain uppercase, lowercase, digit, and special characters.
- JWT claims: `sub` (user id), `email`, `iat`, `exp`, `typ: access`.
- JWT secret from environment; never committed.
- Refresh tokens are not rotated; the same token is valid for 90 days unless revoked.
- Logout revokes the supplied refresh token and blocklists the current access token until its natural expiration.
- Media proxy requires a valid access token.
- `html-to-markdown-rs` + `readability-rs` strip scripts and ads; no additional sanitizer per requirements.
- All user input validated with `validator` before touching the database.
- SQL injection prevented by `sqlx` compile-time checked queries.
- SQLite foreign keys and WAL mode are enabled globally on every connection.
- Web login form includes a hidden `timezone` field populated via `Intl.DateTimeFormat().resolvedOptions().timeZone`; API clients default to UTC.

## 6. Deployment

Self-hosted binary:
- Build the Rust binary with `cargo build --release` and run it directly.
- Serves HTTP on port `9119` by default.
- SQLite database file lives at `./data/rustrsssrv.db`; migrations are embedded with `sqlx::migrate!` and applied automatically on startup.
- `GET /health` verifies SQLite connectivity and file writability.
- `tracing` with env-filter and `tracing-appender` provides daily-rotating file logs (kept for 7 days) in `./logs` and stdout output.
- No Docker, Docker Compose, or containerization support is included in v1.

No `.env` file is committed; configuration is read from environment variables:
- `DATABASE_URL` — SQLite file path, default `sqlite:./data/rustrsssrv.db`
- `JWT_SECRET`
- `RUST_LOG` — default `warn`
- `PORT` — default `9119`
- `ENABLE_CRAWLER` — set to `true` to start the background crawler
- `LOG_DIR` — default `./logs`
