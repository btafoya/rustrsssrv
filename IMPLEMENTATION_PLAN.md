# Implementation Plan

Stages for building the Rust RSS server. Each stage must compile and pass its tests before moving on.

## Stage 1: Project Skeleton, Database, Auth

**Goal**: A runnable Axum server connected to SQlite with user accounts, setup wizard, login/refresh/logout, and the `/users/me` endpoints.

**Success Criteria**:
- `cargo build` succeeds.
- `sqlx migrate run` applies the schema from `DESIGN.md`.
- `GET /setup` renders when no users exist and creates the first admin.
- `POST /api/v1/auth/login` returns access and refresh tokens.
- `POST /api/v1/auth/refresh` returns a new access token.
- `POST /api/v1/auth/logout` revokes the refresh token.
- `GET/PATCH/DELETE /api/v1/users/me` work for authenticated users.

**Tests**:
- Integration test: setup wizard flow.
- Integration test: login with valid/invalid credentials.
- Integration test: refresh and logout token lifecycle.

**Status**: Complete

## Stage 2: Feed Management

**Goal**: Users can subscribe to feeds, discover feeds from websites, import/export OPML, and refresh feeds manually.

**Success Criteria**:
- `POST /api/v1/feeds` subscribes to a feed by URL.
- `POST /api/v1/feeds/discover` returns candidate feed URLs for a website.
- `GET /api/v1/feeds` lists the current user's subscriptions with cursor pagination.
- `PATCH /api/v1/feeds/:id` updates the feed schedule.
- `DELETE /api/v1/feeds/:id` unsubscribes.
- `POST /api/v1/feeds/import/opml` and `GET /api/v1/feeds/export/opml` round-trip subscriptions.

**Tests**:
- Add, list, patch, delete feed subscriptions.
- Discover feeds on a small local HTML page.
- Import and export a known OPML file.

**Status**: Complete

## Stage 3: Background Crawler and Article Ingestion

**Goal**: The server polls subscribed feeds, normalizes articles, and stores them with the correct per-user read state.

**Success Criteria**:
- In-process crawler runs on a Tokio worker pool with 4 concurrent fetches.
- Feeds are polled on a 15-minute default interval with preset per-feed overrides.
- Adding a feed triggers an immediate fetch.
- Articles are normalized on ingest and de-duplicated globally by URL.
- `GET /api/v1/articles` lists articles across the user's subscriptions with cursor pagination.
- `GET /api/v1/articles/:id` returns a single article.

**Tests**:
- Fetch a local RSS/Atom/JSON feed and verify stored article fields.
- Verify duplicate articles by URL update the existing row instead of creating a new one.
- Verify a user only sees articles from feeds they subscribe to.

**Status**: Complete

## Stage 4: Content Cleaning and Media Proxy

**Goal**: Feed and origin HTML are cleaned to Markdown, media is proxied and served from SQLite BLOBs, and the web view renders clean HTML.

**Success Criteria**:
- Feed-provided HTML is converted to Markdown with `html-to-markdown-rs`.
- Truncated feeds trigger origin scraping with `reqwest` + `readability-rs` and fall back to feed content on failure.
- Original raw HTML and cleaned Markdown are both stored.
- Images/enclosures ≤ 128 KB are inlined as base64 in Markdown.
- Larger media is fetched at ingest, stored as SQLite BLOBs, deduplicated by BLAKE3, and served at `/api/v1/media/:hash`.
- Web article pages render Markdown to sanitized HTML with `comrak`.
- The API returns raw Markdown for articles.

**Tests**:
- Convert a sample RSS article to Markdown and assert no scripts/ads remain.
- Scrape a local truncated feed and verify fallback behavior.
- Fetch a media asset through the proxy and verify the correct MIME type and hash.
- Verify base64 inlining for small images and proxy URLs for large images.

**Status**: Complete

## Stage 5: Reading State, Search, Web UI, and Polish

**Goal**: Users can read/star articles, search, use the TailAdmin-styled web UI, and the deployment works end-to-end.

**Success Criteria**:
- `POST /api/v1/articles/:id/{read,unread,star,unstar}` update per-user state.
- `GET /api/v1/search?q=...` returns full-text search results.
- Web pages (`/`, `/feeds`, `/articles/:id`, `/search`, `/settings`) render with TailAdmin styling.
- User timezone and default filter/sort preferences persist and apply to the web view.
- All existing tests pass and `cargo clippy` / `cargo fmt` are clean.

**Tests**:
- Mark articles read/unread/starred and verify filter behavior.
- Search for a word known to exist in an article title and content.
- End-to-end smoke test: setup wizard, add feed, fetch articles, render article page.

**Status**: Complete
