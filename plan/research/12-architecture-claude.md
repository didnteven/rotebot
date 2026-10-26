# 12 - App architecture and tech stack (independent take B, Claude)

Status: planning only. Written 2026-10-22. Facts are tagged **[verified: source]**,
**[local: checked on this Mac]** or **[assumption]**.

## TL;DR

**Pick: (a) Tauri 2.** One Rust workspace powers everything. That is the menu-bar shell, the
always-on observer, the pattern/model engine, the micro-app runner and the client for
`muse serve`. On top sits a React + TypeScript UI rendered in the system webview (WKWebView
on macOS, WebView2 on Windows). Platform capture code is Rust: `objc2`/`screencapturekit`
on macOS and `windows-rs` on Windows. If a macOS API turns out too painful to call from
Rust, a small Swift static library is the planned fallback.

The decision rests on three things this product needs that most desktop apps don't:

1. **A process that never quits and has to be cheap.** The always-on parts (shell plus
   observer) are pure Rust with no JS runtime. Webviews are created when a window opens and
   destroyed when it has been closed for a while.
2. **Hosting untrusted, AI-written web micro-apps.** Tauri's per-window capability ACLs and
   custom URI schemes let a micro-app window get exactly the bridge calls it declared and
   nothing else. No Node process sits in the trusted path.
3. **One backend language.** Capture, redaction, storage, IPC, the MSP client and the
   sandbox broker all share types in one Cargo workspace. AI agents get compiler feedback
   on every boundary.

Trade-offs I accept: WebKit and Chromium render differently, so what the owner sees on Mac
isn't pixel-identical to Windows. macOS gets no WebDriver end-to-end testing. Menu-bar
popover polish needs one native shim (NSPanel). And Tauri 2 is younger than Electron, so
agents sometimes write v1-era APIs.

Runner-up: Electron with Rust native helpers. It is genuinely good, and I would switch to it
if the Week-0 spikes show WKWebView terminal or micro-app hosting is broken. Section 2
explains why it loses anyway.

---

## 1. Facts that shaped the decision

These came from inspecting the machine and the reference files today. Two of them change
the architecture.

| Fact | Tag | Consequence |
|---|---|---|
| `~/.local/bin/muse` is a **bash launcher**. `detect_platform()` accepts only `Darwin` and `Linux` (arm64/x86_64) and dies with "unsupported platform" on anything else. | [local: /Users/todd/.local/bin/muse lines 91-98] | **This launcher can't run Muse Code on Windows.** A third-party page mentions a PowerShell installer; Meta's own pages don't confirm it [unverified: innfactory.ai/en/ai-harness/muse]. The architecture must not *require* `muse` on Windows. |
| The real binary is `muse-bin-1.4.11-R6667.1`: **386 MB**, Mach-O arm64, Developer ID signed, hardened runtime, TeamIdentifier `V9WTTPBFK9`. | [local: `codesign -dv`] | Don't bundle it: it would add 386 MB per architecture, and the redistribution terms are unknown. Use the user's install, pinned and checked. |
| The launcher **auto-updates the binary in the background about once an hour** (`MUSE_UPDATE_INTERVAL_SECONDS`, default 3600). `MUSE_NO_AUTO_UPDATE=1` disables it. | [local: launcher `main()`] | The MSP schema can change under us. Check `schema.fingerprint` at `initialize` and set `MUSE_NO_AUTO_UPDATE=1` on the hosts we spawn. |
| If a download gets a 401/403/404, the launcher can start an OAuth **device login**, but only when stderr is a TTY or `MUSE_LOGIN=1`. | [local: `login_after_denial`] | Installing or updating muse may need a Meta *account* login even though model calls use the API key. We spawn with `MUSE_LOGIN=0` and stdio pipes so a host never hangs on an invisible prompt. |
| Muse credentials live at `$XDG_CONFIG_HOME/muse/auth.json`, otherwise `~/.config/muse/auth.json`. `MUSE_AUTH_PATH` overrides this. | [local: launcher] | We can point our hosts at an app-private config dir so we don't touch the user's own muse setup. Still to verify: that the binary (not just the launcher) honours this. Check `initialize.museHome`. |
| MSP is **JSON-RPC 2.0** over stdio. `InitializeResult` returns `museHome`, `platformOs` (`macos`/`linux`/`windows`), `sandboxAvailability`, `schema` (version + fingerprint) and `grantedCapabilities` (`userShell`, `sessionMcp`, ...). | [local: plan/reference/msp/msp.d.ts:811-839, 1118, 1431] | Generate Rust types from `msp.schema.json` and TS types from `msp.d.ts`. The schema allows a Windows host, so Windows support depends on packaging, not the protocol. |
| Vendored schema fingerprint: `sha256:f282ad16...1082`. | [local: plan/reference/msp/manifest.json] | This is the compatibility pin CI checks. |
| Dev Mac: M4, macOS 26.5, Swift 6.3.3, Node 24.10, Flutter installed, **no Rust toolchain installed**. | [local] | Day 1 means installing rustup. Everything else is already there. |

External facts used below:

- Flutter 3.44.8 stable ships the windowing API, but every class in it is `@internal`. Real
  multi-window needs the main channel or the `desktop_multi_window` plugin, which runs
  separate engines. **[verified: startdebugging.net/2026/08/how-to-enable-multi-window-support-in-a-flutter-desktop-app — single source]**
- Tauri's updater requires signatures, and this can't be turned off. **[verified: v2.tauri.app/plugin/updater]**
  CVE-2026-95625 is an unsigned-manifest downgrade in `tauri-plugin-updater`, fixed in
  updater-v2.12.0. **[verified: app.opencve.io/cve/CVE-2026-95625]**
- `screencapturekit` crate 8.0.1 covers screen/window/app capture and screenshots and
  includes a Tauri example. **[verified: docs.rs/crate/screencapturekit/8.0.1]**
  The `windows-capture` and `wgc` crates wrap Windows.Graphics.Capture. **[verified: docs.rs/windows-capture, docs.rs/crate/wgc]**
- TCC (macOS privacy permissions) treats a helper embedded in an app bundle as the app, and
  shows the app's name in the alert. **[verified: developer.apple.com/forums/thread/694948]**
  Grants made to a bare, versioned CLI binary break on every update. **[verified: claudeissues.com/issue/84948]**
  A LaunchAgent with screen-capture permission still gets recurring "still capturing"
  notifications on macOS 15+. **[verified: developer.apple.com/forums/thread/694948 + thread/762532]**
- Tauri vs Electron idle memory is **disputed**. Blog retests claim about 42 MB vs 168 MB
  **[verified: tech-insider.org/tauri-vs-electron-2026]**. A Tauri GitHub issue found the gap
  mostly disappears once shared memory is counted, and that WebView2 is close to Electron
  **[verified: github.com/tauri-apps/tauri/issues/5889]**. So I don't pick Tauri because
  "the webview is smaller". I pick it because **the always-on process doesn't have to hold
  a webview at all**.
- The Tauri positioner plugin supports tray-relative positions (`TrayCenter`, `TrayRight`,
  and so on). **[verified: v2.tauri.app/reference/javascript/positioner]**

---

## 2. Stack evaluation

Scores run from 1 (bad) to 5 (excellent) for *this* product. They're judgments, not
benchmarks.

| Criterion | (a) Tauri 2 | (b) Electron + helpers | (c) Flutter + FRB | (d) SwiftUI + WinUI 3 + Rust |
|---|---|---|---|---|
| Menu-bar/tray UX | 4 (tray + positioner; NSPanel via shim) | 4 (mature Tray; panel quirks too) | 3 (tray_manager/window_manager plugins) | 5 Mac / 4 Win |
| Multiple windows | 5 (first-class) | 5 | 2 (multi-window not public in stable) | 5 |
| Embedded terminal | 4 (xterm.js + portable-pty) | 5 (xterm.js + node-pty, VS Code grade) | 3 (xterm.dart, less battle-tested) | 3 (SwiftTerm on Mac, nothing equivalent on WinUI) |
| Webview micro-app hosting | 5 (multi-webview, per-window capabilities, custom schemes) | 5 (WebContentsView, partitions) | 2 (third-party desktop webviews, uneven on Windows) | 4 (WKWebView / WebView2, two bridges to write) |
| Native capture (SCK/AX) | 4 (Rust crates; Swift fallback) | 3 (always a separate native helper anyway) | 3 (helper or FFI anyway) | 5 Mac |
| Idle footprint (always-on) | 5 (no webview needed while idle) | 2 (main + GPU process stay resident) | 3 (engine stays resident) | 5 |
| Auto-update | 4 (plugin, signed; patch the CVE) | 5 (electron-updater) | 2 (roll your own / Sparkle + WinSparkle) | 3 (Sparkle + MSIX/custom, two systems) |
| Signing/notarization | 4 (built into bundler; sidecars signed) | 5 (electron-builder) | 3 (manual) | 3 (two pipelines) |
| Windows parity (owner can't test) | 3 (WebView2 differs from WebKit; strong Windows CI) | **5** (same Chromium both sides) | 4 (same renderer both sides) | **1** (a second UI nobody can test) |
| AI-agent fluency | 4 (React/TS 5, Rust 4, Tauri v1/v2 confusion) | 5 | 4 (Dart good, desktop plugins thin) | 3 (Swift ok, WinUI 3 weak) |
| Testability | 4 (Rust headless; Win WebDriver; mock IPC) | 4 (Playwright-electron) | 3 | 2 |
| **Total** | **46** | **48** | 32 | 39 |

Electron scores higher on paper and loses anyway. Here is the honest reasoning.

**Why not Electron, despite the score.** Its two big wins are rendering parity and a
mature terminal and updater. Both are real. But:

- **Two backend languages.** Capture has to be native, so Electron means Node in the main
  process plus Rust or Swift helpers. Every type crosses a Node/native boundary. In Tauri
  the shell *is* Rust and links the same crates as the observer.
- **Node in the trusted path.** The main process holds the API key, decrypts frames and
  brokers micro-app permissions. That's the worst place for a big transitive npm dependency
  tree, in an app that sees the whole screen.
- **The tray process is resident.** Tray, main process and GPU process stay alive at about
  80-150 MB, all day, every day [assumption, consistent with tauri#5889 numbers]. You'd end
  up moving the tray into a native helper, at which point you're building Tauri by hand.
- **The parity argument is weaker than it looks.** Windows parity risk here is mostly
  *capture* (WGC, UIA, hooks, antivirus heuristics), not CSS. Electron doesn't help with
  that at all.

**Why not Flutter, though the owner knows it.** Two of the product's core surfaces are
web-native: hosting HTML micro-apps and an xterm-grade terminal. Flutter desktop has no
first-party desktop webview. Multi-window isn't public on stable (verified above, one
source). Desktop maintenance is reportedly moving to Canonical [unverified: ubunlog.com,
single source]. And the owner builds with agents, so personal familiarity counts for less
than ecosystem depth.

**Why not native x2.** It's the best Mac app and the worst plan for this owner. The Windows
UI would be written by agents in the stack they're weakest at (WinUI 3/XAML) and never
hand-tested. That guarantees the worst parity.

**Main risk of my pick and its kill switch.** WKWebView may fall short for xterm.js
(WebGL renderer, IME, copy/paste) or for micro-app isolation. Spike S4 tests this in
Week 0. If it fails, the Rust crates move unchanged under an Electron shell; only `apps/shell`
and the UI bridge are rewritten. I keep the UI framework-agnostic: React talks only to a
generated `bindings.ts`, never to `@tauri-apps/api` directly.

---

## 3. Chosen stack, concretely

| Layer | Choice | Notes |
|---|---|---|
| Shell | Tauri 2.x (latest stable), `tauri-plugin-updater` >= 2.12.0 | Plugins: tray, positioner, global-shortcut, notification, autostart, deep-link, single-instance, window-state |
| UI | React 19 + TypeScript + Vite, TanStack Router/Query, Tailwind + Radix primitives | Agents are most fluent here. One SPA with routes per window type. |
| UI <-> Rust bindings | `tauri-specta` (generated TS types for commands/events) [assumption: crate is maintained for Tauri 2] | Agents can't drift from the Rust types. Fallback: hand-written `ts-rs`. |
| Terminal | `xterm.js` (+ WebGL addon, falls back to canvas) and `portable-pty` in Rust | Same pair WezTerm/VS Code-style terminals use |
| Async runtime | `tokio` | Every process |
| Storage | SQLite via `rusqlite` (bundled SQLCipher feature), `refinery` migrations, encrypted frame files | Single writer per database (section 6) |
| Vectors | `sqlite-vec` extension [assumption: stable enough], else a flat in-process index | Only for workflow similarity; data stays small |
| Secrets | `keyring` crate: macOS Keychain, Windows Credential Manager/DPAPI | |
| macOS capture | `screencapturekit` 8.x, `objc2-app-kit`, `objc2-application-services` (AX), CGEventTap through `core-graphics`, `objc2-vision` for OCR | Swift fallback: `swift-rs` static lib `MacSensors` |
| Windows capture | `windows` (windows-rs): Windows.Graphics.Capture, UI Automation, `SetWinEventHook`, Raw Input, `Windows.Media.Ocr` | |
| HTTP to Meta | `reqwest` + `rustls`; base URL and model ids come from config, never hard-coded | Base URLs aren't verified (BRIEF) |
| MSP client | Our own crate `mscope-msp`: types generated with `typify` from `msp.schema.json` | NDJSON vs Content-Length framing: [assumption: newline-delimited JSON-RPC; verify in spike S1] |
| Crash capture | `crash-handler` + `minidumper` writing local minidumps; upload is opt-in and scrubbed | No always-on telemetry SaaS |
| Tests | `cargo nextest`, `insta` (snapshots), `proptest`; Vitest + Testing Library; Playwright (WebKit + Chromium) for UI against mocked IPC; WebdriverIO + `tauri-driver` on Windows | |

---

## 4. Process model

There are five kinds of process, all signed and shipped **inside one bundle**. No bare
binaries go anywhere on disk; that's what keeps TCC grants alive across updates.

```
Macroscope.app/Contents/
  MacOS/Macroscope                 <- shell (Tauri), LSUIElement=1 (menu-bar only, no Dock icon by default)
  MacOS/macroscope-observer        <- always-on capture helper
  MacOS/macroscope-engine          <- pattern + model worker
  MacOS/macroscope-runner          <- micro-app executor (one process per run)
  Resources/ui/...                 <- built web assets
  Resources/sandbox/*.sb           <- runner sandbox profiles
(external, not bundled)  ~/.local/bin/muse -> muse-bin-x.y.z   <- spawned as `muse serve`
```

On Windows the same binaries sit in `%LOCALAPPDATA%\Programs\Macroscope\`, installed
per-user by NSIS with no admin rights.

### 4.1 Shell (`Macroscope`): supervisor + UI host, always running

- Owns the tray/menu-bar icon, every window, the settings store, the secrets store, the
  update client and the **supervisor** for every other process.
- Hosts the **agent broker**, which spawns and talks to `muse serve` hosts (or the fallback
  builders), and the **PTY host** for the embedded terminal.
- Hosts the **capability broker** for web micro-apps: the `window.macroscope.*` bridge.
- Serves frames to the UI through a custom scheme, `mscope-frame://<frame_id>`. It decrypts
  in memory, so the webview never sees file paths or keys.
- Runs at login: `SMAppService.mainApp` on macOS (through `tauri-plugin-autostart` or a
  10-line objc2 call), the HKCU Run key on Windows.
- **Idle discipline:** the Rust process with no webviews is the only resident UI cost. The
  menu-bar popover webview stays warm so it opens instantly. Main, Builder and micro-app
  windows are *destroyed* 5 minutes after closing, and their state lives in Rust or SQLite.
  Budget: shell under 60 MB RSS idle with the popover warm, under 40 MB cold
  [assumption: targets to verify in spike S6].
- Single instance (`tauri-plugin-single-instance`). A second launch focuses the main window.

### 4.2 Observer (`macroscope-observer`): always on while capture is enabled

- **Senses:** active app/window changes, window titles, AX/UIA focused-element role and
  label (no values from secure fields), click and keystroke *patterns* (counts, modifier
  chords, timing; never characters), clipboard *events* (type and size, not content, unless
  the user allows it per app), and screen frames **only when the UI changes**.
- **Frame pipeline:** SCK/WGC stream at low fps, then a perceptual-hash diff, then a debounce
  (e.g. 750 ms settle), then on-device OCR (Vision / Windows.Media.Ocr), then **redaction**
  (`mscope-redact`: PII regexes, secure-field rects from AX/UIA, app/URL denylist blackouts),
  then encrypt and write. Raw unredacted pixels never reach disk. That's a design rule, not
  a setting.
- Writes only to `capture.sqlite` plus the frame store. Sends status and heartbeats to the
  shell over IPC.
- **Tied to the shell's lifetime on purpose.** If the shell dies, the observer sees EOF on
  its control pipe and a kqueue `NOTE_EXIT` on the parent PID (Windows: a Job Object with
  `KILL_ON_JOB_CLOSE`), then flushes and exits. Privacy invariant: **no menu-bar icon means
  no capture.** That's also why the observer isn't a launchd KeepAlive agent; the recurring
  capture notification for LaunchAgents is a second reason.
- Budget: under 60 MB RSS, under 2% of one core averaged, about 0 when the screen is static
  [assumption: targets].

### 4.3 Engine (`macroscope-engine`): pattern + model worker, scheduled

- Runs batch passes: segment events into episodes, mine recurring workflows, rank them by
  time × frequency × load, then call Meta models for labelling and description. The detail
  is covered by the pattern-engine track; this doc fixes only its boundaries.
- **The only process that sends capture-derived content off the device.** Every outbound
  request goes through `mscope-meta::Egress`, which:
  1. refuses anything not marked `redacted=true` by the store,
  2. enforces the daily budget and cost cap,
  3. writes an `egress_log` row: what was sent, which model, tokens, cost and a hash of the
     payload, so the UI can show "what left my machine today".
- The shell starts it when the user is idle, on power, under budget, or on demand ("Find
  patterns now"). It exits when done. Killing it at any time is safe, because each pass is
  a transaction.
- Writes only `knowledge.sqlite`. Reads `capture.sqlite` through SQLite WAL, which allows
  concurrent readers.

### 4.4 Runner (`macroscope-runner`): micro-app executor, one process per run

- One short-lived process per script run (shell, Python, Node, AppleScript, PowerShell).
  Spawned by the shell with an explicit **run spec**: app id, pinned version hash, inputs,
  granted capabilities.
- macOS: `sandbox-exec` with a generated SBPL profile limited to the app's workspace, granted
  paths and network allowlist [assumption: deprecated but functional on macOS 26; the
  micro-app-runtime track owns the final sandbox choice]. Windows: AppContainer or
  restricted token plus a Job Object (CPU/mem/time limits, kill-on-close).
- Side effects outside the sandbox (send an email, write a file outside the workspace, open
  a URL) go back to the shell as `capability.request`. The shell checks the pinned grant and
  asks the user the first time.
- **Web micro-apps don't use the runner.** The shell opens them in a dedicated Tauri window
  that loads `mapp://<app_id>/<version>/index.html`, with a **per-window capability set**
  generated from the app's manifest: no `fs`, no `shell`, only the declared
  `macroscope.*` bridge calls. Remote navigation is off unless declared.

### 4.5 Muse hosts (`muse serve`), spawned on demand by the shell's agent broker

- **One host per sandbox posture**, because the posture is fixed per process
  [local: `muse serve --help`]:
  - `builder` host: `--sandbox-network proxy-only`, writes on, shell on, `--no-session-log`
    off (we want durable sessions). Used for "Automate this".
  - `explain` host: `--disable-write --disable-shell`. Used for "why did you suggest this?"
    and code-review side chats.
- Spawn env: `META_API_KEY=<from keychain>`, `MUSE_NO_AUTO_UPDATE=1`, `MUSE_LOGIN=0`,
  `XDG_CONFIG_HOME=<app>/agent/muse-config` [assumption: isolates muse settings/trust; check
  that `initialize.museHome` points there]. stdin/stdout are pipes owned by the broker;
  stderr goes to `logs/muse-<host>.log` (redacted).
- `initialize` is sent with `clientInfo {name:"macroscope", version}` and requests
  `userShell`. The broker then checks:
  - `schema.version == 1`
  - the fingerprint: a mismatch logs a warning and continues, because the stable schema is
    additive
  - `platformOs`
  - `sandboxAvailability`: if it's `unavailable`, builder sessions are refused and the UI
    says why
- Approval mode is set on the wire to `on-request`. Every `approval/requested` becomes a
  native-looking card in the Builder window. `approval/decide` carries a UUIDv7
  `commandId`, so a replay after a crash is idempotent [local: msp.d.ts ApprovalDecideParams].
- Hosts are idle-reaped after 10 minutes without a session. Sessions are durable and
  resumed with `session/resume`.
- **The `Builder` trait** (`mscope-builder`) hides muse behind an interface, with three
  implementations:
  1. `MuseServeBuilder` (primary),
  2. `MuseExecBuilder` (`muse exec --json --api-key-stdin --output-schema`, for headless
     regeneration and as a fallback if serve misbehaves),
  3. `DirectMetaBuilder`: a minimal agent loop of our own that calls the Meta API directly
     with three tools (write_file, run_tests_in_runner, read_file). **This is the Windows
     safety net** in case Muse Code really doesn't ship for Windows.

### 4.6 Embedded terminal ("power mode")

The owner wants Muse Code in a terminal inside the app. The Builder window has two tabs over
the **same micro-app workspace**:

- **Guided** (default): the structured MSP conversation (plan, diff, approvals, test run,
  pin) rendered natively from `item/*` events.
- **Terminal**: xterm.js connected to a `portable-pty` session in the shell. It runs the
  user's login shell in `apps/<id>/src` with `META_API_KEY` injected **only into that PTY's
  environment**, and auto-types `muse` if the user picks "Open Muse Code here". This is
  real interactive Muse Code, unmodified.

Because both tabs work on the same git repo, a change made in the terminal shows up in
Guided as a diff. That makes the terminal a first-class escape hatch, not a separate
product. The PTY is killed when the window is destroyed, and the scrollback is saved to
`apps/<id>/terminal.log`.

---

## 5. IPC

```
 UI (webview)  <--Tauri commands/events (typed, tauri-specta)-->  Shell (Rust)
 Shell <--> observer/engine/runner : JSON-RPC 2.0, length-prefixed frames,
                                     Unix domain socket (mac) / named pipe (win)
 Shell <--> muse serve             : MSP = JSON-RPC 2.0 over child stdio
 Shell <--> web micro-app window   : Tauri IPC limited by a per-window capability ACL
 Bulk data (frames, embeddings)    : never over IPC; written to the store, ids passed
```

Decisions:

- **Child IPC uses JSON-RPC 2.0, matching MSP.** It's one mental model, one debug tool
  (`mscopectl tap`), and agents already know it. Messages are serde types in `mscope-ipc`.
  JSON Schemas are exported with `schemars` and snapshot-tested, so a breaking change fails
  CI.
- **Transport:** the `interprocess` crate. Socket path:
  `~/Library/Application Support/Macroscope/run/<pid>.sock` (dir mode 0700). On Windows,
  `\\.\pipe\macroscope-<user-sid>-<pid>` with a DACL that allows only the current user SID.
- **Peer authentication:** the shell spawns children and passes a one-time 32-byte token on
  stdin as the first line. Children present it in `hello`. On macOS the shell also checks
  the peer's code signature (`SecCodeCopyGuestWithAttributes` with the PID and the same Team
  ID requirement). On Windows: `GetNamedPipeClientProcessId` plus an image path under the
  install dir. Other same-user processes can't impersonate the observer to read or inject
  events.
- **Version handshake:** `hello {build_id, ipc_version}`. A mismatch (say, after an update
  that only partly restarted things) makes the shell restart the child.
- **Key methods (illustrative):**
  - observer: `capture.pause {reason, until}`, `capture.resume`, `capture.status ->`, `policy.reload`, notify `capture.health {fps, dropped, tcc_state}`, `heartbeat`
  - engine: `engine.runPass {from,to,budget_usd}`, notify `engine.progress`, `engine.workflowsChanged {ids}`, `engine.egress {row_id}`
  - runner: `run.start {spec}`, notify `run.log`, request `capability.request {kind, detail}`, notify `run.finished {status, outputs}`
- **Heartbeats** every 5 s. Three misses means the shell kills and restarts the child
  (section 7).
- **Secrets delivery:** the API key never goes into argv. The engine receives it in the
  stdin bootstrap message. Muse hosts and the PTY get it through the child env, because
  that's the only path MSP serve offers; `-c` accepts only `${VAR}` references
  [local: BRIEF / help text]. The shell itself never has `META_API_KEY` in its own
  environment.

---

## 6. On-disk storage layout

Root: macOS `~/Library/Application Support/Macroscope/`; Windows `%LOCALAPPDATA%\Macroscope\`
(Local, not Roaming: the data is large and per-machine). Caches go in `~/Library/Caches/Macroscope`
or `%LOCALAPPDATA%\Macroscope\Cache`. Logs go in `~/Library/Logs/Macroscope` or `...\Macroscope\Logs`.

```
Macroscope/
  config/
    settings.json            # versioned ("schema": 3); written only by the shell
    policies.json            # app/URL denylist, masking rules, retention, budgets
  keys/                      # nothing secret on disk; wrapped DEK only
    dek.wrapped              # data key, wrapped by Keychain/DPAPI master key
  db/
    capture.sqlite (+-wal)   # WRITER: observer   events, frames index, sessions, tcc log
    knowledge.sqlite         # WRITER: engine     episodes, workflows, rankings, egress_log, vectors
    apps.sqlite              # WRITER: shell      micro-apps, versions, grants, runs, audit, muse session ids
  frames/
    2026/10/22/14/<frame_id>.webp.enc   # redacted, AES-256-GCM, per-file nonce
  blobs/sha256/ab/cd/<hash>             # content-addressed attachments (sample frames sent to builder)
  apps/
    <app_id>/
      manifest.json          # type, entrypoints, declared capabilities, trigger
      src/                   # git repo; muse workspace; drafts on worktrees
      versions/<hash>/       # immutable pinned builds the runner executes
      runs/<run_id>/{stdout.log,stderr.log,outputs/}
      terminal.log
  agent/
    muse-config/             # isolated XDG_CONFIG_HOME for spawned muse hosts
    transcripts/             # exported MSP session logs per app (debug, redacted)
  run/                       # sockets, pid files, lock files (0700)
  crash/                     # minidumps + last-200-lines ring logs
  tmp/                       # atomic-write staging; swept on start
```

Rules:

- **One writer per database.** Each DB is written by exactly one process, which also owns
  its migrations and runs them at startup. Readers check `PRAGMA user_version` and refuse to
  read a schema newer than they know. That rule removes most cross-process SQLite locking
  bugs.
- **Encryption at rest:** SQLCipher for the three DBs, AES-GCM for frames. One random data
  key (DEK) is wrapped by a Keychain or DPAPI item that's bound to the app. Deleting the
  Keychain item crypto-shreds everything, which doubles as the "Delete all my data" fast path.
- **Retention:** redacted frames are kept for 7 days by default; episodes and workflows
  until deleted. Disk cap: 5 GB, oldest first. The observer runs the frame GC, because it
  owns the frame store.
- **Atomic writes:** write to `tmp/`, fsync, rename, *then* insert the DB row. On startup,
  orphaned tmp files and DB rows without files are reconciled.

---

## 7. Crash recovery

| Failure | Detection | Recovery | User sees |
|---|---|---|---|
| Observer crash/hang | pipe EOF or 3 missed heartbeats | Restart with backoff 1 s, 2 s, 4 s ... capped at 60 s. **Crash-loop breaker:** 5 crashes in 10 minutes disables capture. | Menu-bar icon shows "paused"; banner "Capture stopped after repeated errors - View log / Retry" |
| TCC permission revoked | SCK stream error / `AXIsProcessTrusted()==false` / `CGPreflightScreenCaptureAccess` | Observer reports `tcc_state`; no restart loop | Onboarding-style permission card with a deep link to System Settings |
| Engine crash | exit code | Pass transaction rolled back; reschedule next idle window | Nothing, unless 3 passes fail in a row |
| Runner crash/timeout | Job Object/process group | Run marked `failed`/`interrupted`; **no automatic retry** unless the manifest says `idempotent: true` | Run card with logs and "Fix with Muse" |
| Muse host crash | stdio EOF | Respawn host, `session/resume` with the stored session id; un-acked commands replayed with the same `commandId` | "Reconnecting to Muse..." inline, then the conversation resumes |
| Muse auth/quota | MSP `TurnErrorKind=authRequired` / `QuotaDetail` [local: BRIEF] | No retry until the user acts or the reset instant passes | "API key rejected - Update key" / "Rate-limited until 15:40" |
| Shell crash | n/a (it's the supervisor) | Children exit (parent-death detection). Next launch: replay WAL, sweep tmp, mark runs `interrupted`, show the capture gap. | "Macroscope quit unexpectedly. Capture was off 14:02-14:09." |
| WebView content crash | Tauri webview process-terminated event | Recreate the window from Rust-held state | A brief flicker |
| Disk full | write error | Observer pauses itself; GC runs | Banner with a storage settings link |

The shell itself isn't KeepAlive-relaunched in v1. A boring Rust supervisor with no
JS-driven logic rarely crashes, and an honest "capture was off" gap is better than a
background resurrection the user didn't ask for. If crash telemetry (opt-in) shows this is
a problem, v1.1 can register the shell as an `SMAppService.agent` with
`KeepAlive{SuccessfulExit=false}`.

---

## 8. Updates, signing, notarization

**The app**

- `tauri-plugin-updater` **>= 2.12.0** (CVE-2026-95625), plus a defence of our own: the
  updater manifest (`latest.json`) gets a detached minisign signature, and the client
  **refuses any version lower than the one installed** unless a signed `min_version`
  rollback field says otherwise.
- Hosting: GitHub Releases (artifacts) plus a static `latest.json` per channel
  (`stable`, `beta`) on a CDN.
- Apply policy: check every 6 h. Download in the background. Install only when no Builder
  session or run is active, at most once per day, unless `urgency: critical`. On install
  the shell flushes the observer, swaps, and relaunches; the capture gap is about 3 s.
- **macOS:** a Developer ID Application certificate with hardened runtime on *every* nested
  executable (Tauri `externalBin` sidecars get signed by the bundler), `notarytool` from CI
  with an App Store Connect API key, then stapling. Ship a universal2 DMG (arm64 + x86_64).
  **Not** in the Mac App Store: the App Sandbox is incompatible with global AX, event taps
  and spawning user CLIs.
- **TCC durability rule:** the bundle id `com.macroscope.app` (or whatever the owner picks)
  and the Team ID **never change**, and all helpers live inside the bundle, so Screen
  Recording, Accessibility and Input Monitoring grants survive updates.
- **Windows:** NSIS per-user installer for x64 and arm64, signed with Azure Trusted
  (Artifact) Signing [assumption: available to the owner as an individual; check
  eligibility]. Expect SmartScreen warnings until the publisher builds reputation. The
  updater runs NSIS in passive mode. The WebView2 runtime is evergreen on Windows 11; the
  installer bootstraps it on 10.

**Muse.** We don't update it. The shell shows the installed version and fingerprint
compatibility in Settings > Builder, and offers "Update Muse Code", which runs the official
launcher with `MUSE_LAUNCHER_INSTALL=1` in a visible terminal tab, because it may need a
device login [local].

**Micro-apps** are versioned by git commit and pinned by content hash. "Update" means the
agent proposes a diff, the user previews it, and the user re-pins. A pinned version never
changes silently.

---

## 9. Component diagram

```
+--------------------------------------------- Macroscope (one signed bundle) ----------------------------------------------+
|                                                                                                                           |
|   +------------------------------- Webviews (created on demand) -------------------------------+                          |
|   |  [Menu-bar popover]  [Main: Inbox/Timeline/Apps]  [Builder: Guided | Terminal]  [mapp://*] |                          |
|   |   React + TS, generated bindings.ts                xterm.js <-+         micro-app windows  |                          |
|   +------------^------------------------^--------------------------|-----------------^---------+                          |
|                | Tauri IPC (typed)      |                          | PTY bytes       | capability-ACL'd bridge            |
|   +------------+------------------------+--------------------------+-----------------+--------------------------------+   |
|   |  SHELL (Rust, always on)                                                                                          |   |
|   |   tray/windows | settings | secrets(keyring) | updater | Supervisor | Hub(event bus)                              |   |
|   |   AgentBroker --(Builder trait)--> MuseServe | MuseExec | DirectMeta     PTY host     CapabilityBroker            |   |
|   |   mscope-frame:// decrypting scheme      apps.sqlite (writer)                                                     |   |
|   +-----+-------------------+----------------------+---------------------------+-------------------------+-----------+   |
|         | JSON-RPC/UDS|pipe | JSON-RPC             | JSON-RPC (per run)        | MSP JSON-RPC stdio      | env key   |   |
|   +-----v---------+   +-----v-----------+   +------v-----------+        +------v-----------+   +---------v---------+   |
|   | OBSERVER      |   | ENGINE          |   | RUNNER (xN)      |        | muse serve       |   | login shell in PTY|   |
|   | SCK/WGC frames|   | episodes, mining|   | sandbox-exec /   |        | (user-installed, |   | -> `muse` TUI     |   |
|   | AX/UIA, hooks |   | ranking         |   | AppContainer+Job |        |  pinned, isolated|   +-------------------+   |
|   | OCR + REDACT  |   | Egress gate ----+---+------------------+--+     |  XDG config)     |                           |
|   | capture.sqlite|   | knowledge.sqlite|   | apps/<id>/vers/* |  |     +--------+---------+                           |
|   +-------+-------+   +--------+--------+   +------------------+  |              |                                     |
|           | writes            ^ reads (WAL)                       |              |                                     |
|   +-------v-------------------+---------------------------------+ |              |                                     |
|   |  STORE: db/*.sqlite (SQLCipher)  frames/*.enc  blobs/  apps/ | |              |                                     |
|   +--------------------------------------------------------------+ |              |                                     |
+-------------------------------------------------------------------|--------------|-------------------------------------+
                                                                    | HTTPS        | HTTPS (META_API_KEY)
                                                                    v              v
                                                         +-----------------------------------+
                                                         |  Meta model API (Muse models)      |
                                                         |  base URL + model ids from config  |
                                                         +-----------------------------------+
```

Two egress points exist and both are visible in the UI: `Engine.Egress` (analysis) and the
muse host (building). The muse host only ever receives what the broker hands it: a workflow
description, redacted sample frames through `media/upload`, and the app workspace.
Capture-derived text goes in as **quoted data inside the user turn**, never into
skills/rules/AGENTS.md. That's a structural prompt-injection mitigation; the full threat
model belongs to the privacy track.

---

## 10. Monorepo

```
macroscope/
  AGENTS.md                      # rules for coding agents: Tauri 2 (NOT v1) APIs, crate map, how to run tests
  CLAUDE.md -> AGENTS.md
  Cargo.toml                     # workspace
  rust-toolchain.toml            # pinned stable
  package.json, pnpm-workspace.yaml
  crates/
    mscope-core/                 # ids (UUIDv7), time, config types, errors
    mscope-ipc/                  # JSON-RPC framing, UDS/named-pipe transports, peer auth, schemars export
    mscope-store/                # rusqlite+SQLCipher, migrations per DB, frame store, DEK wrapping
    mscope-capture/              # Sensor trait, frame diff/debounce pipeline, FakeSensor (fixture replay)
    mscope-capture-mac/          # cfg(macos): screencapturekit, AX, CGEventTap, NSWorkspace, Vision OCR
    mscope-capture-win/          # cfg(windows): WGC, UIA, WinEvent hooks, Raw Input, Windows.Media.Ocr
    mscope-redact/               # detectors + rect blackout; golden-image tests
    mscope-patterns/             # segmentation, mining, ranking (pure, deterministic)
    mscope-meta/                 # Meta HTTP client, Egress gate, budget governor
    mscope-msp/                  # generated MSP types + async client + transcript recorder/replayer
    mscope-builder/              # Builder trait; muse_serve, muse_exec, direct_meta impls
    mscope-runner-core/          # run spec, sandbox profile generation, capability types
    mscope-supervisor/           # spawn, backoff, heartbeats, crash-loop breaker, parent-death
    mscope-secrets/              # keyring wrapper
    mscope-pty/                  # portable-pty session mgmt
  apps/
    shell/                       # Tauri app (src-tauri equivalent): tray, windows, commands, capabilities/*.json
    observer/                    # bin
    engine/                      # bin
    runner/                      # bin
    mscopectl/                   # dev CLI: db inspect, `tap` IPC, replay fixtures, fake MSP host
    mac-sensors/                 # (only if needed) Swift package exposed via swift-rs
  ui/
    app/                         # React SPA; routes: /popover /main /builder /onboarding /settings
    packages/
      bindings/                  # generated by tauri-specta (checked in, CI verifies fresh)
      msp-types/                 # generated from schemas/msp/msp.d.ts
      ui-kit/                    # design-system components
  microapp-sdk/
    js/                          # window.macroscope bridge typings + shim
    templates/                   # script-python, script-shell, webapp, shortcut, extension
  schemas/
    msp/                         # vendored msp.schema.json, msp.d.ts, manifest.json (fingerprint pin)
    ipc/                         # exported JSON Schemas (snapshot-tested)
  fixtures/
    capture/                     # synthetic recorded sessions (events + redacted frames)
    msp/                         # recorded MSP transcripts (echo + real, scrubbed)
  tests/
    e2e-win/                     # WebdriverIO + tauri-driver
    e2e-ui/                      # Playwright against mocked IPC (WebKit + Chromium)
  scripts/                       # dev.sh, bundle-mac.sh, notarize.sh, gen-bindings.sh
  docs/adr/                      # one ADR per decision in this file
  .github/workflows/             # ci.yml, release.yml, nightly.yml
```

Agent-friendliness rules written into AGENTS.md:

- Every crate has a `README` with its single responsibility and its public API.
- Platform code is only allowed in `*-mac`/`*-win` crates behind traits.
- Generated files are never hand-edited.
- `just test` runs everything that can run on the current OS in under 3 minutes.

---

## 11. CI (GitHub Actions)

| Workflow | Trigger | Runners | Steps |
|---|---|---|---|
| `ci.yml` | PR / push | `macos-15` (arm64), `windows-latest` (x64), `ubuntu-latest` | fmt + clippy `-D warnings`, `cargo nextest` (platform crates on their own OS; pure crates on Ubuntu for speed), check generated bindings are fresh, eslint + tsc + Vitest, IPC schema snapshots, **MSP replay tests** (recorded transcripts against `mscope-msp`), Playwright UI tests (WebKit + Chromium) with mocked IPC |
| `ci.yml` (Windows extra) | PR | `windows-latest` | Build the NSIS bundle, then **WebdriverIO + tauri-driver E2E** (onboarding, tray popover, open builder, run a fixture micro-app). Upload **screenshots of every window** as artifacts so the owner or a vision-capable agent can review Windows UI from the Mac. Also an observer smoke test: start observer with `FakeSensor` + a real WGC capture of a test window [assumption: hosted Windows runners have an interactive desktop session] |
| `nightly.yml` | cron | **self-hosted runner on the owner's Mac** + `windows-latest` | Live tests against `muse serve --provider echo` (deterministic, no cost) and one tiny real `meta` call with a CI key and a $ cap. Memory/CPU budget test: run shell + observer for 30 min against a scripted UI, fail if RSS or CPU goes over budget. Real SCK capture with TCC pre-granted on the self-hosted machine. |
| `release.yml` | tag `v*` | `macos-15`, `windows-latest` (x64 + arm64 cross) | Build universal2 `.app`, sign every binary, notarize + staple, DMG. Windows: build, Trusted Signing, NSIS. Generate and sign `latest.json` (Tauri key + minisign). Upload to GitHub Release. Promote `beta` to `stable` by hand. |

Notes:

- `muse` probably can't be installed on hosted runners without credentials (the launcher
  may want a device login) [local]. So hosted CI uses **recorded transcripts**, and live
  MSP tests run only on the self-hosted Mac.
- The fake MSP host in `mscopectl` (`mscopectl fake-msp --script x.jsonl`) lets Windows CI
  exercise the full Builder UI flow with no muse binary. That's how Windows Builder parity
  gets tested.
- macOS has no WebDriver for WKWebView, so tauri-driver is Windows/Linux only
  [assumption: per Tauri WebDriver docs, re-check]. Mac UI coverage comes from Playwright
  WebKit against the mocked IPC, plus the owner's hands. The *least* hand-tested platform
  gets the *most* automated E2E, which is the right way round.

---

## 12. Testability strategy

- **Pure-core first:** `mscope-patterns`, `mscope-redact`, `mscope-ipc` and `mscope-msp`
  have no OS dependencies and carry most of the logic. Property tests cover the ranking and
  segmentation invariants. Golden images cover redaction: a frame with a fake credit card
  must come out blacked out, and an `insta` snapshot of the boxes is checked.
- **Sensor fakes:** `FakeSensor` replays `fixtures/capture/*.jsonl` with frames, so the
  observer pipeline, the store and the engine run end-to-end on any OS without TCC.
- **MSP:** recorder/replayer. Every live session in dev can be saved (scrubbed) as a fixture.
  The echo provider gives deterministic live tests.
- **UI:** components don't call Tauri directly. They get a `Backend` interface, which is the
  real generated bindings in the app and `mockIPC` in tests and Storybook-style preview
  routes.
- **Budgets as tests:** footprint and egress (no non-redacted frame leaves the device) are
  CI assertions, not aspirations.

---

## 13. Week-0 spikes (each one is a go/no-go for part of this plan)

| # | Spike | Pass criterion | If it fails |
|---|---|---|---|
| S1 | Rust MSP client talks `initialize` + `session/start` + one turn to `muse serve` using `META_API_KEY` from env (echo, then meta) | Turn completes; `authRequired` is surfaced on a bad key; framing confirmed | Use `MuseExecBuilder` as primary |
| S2 | Tauri tray + popover as a non-activating NSPanel that shows over full-screen apps | Opens in under 100 ms warm, dismisses on outside click | Use tauri-nspanel community plugin [assumption: exists] or an objc2 subclass swap |
| S3 | `macroscope-observer` inside the bundle captures with `screencapturekit`; TCC prompt names "Macroscope"; grant survives an app update | All three hold | Move the capture calls into the `mac-sensors` Swift lib |
| S4 | xterm.js (WebGL) + portable-pty runs the interactive `muse` TUI in WKWebView, with IME, copy/paste and resize | Feels native | **Kill switch:** reconsider Electron for the shell |
| S5 | Windows CI: build, E2E, screenshot artifacts, WGC capture on the hosted runner | Green | Use a cloud Windows VM runner |
| S6 | Footprint: shell (popover warm) + observer idle for 1 h on M4 | Under 150 MB combined RSS, under 2% CPU | Destroy the popover webview when idle; tune capture fps |

---

## 14. Product-killing risks this architecture has to contain

1. **Muse isn't available on Windows** [local launcher evidence]. Contained by the `Builder`
   trait and `DirectMetaBuilder`. Windows v1 may ship with "Builder (lite)". The owner must
   accept that parity gap, or Meta must ship Windows Muse.
2. **Muse schema/behaviour drift from hourly auto-updates** [local]. Contained by
   `MUSE_NO_AUTO_UPDATE=1`, the fingerprint check, a replay test corpus, and the
   `MuseExecBuilder` fallback.
3. **Muse install may need a Meta account (device login) even though inference is API-key
   billed** [local]. That may conflict with "API key only". Raised as an owner question.
4. **Windows antivirus heuristics:** a per-user app with low-level input hooks plus screen
   capture plus a new signing reputation looks like a keylogger. Contained by using
   `SetWinEventHook`/UIA and Raw Input counts rather than `WH_KEYBOARD_LL` where possible,
   capturing no key characters by design, Trusted Signing, and a clear onboarding
   explanation [assumption: heuristics vary by vendor].
5. **macOS recurring screen-capture confirmations** (Sequoia+) nag the user every month or
   so [verified: Apple forum threads above]. This can't be avoided for a Developer ID app
   without special entitlements. The UX track should design for it.
6. **Rendering divergence WebKit vs WebView2** on an untestable platform. Contained by
   conservative CSS, Playwright cross-engine tests and Windows screenshot artifacts.

---

## Open questions for the owner

1. **Windows Builder parity:** if Muse Code doesn't run natively on Windows, is
   "Builder (lite)" on Windows v1 acceptable? That's our own agent loop calling Meta models
   directly with the same API key, and no Muse terminal. The alternative is making Windows
   v1 observer-plus-suggestions only.
2. **Muse install:** installing and updating Muse Code may need a one-time Meta *account*
   device login. Is that acceptable for end users, or must the app work with the API key
   alone (pushing us toward `DirectMetaBuilder` everywhere)?
3. **Windows VM:** would you run Windows 11 ARM in a VM on the M4 (VMware Fusion, Parallels
   or UTM) for occasional smoke tests? That turns "untestable" into "rarely tested" and
   costs about a day of setup.
4. **Identity:** the bundle id and the Apple Team (individual or company) must be fixed
   before the first public build. TCC grants and the updater depend on them never changing.
   Which one?
5. **Self-hosted CI runner on your Mac** for nightly live Muse/TCC tests: OK to keep a
   GitHub Actions runner on this machine, with a separate low-budget Meta API key?

## Decisions I made

- Stack is Tauri 2 + React/TS UI + one Rust workspace for all backend processes. Electron
  is the documented fallback if spike S4 fails.
- Platform capture is written in Rust (`screencapturekit`, objc2, windows-rs). A Swift
  static lib is used only if a spike forces it.
- Five process types ship in one signed bundle: shell, observer, engine, runner (one per
  run), and external `muse serve` hosts.
- The shell is the supervisor and the login item. The observer dies with the shell (no
  icon means no capture).
- The observer is not a launchd KeepAlive agent in v1.
- Webviews are created on demand. Only the popover stays warm. Idle budget is 150 MB for
  shell + observer.
- Raw unredacted pixels never touch disk. Redaction happens inside the observer.
- The engine is the only analysis egress, through an audited `Egress` gate.
- Child IPC is JSON-RPC 2.0 over UDS/named pipes with a token and code-signature peer auth.
  Bulk data goes through the store.
- Each SQLite database has exactly one writer process. Everything is encrypted (SQLCipher +
  AES-GCM, with the DEK wrapped by Keychain/DPAPI).
- Muse isn't bundled. We use the user's install with `MUSE_NO_AUTO_UPDATE=1`,
  `MUSE_LOGIN=0`, an isolated config dir and a fingerprint check.
- The API key goes to the engine on stdin and to muse/PTY through the child env only. It
  never goes in argv or the shell's own env, and we never write to `muse auth`.
- There is one muse host per sandbox posture (builder vs read-only explain). Approvals are
  rendered natively.
- `Builder` trait implementations: MuseServe (primary), MuseExec, DirectMeta (Windows safety
  net).
- The embedded terminal is xterm.js + portable-pty on the same git workspace as the Guided
  builder.
- Web micro-apps run in `mapp://` windows with per-window Tauri capability ACLs generated
  from their manifest.
- Updates: `tauri-plugin-updater` >= 2.12.0 plus a minisign-signed manifest with no
  downgrades. Developer ID + notarization. NSIS per-user + Azure Trusted Signing.
- Not on the Mac App Store.
- CI: macOS + Windows + Ubuntu hosted. Windows gets WebDriver E2E and screenshot artifacts.
  Nightly live tests run on a self-hosted Mac.
