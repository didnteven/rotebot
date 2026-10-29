#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || "$1" != *.app || ! -d "$1" ]]; then
  echo "Usage: $0 <path-to-.app>" >&2
  exit 1
fi

workspace="$(cd "$(dirname "$0")/../../.." && pwd)"
macos="$1/Contents/MacOS"
runner="$macos/macroscope-runner"
profile="$macos/macroscope-runner.sb"
capture="$macos/macroscope-capture"

for file in "$runner" "$profile" "$capture"; do
  if [[ ! -f "$file" ]]; then
    echo "FAIL: bundle is missing $file" >&2
    exit 1
  fi
done
if [[ ! -x "$runner" || ! -x "$capture" ]]; then
  echo "FAIL: bundle helper is not executable: $runner" >&2
  exit 1
fi
if ! cmp -s "$profile" "$workspace/crates/runner/profile.sb"; then
  echo "FAIL: bundled sandbox profile differs from crates/runner/profile.sb" >&2
  exit 1
fi

echo "PASS: capture helper, sandbox runner and byte-identical profile present in $1"
ls -l "$macos"
