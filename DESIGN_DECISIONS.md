# Design Decisions — Rust RSS Server

**Document date:** 2026-08-08  
**Status:** Locked for v1 implementation.  
**Sources:** Answers from `/sc:brainstorm` and `/sc:design` multiple-choice sessions, applied to `REQUIREMENTS.md`, `DESIGN.md`, and `openapi.yaml`.

## Architecture Pivot

- **Database switched from PostgreSQL to SQLite** (single file at `./data/rustrsssrv.db`).
- Driver: `sqlx` with SQLite.
- Migrations: embedded via `sqlx::migrate!` and applied on startup.
- Media storage: SQLite `BLOB` column in `media` table, deduplicated by BLAKE3 hash.
- Full-text search: SQLite FTS5 virtual table (`articles_fts`) synchronized with triggers.
- Docker Compose: single `app` service with a named volume for `./data` and `./logs`; no separate database service.

## Core Implementation Choices

| # | Topic | Decision |
|---|-------|----------|
| 1 | Rust edition | 2024 edition. |
| 2 | Configuration | `dotenvy` + environment variables. |
| 3 | Error response format | Structured JSON with `error`, `message`, and optional `details`. |
| 4 | Database pool | Single shared `SqlitePool` in `AppState`, default size, WAL mode, foreign keys enabled. |
| 5 | Password hashing | Bcrypt cost factor 12. |
| 6 | Refresh token rotation | Do not rotate; same token valid for 90 days unless revoked. |
| 7 | Integration test DB | Temporary SQLite file per test. |
| 8 | Module layout | Flat `src/handlers/`, `src/services/`, `src/models/`, plus `src/state.rs`, `src/errors.rs`. |
| 9 | Web authentication | Same as API: `Authorization: Bearer` header. No cookie. |
| 10 | Askama templates | `src/templates/` directory. |
| 11 | TailAdmin assets | npm/Tailwind build producing `assets/`, embedded via `rust-embed` with a `build.rs` step. |
| 12 | Crawler scheduler | `tokio::time::interval` waking every minute to queue due feeds. |
| 13 | Crawler workers | `tokio::sync::mpsc` bounded channel with 4 consumer workers. |
| 14 | Crawler start | Opt-in via `ENABLE_CRAWLER=true` environment variable. |
| 15 | Retention job | Daily, inside the crawler scheduler, in three explicit phases. |
| 16 | Feed discovery | Validate each candidate by fetching and parsing it; return parsed metadata. |
| 17 | Missing publish date | Fallback to `fetched_at` for ordering. |
| 18 | Migrations | Embedded `sqlx::migrate!` macro. |
| 19 | Media deduplication | `INSERT OR IGNORE` by `content_hash`. |
| 20 | Article deduplication | `INSERT ... ON CONFLICT(url) DO UPDATE`, updating content and `fetched_at`, preserving `published_at`. |
| 21 | FTS5 sync | SQLite triggers on `articles` for insert/update/delete. |
| 22 | Feed due query | Select feeds where `next_fetch_at <= now` and (`cache_until` is null or expired). |
| 23 | OPML import | Per-feed independent processing; response includes per-feed results. |
| 24 | Read state creation | Create rows for all current subscribers on article ingest. |
| 25 | Default sort order | `oldest_first`. |
| 26 | Web mark-read | Client-side POST to `/api/v1/articles/:id/read` after page load. |
| 27 | Setup detection | `SELECT COUNT(*) FROM users` on every `/setup` request. |
| 28 | JWT claims | `sub`, `email`, `iat`, `exp`, `typ: access`. |
| 29 | Refresh request body | Refresh token body only. |
| 30 | Logout | Revoke refresh token and blocklist access token until expiration. |
| 31 | Password change | Requires `current_password` and `new_password`; revokes all refresh tokens for the user. |
| 32 | Feed subscribe | Asynchronous fetch via crawler; return subscription immediately. |
| 33 | Manual refresh | Queue and return `202 Accepted`. |
| 34 | Default port | `9119`. |
| 35 | Request IDs | Generate a UUID request ID per HTTP request and include in tracing spans. |
| 36 | OpenAPI | `utoipa` generated spec is authoritative; `openapi.yaml` becomes a reference snapshot only. |
| 37 | Swagger UI | Serve at `/api-docs` using `utoipa-swagger-ui`. |
| 38 | Release profile | Thin LTO, 1 codegen unit. |
| 39 | Dependencies | Caret requirements + committed `Cargo.lock`. |
| 40 | Log rotation | Daily rotation, keep 7 days, human-readable text format, default level `warn`, directory `./logs`. |
| 41 | Database file | `./data/rustrsssrv.db`. |
| 42 | Crawler shutdown | Drain in-flight jobs with a timeout before exiting. |
| 43 | Feed title fallback | URL host if no feed title. |
| 44 | Article title | Required; fallback to URL path segment if missing. |
| 45 | Feed URL normalization | Store exactly as provided. |
| 46 | Article URL normalization | Resolve relative URLs and canonicalize before deduplication. |
| 47 | HTML-to-Markdown failure | Store raw HTML as fallback Markdown. |
| 48 | Origin scrape schemes | Any `reqwest`-supported scheme. |
| 49 | Origin scrape timeout | 10 seconds. |
| 50 | Feed fetch timeout | 30 seconds. |
| 51 | Media fetch timeout | 30 seconds. |
| 52 | Media max size | 10 MB. |
| 53 | User-agent pool | 5 hardcoded browser strings; domain assignment by `crc32` of domain. |
| 54 | Politeness throttle | `tokio::sync::Mutex<Instant>` shared across workers. |
| 55 | Backoff formula | `min(2^failures * 5 minutes, 24 hours)`. |
| 56 | Permanent redirects | Update stored feed URL on 301/308. |
| 57 | Origin fallback | Always fall back to feed-provided content on scrape failure. |
| 58 | Media inline threshold | 256 KB. |
| 59 | Media BLOB serving | Stream from SQLite in chunks. |
| 60 | Timezone capture | `Intl.DateTimeFormat().resolvedOptions().timeZone` in hidden login form field. |
| 61 | User delete scope | Delete user, subscriptions, read states, refresh tokens; keep shared articles/media. |
| 62 | Article summary | Feed-provided summary, falling back to a Markdown excerpt if absent. |
| 63 | Duplicate update fields | Content and `fetched_at` only; `published_at` preserved. |
| 64 | Crawler failure definition | HTTP/parse failures only; empty feeds are not failures. |
| 65 | Search query syntax | Raw FTS5 `MATCH` syntax passed through. |
| 66 | Search result order | FTS5 BM25 rank. |
| 67 | Web nav active state | Manual path matching in Askama templates. |
| 68 | Asset build | npm build produces `assets/`, `build.rs` embeds via `rust-embed`. |
| 69 | API version prefix | `/api/v1`. |
| 70 | Validation crate | `validator`. |
| 71 | Password complexity | Minimum 8 chars + uppercase + lowercase + digit + special (any non-alphanumeric). |
| 72 | Email uniqueness on update | Reject duplicate email with 409. |
| 73 | OPML response | Per-feed results including URL, title, and success/failure status. |
| 74 | Web unauthenticated | Redirect to `/login?next=<path>`. |
| 75 | API 401 | Return JSON error with `WWW-Authenticate: Bearer` header. |
| 76 | Setup form validation | Server-side + password confirmation field. |
| 77 | Feed discovery schemes | `http`/`https` only. |

## Notes

- Any decision not listed here falls back to the existing `REQUIREMENTS.md` / `DESIGN.md` / `CLAUDE.md` instructions.
- If a requirement conflicts with a decision above, this document and `REQUIREMENTS.md` take precedence for v1.
