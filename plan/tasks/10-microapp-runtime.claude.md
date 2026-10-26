Track: MICRO-APP RUNTIME, LIFECYCLE AND RELIABILITY. Output: plan/research/10-microapp-runtime.md
Design what a generated "micro-app" IS and how it lives on the user's machine:
- Artifact types, ranked by v1 priority: workflow on the Macroscope SDK/DSL; TypeScript/Python script; browser automation (Playwright/CDP against the user's real browser profile vs a separate one); browser extension; AppleScript/JXA/Shortcuts on macOS and PowerShell on Windows; tiny UI apps (a webview panel/widget with a form) hosted inside Macroscope; menu-bar items.
- A manifest format (show a full example YAML/JSON): id, name, version, triggers (hotkey, schedule, file-watch, app-event, manual, "when I'm in app X"), inputs/params, permissions (fs paths, network hosts, apps controlled, clipboard, secrets), runtime, entrypoint, tests, provenance (which opportunity, which model, which session).
- Runtimes to bundle: Deno or Bun (permission flags) vs Node; Python via uv; versions pinned; how to avoid making the user install toolchains.
- Execution: on-demand processes, timeouts, resource caps, logging, dry-run mode, the "preview what it will do" step, undo where possible (file ops to trash, not delete).
- Testing generated tools: record-and-replay fixtures from the observed episode, golden outputs, the agent must make the tests pass before it is offered.
- Reliability: UIs change, so automations break. Detect breakage, self-repair loop via Muse with the failing run log + fresh screenshot, versioning and rollback, health dashboard.
- Sharing/export later (a gallery? export as a standalone script?).
