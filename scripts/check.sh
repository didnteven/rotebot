#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run --workspaces --if-present typecheck
npm run --workspaces --if-present test -- --run
npm run -w apps/desktop build
