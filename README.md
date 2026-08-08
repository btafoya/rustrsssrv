# Rust RSS Server

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.88+-orange.svg)](https://www.rust-lang.org)
[![GitHub Repo](https://img.shields.io/badge/GitHub-btafoya%2Frustrsssrv-blue.svg)](https://github.com/btafoya/rustrsssrv)

A self-hosted RSS aggregation server for keeping up with the web on your own terms.

## What It Is

Rust RSS Server is a personal feed aggregator that fetches, stores, and serves RSS, Atom, and JSON feeds through a web interface and a REST API. It is designed for people who want to own their reading data instead of relying on cloud-based services.

- Subscribe to feeds by URL or let it discover them from a website
- Read articles in a clean, web-based interface
- Search across everything you follow
- Star articles to keep them around
- Import and export subscriptions with OPML

## Requirements

- [Rust](https://www.rust-lang.org) 1.88 or later
- [Node.js](https://nodejs.org) and npm (for Tailwind CSS asset generation)
- A free port for the HTTP server (default `9119`)

## Installation

1. Clone the repository:

```bash
git clone https://github.com/btafoya/rustrsssrv.git
cd rustrsssrv
```

2. Install npm dependencies and build everything:

```bash
npm install
npm run build:all
```

This compiles the Tailwind CSS assets and then builds the release binary.

The compiled binary will be at `target/release/rustrsssrv`.

## Configuration

Configuration is read from environment variables. An optional `.env` file in the project root is also supported.

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:./data/rustrsssrv.db` | SQLite database path |
| `JWT_SECRET` | `dev-secret-do-not-use-in-production` | Secret used to sign JWT tokens. **Change this in production.** |
| `PORT` | `9119` | HTTP server port |
| `ENABLE_CRAWLER` | `false` | Set to `true` to run the background feed crawler |
| `LOG_DIR` | `./logs` | Directory for rolling log files |
| `RUST_LOG` | `warn` | Log level filter, e.g. `info`, `debug` |

Example `.env` file:

```env
DATABASE_URL=sqlite:./data/rustrsssrv.db
JWT_SECRET=change-me-to-a-long-random-string
PORT=9119
ENABLE_CRAWLER=true
LOG_DIR=./logs
RUST_LOG=info
```

Generate a strong `JWT_SECRET` before running in production:

```bash
openssl rand -hex 32
```

Copy the output into your `.env` file or pass it as the `JWT_SECRET` environment variable.

## Running the Server

Start the server:

```bash
./target/release/rustrsssrv
```

Or with explicit environment variables:

```bash
DATABASE_URL=sqlite:./data/rustrsssrv.db \
JWT_SECRET=change-me \
ENABLE_CRAWLER=true \
./target/release/rustrsssrv
```

You should see a log line showing the bound address, for example:

```text
listening on 0.0.0.0:9119
```

## First-Time Setup

When the server has no users, visiting `/` redirects to the setup wizard at `/setup`. Create the first admin account there, then log in at `/login`.

Once logged in, the web UI provides:

- **Dashboard** (`/`) — unread counts and recent articles
- **Articles** (`/articles`) — read, filter, and paginate articles
- **Feeds** (`/feeds`) — manage subscriptions
- **Search** (`/search`) — full-text search across subscribed articles
- **Settings** (`/settings`) — update email, timezone, default filter, and default sort order

## Using the API

The REST API is available under `/api/v1` and documented interactively at `/api-docs`.

Common workflows:

```bash
# Log in
TOKEN=$(curl -s -X POST http://localhost:9119/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"Password123!"}' \
  | jq -r '.access_token')

# Subscribe to a feed
curl -X POST http://localhost:9119/api/v1/feeds \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/feed.xml"}'

# List articles
curl http://localhost:9119/api/v1/articles \
  -H "Authorization: Bearer $TOKEN" | jq

# Mark an article read
curl -X POST http://localhost:9119/api/v1/articles/1/read \
  -H "Authorization: Bearer $TOKEN"

# Search
curl "http://localhost:9119/api/v1/search?q=rust&limit=10" \
  -H "Authorization: Bearer $TOKEN" | jq
```

## Background Crawler

The in-process crawler polls subscribed feeds every 15 minutes by default. Feed-specific intervals can be set to `5`, `15`, `30`, `60`, `120`, `240`, `720`, or `1440` minutes. Enable the crawler with `ENABLE_CRAWLER=true`. Without it, the server serves stored articles but does not fetch new content.

## Development

Install npm dependencies and build assets before running or testing:

```bash
npm install
npm run build
```

Or build both the assets and the release binary at once with `npm run build:all`.

Run the test suite:

```bash
export DATABASE_URL=sqlite:./data/rustrsssrv.db
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run
```

Regenerate SQLx offline query data after any schema or query change:

```bash
export DATABASE_URL=sqlite:./data/rustrsssrv.db
cargo sqlx prepare -- --all-targets
```

## Who Made This

Built by [Brian Tafoya](https://briantafoya.com).

## License

Released under the MIT License. See [LICENSE](LICENSE) for details.
