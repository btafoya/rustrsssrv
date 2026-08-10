#!/usr/bin/env bash
# Cuts a release: bumps Cargo.toml version, runs the quality gate, commits,
# tags, and pushes — which triggers .github/workflows/release.yml.
# Usage: npm run release -- 0.2.0
set -euo pipefail

VERSION="${1:?usage: npm run release -- X.Y.Z}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "version must be X.Y.Z (got: $VERSION)" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$REPO_ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
    echo "working tree not clean; commit or stash first" >&2
    exit 1
fi

export DATABASE_URL="${DATABASE_URL:-sqlite:./data/rustrsssrv.db}"
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run

sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
cargo check --quiet

git add Cargo.toml Cargo.lock
git commit -m "Release v$VERSION"
git tag -a "v$VERSION" -m "v$VERSION"
git push origin main
git push origin "v$VERSION"

echo "pushed v$VERSION; release workflow: https://github.com/btafoya/rustrsssrv/actions/workflows/release.yml"
