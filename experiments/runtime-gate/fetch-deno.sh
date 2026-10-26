#!/bin/bash
# Pin deliberately. Updating requires review of the lock and a new full canary run.
set -euo pipefail
cd "$(dirname "$0")"
exec python3 fetch_deno.py
