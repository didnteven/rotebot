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

for file in "$runner" "$profile"; do
  if [[ ! -f "$file" ]]; then
    echo "FAIL: bundle is missing $file" >&2
    exit 1
  fi
done
if [[ ! -x "$runner" ]]; then
  echo "FAIL: bundle runner is not executable: $runner" >&2
  exit 1
fi
if ! cmp -s "$profile" "$workspace/crates/runner/profile.sb"; then
  echo "FAIL: bundled sandbox profile differs from crates/runner/profile.sb" >&2
  exit 1
fi

echo "PASS: sandbox runner and byte-identical profile present in $1"
ls -l "$macos"
