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
# Never run the ~/.local/bin/muse launcher: it can auto-update Muse and delete the
# previous binary (ADR 0002). Report the native binary only.
muse_bin=$(ls -1 "$HOME"/.local/bin/muse-bin-* 2>/dev/null | sort -V | tail -1 || true)
if [ -n "$muse_bin" ] && [ -x "$muse_bin" ]; then
  printf 'Muse native binary: %s\n' "$(basename "$muse_bin")"
  version "$muse_bin" --version
else
  printf 'Muse: native binary not found\n'
fi
if [ -x native/macos-observer/.build/release/macroscope-observer ]; then
  printf 'Show observer: built\n'
else
  printf 'Show observer: not built (cd native/macos-observer && swift build -c release)\n'
fi

if command -v node >/dev/null 2>&1; then
  node -e 'const fs = require("node:fs"); const manifest = JSON.parse(fs.readFileSync("plan/reference/msp/manifest.json", "utf8")); console.log("Plan-pinned MSP fingerprint: " + manifest.fingerprint + " (newer fingerprints pass an echo conformance check; see contracts/msp/README.md)");'
else
  printf 'Expected MSP fingerprint: unavailable (Node required to read manifest)\n'
fi
