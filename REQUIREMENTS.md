# Rust RSS Server — Requirements

Discovery date: 2026-08-08

## 1. Product Goal

Build a self-hosted RSS aggregation server in Rust with SQLite storage, a TailAdmin-styled web frontend using server-side templates, and a shared OpenAPI backend that can later support native desktop and mobile clients.

## 2. Core Role

**Feed aggregator with user accounts.** The server crawls external RSS/Atom/JSON feeds, normalizes articles on ingest, stores them, and exposes them through a REST API. Users manage their own subscriptions and reading state.

## 3. Functional Requirements

### 3.1 Users and Authentication
- Multi-user accounts.
- Email + password authentication with bcrypt; passwords must be at least 8 characters.
- First user is created through a web setup wizard on first boot; no public registration endpoint.
- Single user role in v1; no admin-vs-user feature split beyond registration control.
- API clients authenticate with short-lived access tokens (7 days) and longer-lived refresh tokens (90 days fixed) via the `Authorization` header and a token refresh endpoint.
- Users can read and update their own profile via `GET/PATCH /api/v1/users/me` and delete their account via `DELETE /api/v1/users/me`.

### 3.2 Feed Management
- Users add feeds by exact URL only; feed and website URLs must use http or https.
- OPML import and export; uploaded OPML files are limited to 5 MB.
- Feed URL auto-discovery from a website (`<link rel="alternate">` scraping).
- Per-user subscriptions are a flat list in v1; no folders or tags.

### 3.3 Feed Crawling
- Background periodic polling of all subscribed feeds.
- Manual refresh on demand.
- Adding a feed triggers an immediate fetch.
- Default polling interval: 15 minutes.
- Per-feed custom schedules chosen from preset intervals: 5, 15, 30, 60, 120, 240, 720, 1440 minutes.
- Feed fetches are normalized on ingest into a single article schema.
- Duplicate articles are identified globally by URL and updated when the feed republishes changes.
- Feeds that repeatedly fail use exponential backoff instead of being disabled or deleted.
- Support RSS 2.0, Atom, JSON Feed, and media enclosures.

### 3.4 Content Storage and Cleaning
- Store article content as cleaned Markdown.
- Store the original raw HTML alongside the cleaned Markdown.
- Convert feed-provided HTML to Markdown using `html-to-markdown-rs`.
- If the feed is truncated or only a summary is provided, scrape the origin page, convert it to Markdown, and store both raw and cleaned forms.
- If origin scraping fails, fall back to the feed-provided content (still converted/cleaned to Markdown).
- Conversion through `html-to-markdown-rs` and `readability-rs` is trusted to strip scripts, ads, and tracking; no additional HTML sanitizer is used before storage.
- Retention window of 30 days. Non-starred read states and articles older than 30 days are deleted. Articles starred by any user are kept, but only the starring users' read states are retained. The retention job also deletes unreferenced media BLOBs in three explicit phases.

### 3.5 Media Proxying
- All images referenced in article HTML and all feed enclosures (audio, video, etc.) are fetched at ingest and served by the RSS server.
- Media files are stored as SQLite BLOBs.
- Media is deduplicated by BLAKE3 content hash to avoid storing identical files multiple times.
- Media proxy endpoint is `/api/v1/media/{content_hash}` and returns the raw byte stream with the stored/normalized MIME type.
- Media MIME type is determined from the origin HTTP `Content-Type` header, falling back to extension-based guessing, then `application/octet-stream`.
- Media proxy endpoints require authentication.
- Small media assets (≤ 256 KB) are inlined as base64 data URIs inside the cleaned Markdown.
- Larger media is referenced by server proxy URLs in the Markdown.

### 3.6 Search
- SQLite FTS5 full-text search across article titles, summary, and content.

### 3.7 Reading State
- Per-user read/unread status.
- Articles are marked read when opened in the web UI, when scrolled to the end, or by manual toggle.
- Starred articles preserved beyond the retention window.
- Default article view filter is the user's last chosen setting (all, unread, read, or starred); `GET /api/v1/articles` applies this default when `is_read` and `is_starred` are not supplied.

### 3.8 API and Clients
- REST JSON API documented with OpenAPI; URL versioned under `/api/v1/`.
- OpenAPI spec is the contract for all clients.
- Web frontend uses server-side Askama templates styled with TailAdmin; static assets embedded via `rust-embed`.
- Web view renders Markdown to sanitized HTML server-side using `comrak`.
- API returns raw Markdown for native/desktop/mobile clients.
- Native desktop client (egui) and mobile client (Flutter) are planned future clients; v1 designs the API to support them but does not implement them.
- No offline support for clients in the first release; always-online reading.
- Article and feed lists use cursor pagination.
- Default article sort is oldest first; user can switch to newest first for the entire article view.
- Main reading view supports both unified stream and per-feed views.

### 3.9 Notifications
- No notifications in the first release.

### 3.10 Background Crawler
- In-process Tokio worker pool polling feeds.
- 4 concurrent feed fetches.
- Default polling interval: 15 minutes.
- Per-feed custom schedules chosen from preset intervals: 5, 15, 30, 60, 120, 240, 720, 1440 minutes.
- Exponential backoff on repeated failures.
- Respects HTTP `Cache-Control` / `Expires` headers from feed servers and delays the next fetch accordingly.
- Follows redirects up to a limit; updates the stored feed URL on permanent redirects (301/308).
- Uses a global fixed 500 ms delay between feed requests for politeness across all crawler workers.
- Uses a hardcoded rotating user-agent pool; a single UA is assigned to each domain deterministically by hashing the domain.

### 3.11 Timezones
- All dates stored in UTC.
- Web view displays times in the user's timezone.
- User timezone is auto-detected from the browser on first login via a hidden `timezone` field on the web login form and can be overridden in settings; API clients default to UTC until updated.

### 3.12 Deployment
- Self-hosted binary; no Docker or containerization support in v1.
- SQLite database file lives at `./data/rustrsssrv.db` by default.
- `GET /health` endpoint verifies SQLite connectivity and file writability.
- Logs are written to rotating files and stdout using `tracing` with env-filter and `tracing-appender`.

## 4. Non-Functional Requirements

- **Language**: Rust backend; HTTP framework Axum.
- **Database**: SQLite (single file at `./data/rustrsssrv.db`); migrations via embedded `sqlx::migrate!` macro with raw SQL.
- **Frontend template**: TailAdmin free Tailwind dashboard template used with Askama server-side templates.
- **Feed parsing**: `rss` crate plus additional crates for Atom/JSON Feed as needed.
- **HTML-to-Markdown**: `html-to-markdown-rs` for cleaning feed HTML into Markdown.
- **Origin content extraction**: `reqwest` + `readability-rs` for origin-page scraping.
- **Markdown rendering for web**: `comrak`.
- **Media storage**: Proxied images and enclosures stored as SQLite BLOBs, deduplicated by BLAKE3 content hash.
- **Search**: SQLite FTS5 virtual table with triggers keeping it in sync with `articles`.
- **OpenAPI spec**: generated via `utoipa` derive macros.
- **Self-contained binary**: Askama templates and TailAdmin static assets bundled into the Rust binary.
- **Deterministic tests** for feed parsing, API behavior, and retention logic.
- **Persistence tests** exercise a real on-disk SQLite file; in-memory SQLite alone does not model the actual persistence layer.
- **Security**: fail-fast validation, no silent error swallowing, secrets managed via environment variables, no committed `.env` files.

## 5. Out of Scope (First Release)

- Feed publishing/hosting.
- OAuth2 or enterprise SSO.
- WebSub/PubSubHubbub push updates.
- Dedicated search engine (Meilisearch/Elasticsearch).
- GraphQL or gRPC APIs.
- Fever/Google Reader API compatibility.
- Notifications (email, web push, mobile push).
- Offline support in clients.
- Kubernetes/Helm or multi-tenant SaaS deployment.

## 6. API Endpoints (First Release)

Authentication:
- `POST /api/v1/auth/login` — obtain access and refresh tokens.
- `POST /api/v1/auth/refresh` — obtain a new access token with a valid refresh token.
- `POST /api/v1/auth/logout` — revoke the current refresh token.

Users:
- `GET /api/v1/users/me` — get current user profile and settings.
- `PATCH /api/v1/users/me` — update profile, password, timezone, and UI preferences.
- `DELETE /api/v1/users/me` — delete the current user account.

Feeds:
- `GET /api/v1/feeds` — list the current user's subscribed feeds.
- `POST /api/v1/feeds` — subscribe to a feed by URL.
- `POST /api/v1/feeds/discover` — auto-discover feed URLs for a website.
- `GET /api/v1/feeds/:id` — get feed details.
- `PATCH /api/v1/feeds/:id` — update feed schedule and per-feed settings.
- `DELETE /api/v1/feeds/:id` — unsubscribe from a feed.
- `POST /api/v1/feeds/:id/refresh` — manually refresh a feed.
- `POST /api/v1/feeds/import/opml` — import subscriptions from OPML.
- `GET /api/v1/feeds/export/opml` — export subscriptions to OPML.

Articles:
- `GET /api/v1/articles` — list articles across the user's subscribed feeds.
- `GET /api/v1/articles/:id` — get a single article.
- `POST /api/v1/articles/:id/read` — mark article read.
- `POST /api/v1/articles/:id/unread` — mark article unread.
- `POST /api/v1/articles/:id/star` — star article.
- `POST /api/v1/articles/:id/unstar` — unstar article.

Search:
- `GET /api/v1/search?q={query}` — full-text search across article titles and content.

Media:
- `GET /api/v1/media/:content_hash` — stream a proxied media file (authenticated).

Web setup:
- A server-side setup page (`/setup`) creates the first user when the database has no users.

Health:
- `GET /health` — liveness/readiness check that verifies database connectivity.

## 7. Handoff to Design Phase

The requirements decisions above are now stable. Remaining work for `/sc:design`:

- Database schema for normalized articles, per-user subscriptions, read state, media BLOBs, feed health/backoff state, `cache_until`, and FTS5 virtual table.
- Full OpenAPI request/response schemas and pagination cursor format for the confirmed endpoints, including `Article` with a single primary `feed_id` and the `GET /health` endpoint.
- Askama template set and TailAdmin static asset embedding details.
- Future egui/Flutter client structure is out of v1 implementation scope; only note that the API is shaped for future clients.
