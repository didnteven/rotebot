#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
# tauri-build needs the externalBin inputs even for Clippy and workspace tests.
# The capture helpers are macOS only; the script does nothing elsewhere.
TAURI_ENV_DEBUG=true node apps/desktop/scripts/prepare-runner.mjs
TAURI_ENV_DEBUG=true node apps/desktop/scripts/prepare-capture.mjs
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Program/runtime tests need the runner in its synthetic test mode; run them explicitly.
cargo test -p macroscope-app-core --features synthetic-runtime-tests
npm run --workspaces --if-present typecheck
npm run --workspaces --if-present test -- --run
npm run -w apps/desktop build
