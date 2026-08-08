# Project Scope Review — Rust RSS Server

**Review date:** 2026-08-08  
**Scope state:** Documentation complete, no implementation artifacts yet (`Cargo.toml`, `src/`, `migrations/`, or Docker files are absent).  
**Sources reviewed:** `REQUIREMENTS.md`, `DESIGN.md`, `IMPLEMENTATION_PLAN.md`, `openapi.yaml`, `CLAUDE.md`.

## Bottom Line

The project has a clear, reasonably bounded v1 scope: a self-hosted RSS aggregator with PostgreSQL, Axum, Askama/TailAdmin web UI, OpenAPI-backed REST API, and an in-process feed crawler. The requirements are comprehensive enough to start implementation.

The main blockers before coding are a handful of **internal contradictions** and **underspecified behaviors** that will force decisions mid-implementation. Resolving them now avoids rework across `REQUIREMENTS.md`, `DESIGN.md`, `openapi.yaml`, and the implementation.

## 1. Current Scope Summary

### In Scope (v1)

- Multi-user accounts with email/password auth (bcrypt), setup wizard, no public registration.
- JWT access tokens (7 days) + opaque refresh tokens (90 days).
- Feed subscription by URL, OPML import/export, `<link rel="alternate">` discovery.
- Background crawler: 4 concurrent fetches, 15-minute default interval, preset intervals, exponential backoff, 500 ms politeness, redirect handling, per-domain UA.
- RSS 2.0, Atom, JSON Feed parsing with article normalization.
- Content cleaning: feed HTML → Markdown via `html-to-markdown-rs`; origin-page scrape fallback via `reqwest` + `readability-rs`; raw HTML stored alongside Markdown.
- Media proxy: fetch at ingest, BLAKE3 deduplication, ≤128 KB inlined as base64, larger assets stored as PostgreSQL large objects and served at `/api/v1/media/:hash`.
- PostgreSQL full-text search over titles/summary/content.
- Per-user read/star state; 30-day retention with starred articles exempted.
- Cursor pagination for articles/feeds.
- Server-side Askama templates + TailAdmin styling; static assets embedded via `rust-embed`.
- OpenAPI spec generated via `utoipa` and served under `/api-docs/openapi.json`.
- Docker Compose deployment.

### Explicitly Out of Scope

Feed publishing, OAuth/SSO, WebSub/PubSubHubbub, external search engines, GraphQL/gRPC, Fever/Google Reader API, notifications, offline support, Kubernetes/Helm, SaaS multi-tenancy.

### Ambiguous Territory

- Desktop (egui) and mobile (Flutter) clients are mentioned as planned clients in `REQUIREMENTS.md` §3.8 and as "out of v1 implementation scope but architected for" in §7. They are not in the API endpoint list and have no architecture in `DESIGN.md`. Recommendation: keep them out of v1 entirely and state the API is shaped for future clients, but do not imply they are part of this release.

## 2. Inconsistencies and Missing Specificity

### 2.1 Feed status field mismatch

- `openapi.yaml` `Feed.status` enum is `[ok, error, backoff]`.
- `DESIGN.md` schema has `consecutive_failures` and `backoff_until` but no `status` column.
- **Question:** Is `status` computed from `backoff_until` and `consecutive_failures` at query time, or should the schema add a `status TEXT` column? Either is valid, but the API contract and database schema must agree.

### 2.2 Article ownership in a multi-feed world

- `DESIGN.md` normalizes articles globally by URL and links them to feeds via `article_feeds`, so one article can belong to many feeds.
- `openapi.yaml` `Article` requires a single `feed_id` and `feed_title`.
- **Question:** For a unified article stream, which feed wins? The first subscribed feed? The one that most recently published it? The API should probably return an array of originating feed IDs/titles, or the implementation must define a deterministic primary feed.

### 2.3 Preset feed intervals are not enumerated

- `REQUIREMENTS.md` §3.3 and §3.10 say schedules are chosen from "preset intervals."
- `DESIGN.md` stores `fetch_interval_minutes INT DEFAULT 15` with no CHECK constraint or enum.
- **Question:** What are the allowed presets? (e.g. 5, 15, 30, 60, 120, 240, 720, 1440 minutes). This affects validation and the `FeedUpdate` schema.

### 2.4 Timezone auto-detection is unspecified

- `REQUIREMENTS.md` §3.11 says the user's timezone is "auto-detected from the browser on first login and can be overridden in settings."
- Neither `DESIGN.md` nor `openapi.yaml` describe how the browser timezone reaches the server. A hidden field on the web login form? A `timezone` parameter on the login request? A separate endpoint hit after login?
- **Question:** Choose and document the mechanism. If it is web-only, API-native clients (no browser) will default to UTC until the user updates settings.

### 2.5 Cache-Control / Expires handling

- `REQUIREMENTS.md` §3.10 says the crawler respects `Cache-Control` / `Expires`.
- `DESIGN.md` schema only tracks `last_etag` and `last_modified`.
- **Question:** Add a `cache_until TIMESTAMPTZ` or `min_fetch_interval` derived from headers, or drop Cache-Control from requirements. Note that a `Cache-Control: max-age=3600` from a feed could override the 15-minute schedule.

### 2.6 Politeness delay semantics

- "500 ms global politeness delay" is stated, but with 4 concurrent workers it is ambiguous whether the delay is per-worker or global across all outbound requests.
- **Question:** Define it as a global cross-worker throttle (e.g. `tokio::sync::Mutex<Instant>`) to avoid hammering a single domain from multiple workers.

### 2.7 User-agent rotation is not specified

- "rotating user-agent pool with a fixed UA assigned per domain" is mentioned but no pool size, source, or rotation strategy is defined.
- **Question:** How many UAs? Hardcoded list or configuration file? Does "fixed per domain" mean deterministic hash of domain → UA, or random assignment at first crawl?

### 2.8 Retention and media cleanup

- `DESIGN.md` says retention deletes non-starred read states and articles older than 30 days, but leaves media cleanup for a later job.
- **Decision:** Media cleanup runs during retention: delete media rows (and call `lo_unlink`) no longer referenced by any article.
- **Decision:** Retention is per-user. The article row is kept if any user has starred it, but only the starring users' read states are retained; non-starring users' read states are deleted.

### 2.9 Logging requirements have no design

- `REQUIREMENTS.md` §3.12 says logs go to rotating files and stdout as structured or text output.
- `DESIGN.md` does not mention logging, and `IMPLEMENTATION_PLAN.md` does not include it.
- **Question:** Is logging part of v1 or deferred? If part of v1, pick `tracing` + `tracing-subscriber` with env-filter and optionally `tracing-appender` for file rotation; if deferred, move it out of scope.

### 2.10 Validation rules are absent

- No minimum password length, email validation rules, URL schemes, or file-size limits for OPML import are documented.
- **Question:** Define minimum password length (e.g. 12), allowed URL schemes (http/https), and maximum OPML file size.

### 2.11 "Admin" terminology conflicts with single-role design

- `REQUIREMENTS.md` §3.1 says there is a single role in v1.
- `DESIGN.md` and `IMPLEMENTATION_PLAN.md` repeatedly refer to the "first admin" created by `/setup`.
- **Question:** If there is only one role, call the setup user the "first user" or "initial account" to avoid implying an admin dashboard or privilege model that does not exist.

### 2.12 Health/readiness endpoint for Docker Compose

- `REQUIREMENTS.md` §3.12 lists Docker Compose deployment.
- No health check endpoint is documented for the `app` service.
- **Question:** Add `GET /health` or `GET /ready` that confirms database connectivity before the compose stack marks the service healthy.

### 2.13 Default article filter behavior in API

- `REQUIREMENTS.md` §3.7 says the default article view filter is the user's last chosen setting (unread-only or all).
- `openapi.yaml` exposes `is_read` as a query filter but does not describe whether the server applies the user's default when the parameter is omitted.
- **Question:** When `is_read` is omitted, does the API apply the user's `default_filter`, or does it return all articles? The web UI likely wants the default applied; API clients may want explicit behavior.

### 2.14 Mark-read triggers

- `REQUIREMENTS.md` §3.7 says articles are marked read "when opened, when scrolled to the end, or by manual toggle."
- Only manual toggles (`POST /articles/:id/read`) are in the API.
- **Question:** Are "opened" and "scrolled to end" web-UI concerns that call the same `read` endpoint, or should the web UI auto-mark articles read on `GET /articles/:id`? Document the chosen behavior.

### 2.15 Media MIME type normalization

- `openapi.yaml` media endpoint returns `application/octet-stream`.
- `REQUIREMENTS.md` §3.5 says it returns the "stored/normalized MIME type."
- **Question:** How are MIME types determined and normalized? Sniffed from content, derived from filename/extension, or from the HTTP `Content-Type` of the origin fetch? Add a `mime_type` derivation rule.

## 3. Risks

1. **Content cleaning pipeline is large.** Stages 3 and 4 of `IMPLEMENTATION_PLAN.md` combine feed parsing, origin scraping, Markdown conversion, media extraction, large-object storage, and proxying. This is the highest-risk area for bugs and should be the focus of integration tests with mocked HTTP.
2. **Media large objects can bloat the database if cleanup is skipped.** The retention job must also delete unreferenced media rows and unlink large objects.
3. **OpenAPI/utoipa drift.** The hand-written `openapi.yaml` will be replaced by generated spec. The test that compares generated spec to expected contract (per `CLAUDE.md` testing requirements) must be written early.
4. **No existing project skeleton.** Every stage depends on the previous one; Stage 1 sets the pattern for handlers, services, state, errors, and tests. Getting Stage 1 right reduces rework.

## 4. User Stories / Acceptance Criteria

The existing endpoint list maps cleanly to the following acceptance criteria. Any scope change should update these.

### Auth & Setup

- **US-1:** Given an empty database, visiting `/setup` renders a form; submitting it creates the first user and redirects to `/login`.
- **US-2:** Given valid credentials, `POST /api/v1/auth/login` returns a JWT access token and opaque refresh token.
- **US-3:** Given a valid refresh token, `POST /api/v1/auth/refresh` returns a new access token.
- **US-4:** Given a refresh token, `POST /api/v1/auth/logout` revokes it so it cannot be reused.

### User Management

- **US-5:** `GET /api/v1/users/me` returns the current user's email, timezone, default filter, and default sort order.
- **US-6:** `PATCH /api/v1/users/me` updates email, password, timezone, default filter, or default sort order with input validation.
- **US-7:** `DELETE /api/v1/users/me` deletes the account, subscriptions, read states, refresh tokens, and logs the user out.

### Feeds

- **US-8:** `POST /api/v1/feeds` subscribes the current user to a feed URL after validating it is a fetchable feed.
- **US-9:** `POST /api/v1/feeds/discover` returns candidate feed URLs extracted from a given website's `<link rel="alternate">` tags.
- **US-10:** `GET /api/v1/feeds` returns the user's subscriptions with cursor pagination.
- **US-11:** `PATCH /api/v1/feeds/:id` updates only `fetch_interval_minutes` from an allowed preset list.
- **US-12:** `DELETE /api/v1/feeds/:id` unsubscribes the user from the feed without deleting the global feed row if other users subscribe.
- **US-13:** `POST /api/v1/feeds/import/opml` creates subscriptions for every valid feed in the uploaded OPML file and reports counts/errors.
- **US-14:** `GET /api/v1/feeds/export/opml` returns an OPML document containing the user's subscriptions.
- **US-15:** `POST /api/v1/feeds/:id/refresh` queues/triggers an immediate fetch of the feed.

### Articles & Reading State

- **US-16:** The crawler normalizes RSS/Atom/JSON feed entries into the `articles` table, de-duplicating by URL and updating existing rows when content changes.
- **US-17:** `GET /api/v1/articles` returns articles from the user's subscribed feeds, filtered by optional `feed_id`, `is_read`, and `is_starred`, sorted by `oldest_first` or `newest_first`.
- **US-18:** `GET /api/v1/articles/:id` returns a single article with raw Markdown content.
- **US-19:** `POST /api/v1/articles/:id/{read,unread,star,unstar}` updates the per-user read/star state idempotently.
- **US-20:** Web article pages (`/articles/:id`) render the stored Markdown to sanitized HTML using `comrak`.

### Search

- **US-21:** `GET /api/v1/search?q={query}` returns full-text search results across article titles and content.

### Media

- **US-22:** `GET /api/v1/media/:content_hash` streams the proxied media file with the stored MIME type to authenticated users.
- **US-23:** Small images (≤128 KB) are inlined as base64 in article Markdown; larger images are referenced by proxy URL.

### Deployment

- **US-24:** `docker compose up` boots PostgreSQL, applies migrations, starts the Rust server, and serves the application on the configured port.

## 5. Resolved Decisions

| # | Topic | Decision |
|---|-------|----------|
| 1 | Feed status | Computed on read from `consecutive_failures` and `backoff_until`. |
| 2 | Multi-feed articles | Single primary `feed_id`/`feed_title` per article in the API; use the first subscribed feed that linked to the article. |
| 3 | Preset intervals | 5, 15, 30, 60, 120, 240, 720, 1440 minutes. |
| 4 | Timezone capture | Hidden `timezone` field on the web login form, populated by JavaScript. API clients default to UTC until they update settings. |
| 5 | Cache-Control | Respect `Cache-Control`/`Expires` and delay the next fetch accordingly (add `cache_until` to schema). |
| 6 | Politeness delay | Global 500 ms throttle across all 4 workers. |
| 7 | User agents | Hardcoded list of browser UAs; assign one per domain deterministically by hashing the domain. |
| 8 | Media cleanup | Clean up unreferenced media during the retention job. |
| 9 | Retention scope | Per-user. Article row kept if any user starred it; only starring users' read states survive retention. |
| 10 | Logging | In v1: `tracing` + `tracing-subscriber` with env-filter and `tracing-appender` for rotating files plus stdout. |
| 11 | Validation | Password minimum 8 characters; feed/website URLs must use http/https; OPML import max 5 MB. |
| 12 | Admin terminology | Use "first user" / "initial account" instead of "admin". |
| 13 | Health endpoint | Add `GET /health` that verifies database connectivity. |
| 14 | Default filter | `GET /api/v1/articles` applies the user's `default_filter` when `is_read` is omitted. |
| 15 | Mark-read on open | Web detail page (`GET /articles/:id`) auto-marks the article read. |
| 16 | MIME type | Trust origin `Content-Type` header first, then fall back to extension, then `application/octet-stream`. |
| 17 | Desktop/mobile | Keep egui/Flutter as a forward-looking note only; no v1 architecture. |

## 6. Recommended Next Steps

1. ✅ Update `REQUIREMENTS.md` with the decisions above.
2. ✅ During `/sc:design`, update `DESIGN.md` schema (`cache_until`, feed status computation, media cleanup, retention, SQLite pivot, FTS5) and `openapi.yaml` (`Article.feed_id` as single primary feed, add `/health`).
3. Add `Cargo.toml` and the `src/` skeleton so `cargo check` passes before writing business logic.
4. Add the embedded `migrations/` directory matching the final SQLite schema.
5. Begin `IMPLEMENTATION_PLAN.md` Stage 1 once the docs are internally consistent.

## 7. Major Pivot Recorded

- **Database:** PostgreSQL → SQLite (single file).
- **Media storage:** PostgreSQL large objects → SQLite BLOBs.
- **Search:** PostgreSQL `tsvector` → SQLite FTS5 virtual table.
- **Migrations:** `sqlx-cli` → embedded `sqlx::migrate!`.
- **Docker Compose:** two services → single `app` service with data/log volumes.

Full details are in `DESIGN_DECISIONS.md`.
