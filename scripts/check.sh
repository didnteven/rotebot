#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
# tauri-build needs the externalBin input even for Clippy and workspace tests.
TAURI_ENV_DEBUG=true node apps/desktop/scripts/prepare-runner.mjs
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Program/runtime tests need the runner in its synthetic test mode; run them explicitly.
cargo test -p macroscope-app-core --features synthetic-runtime-tests
npm run --workspaces --if-present typecheck
npm run --workspaces --if-present test -- --run
npm run -w apps/desktop build
