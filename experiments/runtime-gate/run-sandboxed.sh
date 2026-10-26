#!/bin/bash
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo 'usage: run-sandboxed.sh <package-dir> <entry.ts>' >&2
  exit 64
fi
exec python3 "$(dirname "$0")/launcher.py" "$1" "$2"
