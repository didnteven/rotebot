#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --locked -p macroscope-runner --bins
python3 experiments/runtime-gate-v2/run_canaries.py "$@"
