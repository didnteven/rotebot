# Record a task — Stage C

Stage C connects the existing memory-only recording/review/consent flow to a
macOS child process. The implementation is built and tested with synthetic
inputs. **Signed-app capture, permissions and live geometry are not verified in
this sandbox.** This document is a manual acceptance script, not evidence that
those shipping gates passed.

## Components and authority

- `native/macos-observer` now builds `macroscope-capture` and the testable
  `CaptureCore` library. The metadata observer remains a separate executable.
- `CaptureProcess` checks permissions without prompting. Only an explicit
  `request_permission` command calls the Screen Recording and Accessibility
  prompting APIs. It does not capture while requesting access.
- `start` presents `SCContentSharingPicker` in **single-window mode**. The helper
  uses only the filter/window returned by that picker; it never calls
  `SCShareableContent` enumeration or a display-capture fallback. Filter changes,
  multiple selections, and missing selected-window identity stop the process.
- macOS **15.2+** is required for real capture: `SCContentFilter.includedWindows`
  became available in 15.2. The package/app still deploy to 15.0, but 15.0–15.1
  report capture unsupported rather than enumerate windows to recover identity.
- The observer's default exclusions are reused, with Macroscope, security agents,
  additional password managers, and all System Settings added. Denying all System
  Settings avoids trying to classify individual security panes.
- Known browsers, apps declaring HTTP/HTTPS URL handling, and apps declaring the
  web-browser category are treated as browsers. No supported browser exposes a
  trusted public/private attestation here: **all such browser windows are refused
  with `private_or_unknown_window`**, including ordinary browser windows. Neither
  title nor AX heuristics can authorize a browser. App classification and a bundle
  deny-list are not a universal detector of browsers/password tools.

The Rust host discovers the bundled `macroscope-capture` beside the desktop
executable, then the prepared development sidecar; `MACROSCOPE_CAPTURE` is a
trusted development override. `prepare-capture.mjs` builds and copies the target
architecture sidecar, `tauri.conf.json` bundles it, and `verify-bundle.sh` checks
its presence/executable bit. The app's `Info.plist` includes Screen Capture and
Accessibility usage descriptions. No new entitlements were added.

`recording_status` reports `captureAvailable`, `helperAvailable`,
`screenRecordingGranted`, `accessibilityGranted`, `pickerState`, picked `appName`
and a bounded reason code. `captureAvailable` requires a successful helper
handshake, supported OS, and both permissions. Status probes do not start capture
or prompt. Only `request_recording_permission` requests access; it is enrolled in
the handler, build manifest and main-window capability. Missing/broken helpers
leave the sample flow usable. The UI keeps the sample choice even when real
capture is available.

## Pixels, OCR and masking

SCStream requests at most one frame per second and sends only complete frames
that differ from the last admitted luma-grid sample. There are no input event
monitors, event taps, keystrokes, click coordinates or clipboard access. A static
window produces no extra frames. Small or transient edits can be missed by this
cheap sampling strategy.

Frames scale to a maximum 1600-pixel long side, with an additional area reduction
to fit the 8 MiB wire budget (including header reserve). Shadows, cursor/click
highlights, child windows, system audio and microphone capture are explicitly
excluded. The whole window maps to the whole buffer; after a resize the
buffer can stretch rather than introducing unaccounted letterboxing.

Vision accurate text recognition runs on-device on the same image used for RGBA
output. Normalized bottom-left boxes become top-left pixel boxes, rounded outward
and clamped. Missing/oversized results, invalid geometry, low-confidence results,
or OCR errors mark the frame uncertain. OCR is not a complete sensitive-data
detector: unrecognized text and novel/private data can still be missed.

AX is inspected only when trusted, only for the picked process's focused window,
and only when its exact CGWindowID matches the picker-selected window. The
optional `AXWindowNumber` attribute is tried first, then the OS's dynamically
resolved `_AXUIElementGetWindow` bridge. That bridge is **private SPI**, not a
stable API promise or an App Store compatibility claim. If neither works, the
frame is uncertain; titles/positions never substitute for window identity.
Changing focus to another window of the same app therefore drops frames instead
of borrowing that window's labels.

AX window bounds must match the SCStream sample's screen rectangle. Visible
controls supply bounded AXTitle/AXDescription/AXValue strings; window-root title
attributes are not read. Secure-field values/children are never read. Secure
roles/subroles produce `secure_field` mask rectangles, mapped from screen points
to captured pixels. Missing geometry, bounded traversal exhaustion, AX errors,
stale samples (>250 ms on receipt), changed AX evidence before/after OCR, or a
processing interval over one second mark uncertainty. Secure input is checked at
receipt and around processing, generates a `secure_input_gap` flag/gap, and causes
the Rust gate to drop the frame. These checks need live alignment/race testing;
rapid secure-input transitions between checks are not an event-level guarantee.

The helper disables core dumps, contains stderr in an anonymous draining pipe,
and installs a process sandbox denying `network*` before permission/capture APIs.
No URLSession or other networking calls are implemented. Sandbox installation
failure closes the helper before capture; already-sandboxed launches may fail.
This uses the deprecated macOS sandbox API and requires signed-bundle validation
on every supported OS. There is no uncontained capture fallback.

The helper never writes frames/text to files, logs, reports or pasteboard. Raw
RGBA/OCR/AX pass only through the anonymous child pipe to Rust. The host builds a
`RecordedFrameDto` with an empty window title and calls **`AppCore::submit_frame`
directly**. It releases the host lock before acquiring `submit_frame`'s local
lifecycle admission. Stop closes the recording
slot under its lock, so it either waits for in-flight masking or rejects a late
submission. The slot also rechecks its native policy epoch at ingress, so a
previously decoded frame cannot cross a policy change. This avoids a stop/start
lock inversion. There is still **no `submit_frame` webview command**.

Stage A/B remain authoritative: uncertainty and secure-input frames are dropped;
opaque masks are burned before PNG; sensitive/overlapping OCR and AX metadata
are cleared; window-title metadata has no helper event field; kept pixels stay in
memory. Only masked PNGs are returned to the renderer. Preview, payload hashing,
explicit consent, exact reviewed packet, model routing and the existing draft
approval path are unchanged. `service/transport.rs` was not changed. A real native
session cannot acquire the sample analyzer's synthetic authority.

## Wire protocol and lifetime

Both directions use:

```
[u32 big-endian JSON byte length][UTF-8 JSON header][payloadBytes raw bytes]
```

The header is 1–65,536 bytes with `v: 1` and explicit `payloadBytes` on the wire.
All host controls include the common envelope; unbound stop/permission controls
use an empty session ID and policy epoch zero. JSON plus pixels is at most
8 MiB. Frame dimensions, byte counts, version, required fields and event kinds
are checked before allocating/accepting pixels. Unknown event fields (including
`windowTitle`) are rejected by Rust. Partial packets/EOF close admission; no
packet contents are included in errors.

| Direction | Types and fields |
| --- | --- |
| Host → helper | `start {sessionId, policyEpoch}`, `stop`, `pause`, `request_permission` |
| Helper → host | `ready {captureSupported, screenRecordingGranted, accessibilityGranted}`; `picked {appName}`; `frame`; `gap {reason}`; `stopped {reason}` |
| Every event | `v`, `type`, `sessionId`, `policyEpoch`, `payloadBytes` |
| `frame` header | `seq`, `monoMs`, `appName`, `width`, `height`, `axLabels`, `ocrBoxes`, `masks`, `uncertain`, `secureInputGap` |
| `frame` body | Exactly `width * height * 4` RGBA bytes; never PNG/unmasked disk staging |

The initial ready event uses empty session ID/epoch zero. After start, events
must match the host's session and policy epoch. Only one picker grant/session is
allowed per process. Pause is terminal for this protocol version: resume requires
a new recording and new picker grant. The input mailbox is bounded; queued stop,
pause, malformed input and EOF revoke capture authority before in-flight OCR/AX
results can be emitted. Output backpressure has a 0.5-second budget and exits
without corrupting framing or writing a disk fallback.

The helper caps candidate frames at 60 and session duration at ten minutes,
including picker time. Sleep, session resignation, picker cancellation, stream
failure and Screen Recording revocation stop it. AX revocation makes frames
uncertain rather than bypassing masking. The Rust host independently limits
process lifetime, validates session/epoch for ingress, and kills/reaps the child
on Stop, discard, EOF/protocol failure, Local-only/egress-policy change, deletion,
and app exit. A 50 ms watchdog catches policy changes and child exit even with a
partial/stalled packet. Stop kills/reaps and closes the recording-slot ingress barrier before review can
proceed.

**Existing stricter limits remain:** Stage A closes frames after five minutes of
monotonic session time; Stage B admits at most 60 kept frames, 150 attempts and
64 MiB cumulative RGBA (including dropped frames). Large windows can hit that byte
budget after only a few frames. These limits were not raised to reach the helper's
ten-minute ceiling. Review remains available for already-kept frames.

## Reproducible verification

All Cargo commands use:

```sh
export CARGO_TARGET_DIR="/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-claude-targets/stage-c/cargo"
export CARGO_INCREMENTAL=0
export CARGO_NET_OFFLINE=true
```

Swift scratch/cache locations are on the same SSD. In the agent sandbox,
SwiftPM's nested sandbox must be disabled; this affects the compiler invocation,
not the capture helper's runtime network containment:

```sh
SWIFT_SCRATCH="/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-claude-targets/stage-c/swift"
export CLANG_MODULE_CACHE_PATH="$SWIFT_SCRATCH/ModuleCache"
export SWIFTPM_MODULECACHE_OVERRIDE="$SWIFT_SCRATCH/ModuleCache"
swift build --package-path native/macos-observer --scratch-path "$SWIFT_SCRATCH" \
  --disable-sandbox --cache-path "$SWIFT_SCRATCH/cache" \
  --config-path "$SWIFT_SCRATCH/config" --security-path "$SWIFT_SCRATCH/security"
swift test --package-path native/macos-observer --scratch-path "$SWIFT_SCRATCH" \
  --disable-sandbox --cache-path "$SWIFT_SCRATCH/cache" \
  --config-path "$SWIFT_SCRATCH/config" --security-path "$SWIFT_SCRATCH/security"
(cd apps/desktop && npx tsc --noEmit && npx vitest run --configLoader runner --no-cache)
cargo test -p macroscope-app-core --all-features
cargo test -p macroscope-desktop --test command_registration
```

The bundling script accepts `MACROSCOPE_SWIFT_SCRATCH` to relocate the scratch
root and `MACROSCOPE_SWIFT_DISABLE_SANDBOX=1` for this nested-build environment.
It defaults to the SSD path above; no worktree `.build` directory is created.
The observer subprocess test harness was corrected to discover its executable
beside the test bundle when using an external scratch directory.

Synthetic tests cover RGBA row alignment with Vision's top-left boxes,
Vision coordinate conversion/clamping, secure-field pixel
mapping, changed/stable frame differences, stale window geometry, packet
round-trips/truncation/oversize rejection, exclusions and private/unknown browser
refusal. Rust tests decode synthetic binary frames, reject version/title/size
violations, kill/reap a fake subprocess, route fabricated native frames through
masking, reject stopped ingress/sample authority, and stop on Local-only/epoch
changes. Frontend tests verify permission requests occur only on the explicit
button, request the window source, and wait for explicit review consent.

Verification on this worktree:

- Swift build: passed; Swift tests: **47 passed**, including the seven new capture
  tests and existing observer suites. No real capture is performed by these tests.
- TypeScript: passed. Vitest: **162 passed** in 21 files.
- Desktop command registration: **1 passed**; the packaged sidecar inputs were
  prepared with both executables built on the external SSD.
- Full app-core all-features run, also continued with `--no-fail-fast`: **134
  passed, 81 failed, 1 ignored**. Failures are existing localhost mock-server
  fixtures denied a listening socket by the sandbox (`Operation not permitted`),
  including the subprocess test that invokes such a fixture. No network/sandbox
  escalation was requested. The suite must be rerun outside this restriction.
- Final app-core library run excluding that one localhost fixture: **81 passed**
  (one additional permission-envelope regression test was added after the full run). All
  new native/protocol/masking/stop/Local-only tests passed.
- `git diff --check`: passed. No edits to `checks.rs` or provider schema conversion.

Logs for this execution are under `/tmp/macroscope-stage-c-{swift-build,swift-tests,
vitest,rust-tests,rust-offline-tests,desktop-tests}.log`. They contain only build
output and synthetic-test diagnostics; no native recording was run.

## Manual acceptance script — signed Mac app, macOS 15.2+

Do this outside the agent sandbox with a signed app and its signed sidecar. Do
not paste real secrets or use production sensitive data for the checks.

1. Build/package using the normal desktop build with the exported Cargo output
   settings above. Run `apps/desktop/scripts/verify-bundle.sh /path/to/Macroscope.app`.
   Verify the capture sidecar is signed consistently with the app. On a fresh test
   account, launch Macroscope: **no Screen Recording/AX prompt should appear**.
2. Open Record a task. Confirm missing-permission guidance and the sample choice.
   Press **Allow recording access**. Grant access through the system prompts or
   open **System Settings > Privacy & Security > Screen Recording** (newer macOS
   may call it Screen & System Audio Recording), switch **Macroscope** on, and do
   the same under **Accessibility**. TCC attribution must name Macroscope; record
   any separate helper attribution as a failed packaging test. Relaunch if macOS
   requests it. Return and press **Try again**. The Choose button should appear.
3. Turn off Local-only, keeping the intended model route selected. Open TextEdit
   with a modest window (~800×600) containing fabricated notes. Open a second,
   visibly different TextEdit document so that wrong-window capture is obvious.
4. Press **Choose a window to record**. The macOS picker must offer one window
   only, never a whole-display/app selection. Pick the first TextEdit document.
   The UI may show **TextEdit**, but must never show its window title as metadata.
5. Perform three steps, pausing two seconds between them: type a short fabricated
   order list; add a total/summary; replace a heading. Include `alice@example.com`
   and synthetic test card `4111 1111 1111 1111` in visible text. Wait after each
   change. Kept-frame counts should increase, and a static window should not
   continually add frames. If all frames drop, treat AX identity/geometry/OCR
   validation as failed; do not relax uncertainty checks to make this test pass.
6. Click **Stop**. Confirm the capture indicator/child process ends immediately
   and the existing review screen opens with **real masked TextEdit images**.
   Inspect every image at readable size: synthetic email/card must be blacked
   out, unrelated text must remain readable, and the second document must not
   appear. Inspect cleared text: no email/card or window-title metadata. Nothing
   should have been sent yet. Check recipient, model, size, instructions/schema
   and cost caveat. Discard if any private content remains.
7. With an already-configured image-capable API/custom route, press **Send for
   analysis** once. Confirm the 2–4 questions and grounded draft appear. Choose
   every answer and press **Looks right**. Confirm the normal Check the plan
   screen appears. No provider/schema conversion changes are needed for Stage C.
8. Start another session, cancel the picker, and verify stopped status with no
   frames. Try Macroscope/password managers/System Settings: they must be
   excluded or refused. Pick both ordinary and private browser windows: both must
   refuse with `private_or_unknown_window`, with no kept frames.
9. Use a dedicated native test window with a secure AX text field, initially
   without secure-input mode: verify the field rectangle is fully black at each
   frame edge, after window moves/resizes, on Retina/non-Retina and negative-origin
   secondary displays. Toggle secure input: affected frames must drop. Revoke AX
   while recording: later frames must drop. Revoke Screen Recording: capture must
   stop. Restore permissions and start a fresh picker session.
10. With two native windows of one app, switch focus to the unpicked one: no labels
    or secure-field geometry from it may be used. Test scrolling/changing layouts,
    resize, occlusion, minimization, window close, display scaling and OCR failure.
    Stale/missing geometry must yield dropped frames, never guessed masks.
11. During active capture, enable Local-only; change credentials so the egress
    epoch changes; discard; sleep/lock; quit; terminate the helper. Each applicable
    test must end capture, leave ingress closed, and require a new picker grant.
    Change model settings after review: stale consent must still fail.
12. Observe 60-frame, cumulative-byte, five-minute Rust and ten-minute helper caps.
    Check that no capture frame/text files or outbound rows appear before consent.
    Verify the helper's network-deny sandbox and absence of capture-content logs
    in the packaged application. Test stdout backpressure and parent EOF in a
    harness that retains only fabricated inputs, not real screenshots.

Still required before claiming shipping readiness: these live tests, signed
helper/TCC attribution, the optional AX bridge and sandbox behavior on supported
OS versions, display/secure-field alignment, secure-input transition races,
resource/latency measurement, and a full real-window-to-model manual acceptance
run. This sandbox cannot grant the necessary macOS permissions.
