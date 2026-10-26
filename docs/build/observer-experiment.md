# macOS metadata observer experiment

Experiment date: 2026-10-27. Tested on macOS 26.5, arm64, Apple Swift 6.3.3.
Package: [`native/macos-observer`](../../native/macos-observer/Package.swift),
Swift tools 6.0, deployment target macOS 15 or later.

This is a child-process experiment for one explicitly started Show session. It does
not enable Show in the product or establish the shipping gates in plan §5 and
§8.3 P1/P2. The connector title matching is the narrow, on-device metadata path
requested in ADR 0001 §2 and Task 07. It does not authorize browser pixels, DOM
access, browser routes, or the broader capture implementation in plan §5.2.

## What it captures

The executable is `macroscope-observer`; its implementation is in the testable
`ObserverCore` library. Nothing is observed until a valid `start` control arrives
on stdin. Controls and events use UTF-8 newline-delimited JSON, at most 65,536
bytes per line including LF. Every event has `v: 1`, `sessionId`, increasing
`seq`, session-relative monotonic `monoMs`, and the supplied `policyEpoch`.

| Event | Source and meaning |
| --- | --- |
| `ready` | Current `AXIsProcessTrusted()` result, emitted before observations. |
| `app_focus` | Bundle ID and localized application name from NSWorkspace. Activation notifications trigger sampling; initial/resume snapshots and a two-second poll can also identify changed focus. This is application state, not proof of a click, save, export, or business action. |
| `connector_match` | Connector ID, signal kind, and a generated positional rule ID, such as `gmail:titleSuffixes:0`. Bundle rules apply to native apps; title rules apply only to each connector's explicitly listed browsers. Rules are case-sensitive. A match is a local signature hint, not proof of authentication or account identity. |
| `gap` | `paused`, `excluded`, `ax_unavailable`, `secure_input`, `session_inactive`, or `timeout`. Missing frontmost-app metadata also yields `session_inactive`; that gap does not prove the screen is locked. |
| `stopped` | `user`, `max_duration`, `eof`, or `error`, followed by normal process exit. |

Titles are queried only for browsers with configured title signatures, only while
Accessibility is granted, and only after exclusion, session, pause, and secure
input checks. The adapter reads `kAXFocusedWindowAttribute` and then
`kAXTitleAttribute`, sets a 0.25-second messaging timeout on both elements, and
rechecks trust, focus identity, secure input, authority, and the session deadline
between reads. Titles over 8,192 UTF-16 code units are rejected before copying to
a Swift String. The title is matched locally and discarded; it has no field in
the encodable event type. Identical connector/rule matches are emitted at most
once per 30 seconds, including across pause/resume.

Exclusions are the union of the supplied bundle IDs and these defaults:

- Password tools: `com.1password.1password`, `com.agilebits.onepassword7`,
  `com.bitwarden.desktop`, `com.apple.keychainaccess`, `com.apple.Passwords`.
- Terminals: `com.apple.Terminal`, `com.googlecode.iterm2`, `dev.warp.Warp-Stable`.
- Messaging: `com.apple.MobileSMS`, `net.whatsapp.WhatsApp`, `ru.keepcoder.Telegram`.
- Synthetic finance/health placeholders: `com.example.finance`, `com.example.health`.

Only the bundle ID is obtained before the exclusion check. For excluded apps,
there is no process-detail, app-name, or title read and only an `excluded` gap is
emitted. The placeholders do not recognize real finance/health apps; the host
must supply their bundle IDs. This denylist is not a universal privacy detector.

## What it never captures

No ScreenCaptureKit, pixels, screenshots, OCR, editable AX values, selected text,
document contents, URLs, browser DOM, keystrokes, mouse activity, event taps,
clipboard, or Apple Events. There are no network calls, credential reads, files
opened for writing, or persisted evidence in the helper. Application names and
bundle IDs are the explicitly admitted metadata; raw titles and matched pattern
text never appear in event JSON. Match rule IDs contain positions, not title text.

Stdout carries only events. The helper emits no external stderr diagnostics.
NSWorkspace can produce framework diagnostics when services are unavailable in
the sandbox; stderr is routed to a bounded anonymous pipe and discarded inside
the process, without opening a file or forwarding framework text. Test and
manual-run transcripts under ignored `.build/` are written by the verification
harness, not the observer.

## Permission and lifecycle behavior

Default execution checks Accessibility without prompting. Only an explicit
`--request-accessibility` flag permits `AXIsProcessTrustedWithOptions` with the
prompt option, after receiving `start`. This flag was not exercised. There is no
Screen Recording or Input Monitoring permission API in the experiment.

Denied or revoked AX trust prevents title reads; native bundle matches and app
focus metadata remain available when NSWorkspace is available. Trust is checked
again for each title sample. Secure input closes the observation gate. Session
resignation, screen sleep, and system sleep yield inactive gaps; session/screen
reactivation reopens only the corresponding gate. Pause/resume do not reset the
session deadline. Notifications and reads are serialized on the main run loop.

The stdin reader uses a separate serial queue and a lock-protected mailbox with
at most eight pending controls and one scheduled drain callback. Enqueueing any
control suspends capture admission. EOF permanently removes authority immediately
on detection, including during an AX read; in-flight results are discarded at the
authority recheck. An already executing AX call can take up to its 0.25-second
timeout to return. A partial final line is discarded at EOF.

`maxSeconds` must be positive and is capped at 300. A separate timer and deadline
checks before/between/after reads stop observation even while paused. Scheduling
or a bounded in-flight AX call can delay delivery of the terminal event; results
past the deadline are dropped. One valid start is permitted per process; a second
start or invalid/oversized subsequent input stops with `error`. Invalid input or
EOF before any valid start exits quietly because no session ID exists yet.

Owned buffers are bounded: one incomplete input line, eight queued controls,
128 connectors/1,024 signature entries, a debounce table bounded by the rules,
and 256 KiB of output buffering including reserved terminal-event space. Slow
stdout uses nonblocking writes; saturation stops the session with `error`.
Shutdown allows at most 0.5 seconds to flush, then exits even if the parent has
stopped reading. A disconnected/stalled output pipe can prevent delivery of the
terminal event; the host must also handle process exit. No disk fallback exists.

## Verification results

The requested `cd native/macos-observer && swift build -c release && swift test`
was attempted. **FAIL (environment, exit 1):** the outer sandbox prevents
SwiftPM's nested sandbox from starting; `swift test` is skipped by `&&`. Without a
module-cache override, Swift also tries an unwritable cache outside the workspace.
The decisive failure tail, after moving the module cache into the package, was:

```text
sandbox-exec: sandbox_apply: Operation not permitted
```

The following equivalent build/test invocation keeps caches inside the package
and disables only SwiftPM's nested build sandbox, under the existing outer
sandbox. It does not change compiler checks, tests, or observer behavior.
`CARGO_TARGET_DIR` was left untouched.

```sh
cd native/macos-observer
export CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache"
export SWIFTPM_MODULECACHE_OVERRIDE="$PWD/.build/ModuleCache"
swift build -c release --disable-sandbox \
  --cache-path .build/swiftpm/cache --config-path .build/swiftpm/config \
  --security-path .build/swiftpm/security
swift test --disable-sandbox \
  --cache-path .build/swiftpm/cache --config-path .build/swiftpm/config \
  --security-path .build/swiftpm/security
```

**PASS (both exit 0).** Release build and test output tails:

```text
[5/7] Write Objects.LinkFileList
[6/7] Linking macroscope-observer
Build complete! (5.62s)
```

```text
✔ Test EOFRevokesCaptureAndExitsQuickly() passed after 0.086 seconds.
✔ Test maxDurationExitsWhileStdinRemainsOpen() passed after 2.080 seconds.
✔ Test pauseResumeAndUserStop() passed after 0.086 seconds.
✔ Test EOFDiscardsPartialControlLine() passed after 0.081 seconds.
✔ Test malformedAndOversizedInputStopWithError(oversized:) with 2 test cases passed after 0.163 seconds.
✔ Suite SubprocessTests passed after 2.498 seconds.
✔ Test run with 39 tests in 4 suites passed after 2.498 seconds.
```

Tests cover all six listed browsers, native/non-browser negative cases including
Apple Mail, exact and empty patterns, independent rules/connectors, every default
exclusion and a custom exclusion with read spies, denied/revoked synthetic AX,
secure input, independent session/screen gates, pause/resume, 30-second debounce,
the 300-second cap, in-flight authority/deadline revocation, and event encoding of
an email-bearing synthetic title. Framing tests exercise chunking, the exact
64-KiB boundary, invalid JSON, incomplete lines, and mailbox bounds. Subprocess
tests run the built debug executable and validate ready-first ordering, every
line as JSON, the envelope, increasing sequence numbers, monotonic timestamps,
terminal reason, exit status 0, and empty stderr. The EOF case requires exit
within 1.5 seconds after closing stdin; the max-duration case leaves stdin open.

### Five-second manual release run

The release binary received a start message with `maxSeconds: 5`, the Gmail
signature subset, all six browser bundle IDs, and policy epoch 1. Stdin remained
open until the child exited. **PASS:** exit 0, elapsed 5.444 seconds including
launch, five valid JSON lines with increasing sequences, zero stderr bytes.

Observed event sequence (JSON keys reordered for readability):

```json
{"v":1,"sessionId":"manual-five-seconds","seq":1,"monoMs":0,"policyEpoch":1,"type":"ready","permissions":{"accessibility":"denied"}}
{"v":1,"sessionId":"manual-five-seconds","seq":2,"monoMs":4,"policyEpoch":1,"type":"gap","reason":"session_inactive"}
{"v":1,"sessionId":"manual-five-seconds","seq":3,"monoMs":2012,"policyEpoch":1,"type":"gap","reason":"session_inactive"}
{"v":1,"sessionId":"manual-five-seconds","seq":4,"monoMs":4013,"policyEpoch":1,"type":"gap","reason":"session_inactive"}
{"v":1,"sessionId":"manual-five-seconds","seq":5,"monoMs":5008,"policyEpoch":1,"type":"stopped","reason":"max_duration"}
```

Accessibility was denied, with no prompt observed. No `app_focus` events were
available: this sandbox did not expose a frontmost application. No title reads
or connector detections were demonstrated against a live app. Missing access
was represented by gaps rather than invented events.

## UNVERIFIED — required before exposing Show

- Signed-bundle/helper TCC attribution to Macroscope and the packaged app's actual
  Accessibility grant flow, including the explicit prompt flag, Settings return,
  relaunch, and fresh-account behavior (plan §5.2/5.3).
- Live Accessibility revocation while a packaged session is active; only synthetic
  trust loss and authority revocation are covered here.
- N-1 signed application/helper update and retained/revoked permission behavior.
- macOS 15 runtime behavior. The deployment target compiles, but this run used
  macOS 26.5 only.
- Live focus/tab changes and AX reads in all claimed browsers/native apps, secure
  input, notification loss, sleep/wake, fast user switching, startup while locked,
  and physical lock detection. Session inactivity is not validated lock detection.
- Signed real-app latency, CPU/memory budgets, hung AX providers, output
  backpressure stress, and complete packaged pause/revocation barrier races.
- The wider Show UI, selected-window scope, host policy-epoch invalidation,
  session evidence encryption/deletion, and remaining §5 and P1/P2 ship gates.
  This metadata experiment is not a substitute for those gates.

## Reviewer run (Claude, 2026-10-27, outside the Codex sandbox)

- `swift build -c release` and `swift test` pass outside the agent sandbox (SwiftPM's own
  `sandbox-exec` cannot nest inside Codex's sandbox, which is why the worker had to use
  `--disable-sandbox`).
- Live session while the Mac was locked: the frontmost process was `com.apple.loginwindow`,
  and the first build reported it as an `app_focus` event. That is a lock boundary, not user
  work (plan §5.2: lock closes the content gate). Fixed: `loginwindow` and the screen saver
  now produce `gap: session_inactive` without reading the app name or title, with a test.
- Still unverified on this machine: Accessibility-granted title matching against a real
  Gmail tab (needs the owner to grant Accessibility to the signed app), and app switching
  while unlocked.
