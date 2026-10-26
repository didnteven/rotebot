#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

version() {
  local tool="$1"
  shift
  if command -v "$tool" >/dev/null 2>&1; then
    "$tool" "$@" 2>&1 || printf '%s version unavailable\n' "$tool"
  else
    printf '%s: not installed\n' "$tool"
  fi
}

version cargo --version
version rustc --version
version node --version
version npm --version
version swift --version
version xcodebuild -version
if command -v muse >/dev/null 2>&1; then
  printf 'Muse: present\n'
  version muse --version
else
  printf 'Muse: not installed\n'
fi

if command -v node >/dev/null 2>&1; then
  node -e 'const fs = require("node:fs"); const manifest = JSON.parse(fs.readFileSync("plan/reference/msp/manifest.json", "utf8")); console.log("Expected MSP fingerprint: " + manifest.fingerprint);'
else
  printf 'Expected MSP fingerprint: unavailable (Node required to read manifest)\n'
fi
