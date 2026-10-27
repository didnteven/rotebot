#!/usr/bin/env bash
# Build Macroscope.app for this Mac (unsigned, for local use) with the Show observer
# inside the bundle, where the app looks for it (Contents/MacOS/macroscope-observer).
# Usage: scripts/bundle-mac.sh [--release]
set -euo pipefail
cd "$(dirname "$0")/.."
profile_flag="--debug"
profile_dir="debug"
if [ "${1:-}" = "--release" ]; then profile_flag=""; profile_dir="release"; fi

(cd native/macos-observer && swift build -c release)
if [ "$profile_dir" = "release" ]; then
  cargo build --locked -p macroscope-runner --bins --release
else
  cargo build --locked -p macroscope-runner --bins
fi
(cd apps/desktop && npx tauri build $profile_flag --bundles app)

target_dir="${CARGO_TARGET_DIR:-target}"
app="$target_dir/$profile_dir/bundle/macos/Macroscope.app"
cp native/macos-observer/.build/release/macroscope-observer "$app/Contents/MacOS/macroscope-observer"
cp "$target_dir/$profile_dir/macroscope-runner" "$app/Contents/MacOS/macroscope-runner"
cp "$target_dir/$profile_dir/macroscope-runner-launch" "$app/Contents/MacOS/macroscope-runner-launch"
cp crates/runner/profile.sb "$app/Contents/MacOS/macroscope-runner.sb"
# Re-sign ad hoc so the modified bundle stays internally consistent (not notarized).
codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
echo "Built: $app"
