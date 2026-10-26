# 06 - Capture layer on macOS

Research date: 2026-10-22. Scope: planning only; no capture code or permission-changing probes were run. `plan/BRIEF.md` was read fully. **[local: checked in workspace]**

Evidence convention: **[verified: URL]** identifies external documentation or a first-party report; a report verifies that the problem was reported, not that every Mac reproduces it. **[assumption]** marks proposed product behavior, estimates, and engineering judgments. All numeric budgets below are acceptance targets, not measurements.

## Recommendation and architecture

**[assumption]** Build a small, unsandboxed Swift observer, shipped inside a Developer ID-signed, notarized desktop app. Support macOS 15 and 26 explicitly. Prefer structured browser events, then Accessibility structure, then occasional redacted screenshots. Observe only the foreground allowed application/window. Do not record audio, clipboard contents, typed characters, background tabs, or all displays by default. This trades comprehensive replay for a defensible, inexpensive workflow signal.

```text
NSWorkspace ---- app/session/Space transitions -----------+
AXObserver ----- focus/window/structure changes ----------+
CGEventTap ----- click + aggregate typing/shortcut hints -+--> Swift observer
Browser extension -- permitted visible-page events ------+       |
SCStream ------- candidate pixel buffers ----------------+       v
                                                        policy/privacy gate
                                                               |
                                             mask -> diff -> local Vision OCR
                                                               |
                                            sanitize -> bounded encrypted spool
                                                               |
                                          versioned IPC -> desktop pattern engine
                                                               |
                                         selected sanitized evidence -> Meta API
```

**[assumption]** The observer has no model API key, model client, shell execution, or automation executor. Its IPC offers `setPolicy`, `start`, `pause`, `deleteRange`, `health`, and sanitized evidence delivery. Validate the peer's signing identity through an authenticated XPC connection, not just a PID. Keep raw pixels and raw AX strings inside the observer's privacy boundary. If the main app crashes, capture continues only if the user explicitly enabled background observation; otherwise stop when its authorization lease expires.

**[assumption]** Each event carries schema version, sequence, monotonic timestamp, wall-clock anchor, foreground generation, policy generation, source, confidence, and explicit missing-data reasons. Window IDs are session-scoped hints. Frames, OCR, and AX snapshots share a capture ID and coordinate transform. A capability loss produces a visible gap, never an inferred action. A screenshot containing a button is evidence of state, not proof the user clicked it.

## ScreenCaptureKit: narrow capture, bounded work

ScreenCaptureKit supplies displays, applications, and windows through `SCShareableContent`; `SCContentFilter(desktopIndependentWindow:)` selects a single window. Display filters can exclude applications, including the recording app. Streams can update their configuration and filter while running. **[verified: https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos]**

**[assumption]** Default to one `SCStream` following the focused allowed native window. Resolve AX window identity to `SCWindow` by owning PID and geometry; treat title matches as supporting evidence only. Public cross-framework identifiers are not assumed equivalent. Ambiguous matches, minimized windows, missing focus, or stale geometry mean no pixels. Desktop-independent capture can reveal more than the visible desktop experience; therefore stop accepting frames as soon as the window loses foreground status, even if its stream remains technically capturable.

**[assumption]** Per-window capture avoids unrelated notifications and adjacent apps, but may omit menus, sheets, and separate panels. Correlate those through AX first. Per-display capture is an explicit, time-limited troubleshooting/demo mode, with stronger warnings and the same privacy rules; it is not a silent fallback when window matching fails. Never capture all monitors merely because several are attached.

**[assumption]** Exclude every Macroscope UI/helper application identity and current PID from any display filter; update the exclusion set after helper relaunch. In window mode never select our windows. Stop observation while our UI is frontmost. Picker exclusions complement stream filtering; they do not replace it. Test excluded windows both initially and after filters change.

`SCStreamConfiguration` controls dimensions, pixel format, cursor, and capture interval. Apple's sample uses a queue depth of five, identifies three as the default, and advises staying at or below eight. **[verified: https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos]**

**[assumption]** Start with SDR, BGRA, cursor hidden, audio/microphone disabled, maximum long edge 1,600 pixels, `queueDepth = 3`, and `minimumFrameInterval = CMTime(value: 1, timescale: 2)` for at most two delivered frames/second. This interval is a throttle, not a promise of exact cadence. Permit a short 4 fps experiment only if needed to capture transient workflow state. Do not write a movie or use `SCRecordingOutput` for observation.

Frame metadata includes status, dirty rectangles, content rectangle, and scaling information. Dirty rectangles describe changed areas, while content rectangle and scales locate window content within the output buffer. **[verified: https://developer.apple.com/documentation/screencapturekit/scstreamframeinfo/dirtyrects]** **[verified: https://developer.apple.com/videos/play/wwdc2022/10155/]**

**[assumption]** Accept only valid, complete samples. Release buffers promptly; retain at most two pending candidates and replace stale work. Do not run OCR or compression on the stream callback queue. Use metadata to accelerate comparisons, but never treat missing/empty dirty rectangles as a universal guarantee of unchanged content. Rebaseline after resize, filter change, display reconfiguration, wake, or a coordinate-scale change. Discard frames from an obsolete foreground or policy generation, even if delivered after an asynchronous filter update.

macOS 26 has a dedicated `SCScreenshotConfiguration` surface, including SDR/HDR selection and HEIC/JPEG/PNG output types. The public Rust wrapper documents its macOS 26 availability. **[verified: https://developer.apple.com/documentation/screencapturekit/scscreenshotconfiguration]** **[verified: https://docs.rs/screencapturekit/latest/screencapturekit/]**

A macOS 26 AltTab incident report and maintainer investigation associate repeated `captureSampleBuffer()` screenshots with short-lived stream churn and WindowServer instability. This is a first-party project report, not an Apple-wide guarantee. **[verified: https://github.com/lwouis/alt-tab-macos/issues/5786]**

**[assumption]** Reuse a stream for ongoing observation. For manual still capture, evaluate the dedicated macOS 26 screenshot path behind availability checks; retain a separately tested macOS 15 path. Never set a screenshot file URL before redaction. Include back-deployment launch tests, since checking availability inside a method does not by itself prove that newer symbols link safely on older systems.

### Recurring prompts and the purple indicator

macOS 15 introduced a recurring direct-access confirmation, with wording about bypassing the system private window picker; TechSmith documents its “Allow For One Month” action and periodic recurrence. This is distinct from the initial Screen Recording grant. Apple's macOS 15 notes also warn about alerts from deprecated capture APIs. **[verified: https://support.techsmith.com/hc/en-us/articles/30302332343693-Snagithelper-2024-Is-Requesting-to-Bypass-the-System-Private-Window-Picker-and-Directly-Access-Your-Screen-and-Audio]** **[verified: https://developer.apple.com/documentation/macos-release-notes/macos-15-release-notes]**

Apple recommends `SCContentSharingPicker` for user selection. Its filters and callbacks support selection changes and cancellation. Apple separately offers a managed Persistent Content Capture entitlement specifically for VNC applications, subject to approval; MDM has a `forceBypassScreenCaptureAlert` restriction from macOS 15.1. **[verified: https://developer.apple.com/videos/play/wwdc2023/10136/]** **[verified: https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.persistent-content-capture]** **[verified: https://developer.apple.com/documentation/devicemanagement/restrictions]**

**[assumption]** Use the picker for an introductory “observe this window” session. Broad, focus-following observation is a separately explained opt-in and must tolerate recurring OS confirmations. Do not assume using ScreenCaptureKit eliminates every reminder, that a picker selection grants future access to arbitrary windows, or that the exact monthly cadence is a stable macOS 26 contract. Measure prompts on shipping 15.x/26.x builds and document their actual wording. Do not depend on receiving the VNC entitlement or ask consumers to install MDM profiles.

The purple screen-sharing menu indicator is observable in macOS capture products; Apple's release notes describe the purple window menu, and its support documentation describes the lock-screen observation reminder. **[verified: https://developer.apple.com/documentation/macos-release-notes/macos-15-release-notes]** **[verified: https://support.apple.com/en-lamr/120315]**

**[assumption]** Show the indicator during onboarding and explain that pixel access triggers OS recording UI even when most frames are discarded. Never hide or bypass it. Provide a persistent Macroscope menu-bar state with Pause, Pause for an hour, and Delete recent activity. On OS stop/revocation, stop and show “Screen capture stopped.” An independent registered pause hotkey is useful, but the clickable menu must work when secure input blocks keyboard observation.

## Accessibility structure and interaction signals

`AXUIElementCreateApplication`, attribute reads, hit testing, and messaging timeouts are public APIs. `AXObserverCreate` creates a per-application observer with a run-loop source and explicit notification registration. **[verified: https://developer.apple.com/documentation/applicationservices/1459345-axuielementsetmessagingtimeout]** **[verified: https://developer.apple.com/documentation/applicationservices/1460133-axobservercreate]**

**[assumption]** Maintain observers for the foreground application and temporarily its predecessor during a transition; remove observers when processes exit. Register supported focused-window, main-window, focused-element, window-created/destroyed, title-changed, moved/resized, and relevant value/layout notifications. Not every application supports every notification; record failures and use a slow foreground-only poll, initially once per two seconds when active. AX events trigger bounded rereads; they are not complete change descriptions.

**[assumption]** Read the app's focused window and focused element. Extract role/subrole, sanitized control label, enabled state, geometry, parent relationships, and a capped visible subtree. Start at 150 nodes, depth eight, 100 ms total scheduling budget and a 50 ms AX messaging timeout. Set the process-wide AX timeout explicitly or apply it to each queried object: Apple's documentation says setting it on one non-system object affects only that object. Abort traversal between calls; a deadline cannot cancel an AX call already blocked. Never query AX synchronously inside an input callback.

**[assumption]** Do not read editable `AXValue`, selected text, document body, or clipboard payloads. Window titles, document URLs, labels, and AX descriptions are also potentially sensitive; sanitize before persistence and use title fingerprints where literal text adds no workflow value. Retain structural hashes and capped deltas rather than repeated whole trees. For click targeting, perform an asynchronous `AXUIElementCopyElementAtPosition` and attach it only if the foreground generation still agrees.

`NSWorkspace.didActivateApplicationNotification` identifies the activated `NSRunningApplication` and must be observed through the workspace notification center. Session-active/inactive, application termination, Space changes, and wake notifications are also available there. **[verified: https://developer.apple.com/documentation/appkit/nsworkspace/didactivateapplicationnotification]**

**[assumption]** Use these notifications as the app-switch authority; do not infer switches solely from Command-Tab. Immediately close the pixel gate on a switch; reopen only after fresh policy, focused-window, and secure-field checks. Pause on session resignation, screen sleep, and system sleep. Detect lock state through a separately validated public-session signal; if lock state is uncertain, stay paused. Never save a “last frame” after the privacy gate closes.

### Timing without a keylogger

Apple distinguishes listen-only keyboard taps, which require Input Monitoring authorization, from modifying taps, which require Accessibility. `CGPreflightListenEventAccess` is a permission-check API. `NSEvent` global monitors cannot change events, omit the installing application's events, and run handlers on the main thread; its archived keyboard-monitor guidance references Accessibility trust. **[verified: https://developer.apple.com/videos/play/wwdc2019/701/]** **[verified: https://developer.apple.com/documentation/coregraphics/cgpreflightlisteneventaccess()]** **[verified: https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/MonitoringEvents/MonitoringEvents.html]**

**[assumption]** Prefer a `.cgSessionEventTap` with `.listenOnly` for clicks, scroll bursts, key-down timing, and modifier changes on a dedicated run loop. Mouse motion is unnecessary. Never modify, swallow, or synthesize input. The callback immediately reduces an event into a privacy-safe record; it never serializes `CGEvent`/`NSEvent`, Unicode strings, character fields, or ordinary keycodes.

**[assumption]** Count ordinary keys into one-second buckets, then retain burst duration and coarse cadence rather than individual timings. Recognize only an explicit shortcut allowlist, such as Copy, Paste, Save, Undo, Find, and Tab/Return navigation; discard unrecognized chords. Do not classify every Option combination as a shortcut because some produce text. Record Copy/Paste actions without clipboard content. Quantize click coordinates relative to the active window and prefer a sanitized control identity. Handle tap-disabled notifications with bounded re-enable attempts and a visible health state. Denial disables this source; it does not justify switching APIs to evade permission.

### Passwords and private windows

`IsSecureEventInputEnabled()` reports Secure Event Input. Apple explains that secure input prevents keyboard interception and can remain enabled when the enabling app is in the background. It protects keyboard delivery, not screenshot pixels. **[verified: https://developer.apple.com/library/archive/technotes/tn2150/_index.html]**

**[assumption]** Combine secure-input polling, focused AX secure subrole (`AXSecureTextField`), and browser password/autocomplete classifications. Any positive result pauses all content capture and input aggregates, drops pending work, and records only a generic protected interval. A false secure-input result is not evidence that a page contains no secrets. Custom controls, revealed passwords, and stale AX state remain privacy risks. Never try to disable another application's secure input.

Apple describes `NSWindow.SharingType.none` as legacy; it is not a reliable foundation for capture exclusion. Apple DTS also states that no public API prevents screen capture in response to a macOS 15.4+ question. **[verified: https://developer.apple.com/documentation/appkit/nswindow/sharingtype-swift.enum]** **[verified: https://developer.apple.com/forums/thread/792152]**

**[assumption]** There is no universal private-browsing detector in this design: ScreenCaptureKit window metadata and localized window-title guesses are insufficient evidence. Private browsing and capture-protected windows are separate concepts. Deny password managers, authentication dialogs, finance/health apps, and configured sensitive sites; scrub transient buffers on transitions. Default browser pixels and browser AX page-content traversal to OFF, including when the extension is absent. This removes the need to guess whether an unobservable browser window is private. An explicitly selected browser screenshot remains a separate manual action with a redacted preview.

**[assumption]** Mask all detected visible editable controls before OCR; skip frames when reliable field geometry is unavailable in strict mode. Use opaque fills, not reversible blur. Apply the same exclusions to AX, titles, URLs, OCR, and DOM events. Sensitive material can still appear in static text, images, custom canvases, or mislabeled controls. Therefore no “all secrets always removed” claim: broad cloud screenshot upload must remain disabled until fixture-based leakage tests and user review meet the privacy track's acceptance criteria.

**[assumption]** Hold candidates in a one-second memory quarantine and revalidate focus/privacy immediately before persistence. Privacy transitions invalidate this queue; this reduces asynchronous races but cannot make AX and pixels atomic. While any browser is frontmost, suppress native window titles, AX content, and global input records; accept only eligible extension events. Unknown browser context produces a content-free gap.

## Browser companion: the primary source for web workflows

Chrome content scripts can inspect permitted-page DOM in an isolated JavaScript environment. Host permissions gate access, and tab URL/title availability depends on permissions. Tab `active` does not imply its window is focused. Chrome supports explicitly prohibiting incognito execution with `"incognito": "not_allowed"`. **[verified: https://developer.chrome.com/docs/extensions/develop/concepts/content-scripts]** **[verified: https://developer.chrome.com/docs/extensions/develop/concepts/declare-permissions]** **[verified: https://developer.chrome.com/docs/extensions/reference/api/tabs]** **[verified: https://developer.chrome.com/docs/extensions/reference/manifest/incognito]**

**[assumption]** Yes: an extension is a better web-event source than screenshots. Start with Chromium MV3 and reuse its TypeScript event schema in Safari Web Extensions. Ask for optional access to selected work origins, not browsing history or every site. `activeTab` supports a manual demonstration; ongoing observation requires durable permissions for the approved origins. Native observation remains useful for desktop tools and cross-app transitions.

**[assumption]** Capture trusted click/focus/submit intent, route changes, tab/window activation, scroll bursts, and debounced structural mutations. Emit role, approved sanitized label, stable local selector fingerprint, normalized geometry, and input kind. Never emit field values, `InputEvent.data`, keyboard `key`/`code`, selection contents, hidden DOM, cookies, network bodies, storage values, or page HTML. A submit event is intent, not a confirmed server transaction. Handle SPA navigation and document replacement with a new document generation. Drop events from background tabs and unfocused browser windows.

**[assumption]** Sanitize URLs in the extension and again natively: remove userinfo, query and fragment by default; tokenize variable path segments with a keyed local hash. A path can contain a customer name or secret too. Send only origin/template and pseudonymous entity equality unless an app adapter explicitly permits more. Observe cross-origin frames only with their own granted origin; mark closed shadow DOM, canvas, internal browser pages, PDFs, and denied frames as coverage gaps. No debugger permission or remote-debugging port for v1.

Native messaging uses a registered host, length-prefixed JSON over stdio, and fixed `allowed_origins`; Chrome documents a user-specific macOS manifest location. Content scripts route messages through the extension service worker. Safari provides an app-extension/native messaging bridge. **[verified: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging]** **[verified: https://developer.apple.com/documentation/safariservices/messaging-a-web-extension-s-native-app]**

Apple explicitly supports Developer ID-signed, notarized Safari Web Extension distribution outside the Mac App Store. The companion therefore need not force the unsandboxed observer into App Store distribution. **[verified: https://developer.apple.com/documentation/safariservices/distributing-your-safari-web-extension]**

**[assumption]** Use a tiny signed native messaging adapter connected to the same observer, not a local unauthenticated HTTP server. Install the manifest only when enabling browser integration; support uninstall. Validate extension identity, message size, sender origin, schema, document generation, and policy version. Browser-controlled tab metadata is authoritative for URLs/private flags; page-supplied fields are untrusted. Validate labels and cap them. Add sequence acknowledgments and bounded reconnect queues; no unbounded content buffering while the desktop app is offline.

**[assumption]** Keep private mode disabled. Critically, absence of extension events does not attest that the current browser window is ordinary. Expire web-context leases on browser blur, navigation, disconnect, or uncertain native correlation; never attach stale normal-tab metadata to a private window. Because browser screenshots are off, private activity becomes a content-free gap. Browser window IDs are not assumed to equal `CGWindowID`.

**[assumption]** AX URL extraction is a compatibility adapter: inspect a known browser's focused document/address control only after normal-context eligibility is established. Do not read a live address bar while the user edits it. Optional AppleScript can ask supported Safari/Chromium scripting interfaces for the active document/tab URL, with timeout and per-browser consent. It is lower priority than the extension and must not execute page JavaScript. Exact scripting dictionaries and AX URL paths require versioned compatibility tests; do not invent a universal browser API.

## Frame selection, storage, OCR, and budgets

**[assumption]** Use the following ordered admission policy:

1. Check consent, pause/lock/secure/private/app rules, foreground identity, and policy generation. Reject before expensive work.
2. Normalize content coordinates, mask editable/sensitive rectangles, then compute a low-resolution luminance thumbnail, a 64-bit perceptual hash, and tile differences. Never persist hashes of unredacted content.
3. Treat Hamming distance above roughly six bits or meaningful changed tiles as a candidate, not final truth. Small text edits can defeat global hashes; AX/DOM semantic changes override visual deduplication.
4. Keep a settled frame after app/window switch, meaningful AX structure change, completed navigation, or click/shortcut followed by visual change. Debounce initially 300-700 ms, allow a second settled candidate near 1.5 seconds, and enforce a two-second routine minimum between retained frames.
5. Keep events when pixels are redundant. Coalesce animation/scrolling; do not turn video playback into an archive. Record elapsed durations independently of image retention.
6. After 60 seconds without observed interaction, stop periodic candidate processing. AX/DOM/app events can wake observation. If input permission is missing, use an explicitly lower-confidence idle heuristic. Lock/sleep stops streams immediately.

**[assumption]** A bounded latest-candidate queue and token budgets take precedence over completeness. A click may refer to the last accepted state and the next settled state; no raw rolling video buffer is necessary. Retained frame count and OCR work are independent: a new semantic event need not force either.

| Metric | Initial acceptance target [assumption] |
|---|---|
| CPU | Under 3% of one core averaged over a representative work hour, observer plus attributed OCR/encoding; measure incremental WindowServer/replayd cost separately |
| Idle | Under 0.5% of one core; zero stored content while paused/locked |
| Memory | Observer RSS below 200 MiB after warmup; bounded surfaces/queues; no overnight growth |
| Images | Typical six retained frames/minute or fewer, not a guaranteed rate |
| OCR | One worker, average at most one request/20 seconds; short bursts bounded by CPU tokens |
| Storage | Under 3 GB new data per 40-hour workweek; default rolling vault cap 3 GB and 500 MiB/day ingest cap |
| Latency | Metadata under 250 ms; selected redacted evidence normally within two seconds |

**[assumption]** At six frames/minute, 40 hours produce 14,400 images. At 100-150 kB/image that is 1.44-2.16 GB; reserve 0.5 GB for events, OCR, indexes, and encryption overhead. These sizes require measurement. Enforce actual bytes rather than claiming compression guarantees. At caps, retain semantic events and drop images, with a visible coverage reason. Resource pressure lowers sample rate and OCR first.

Image I/O provides image reading/writing and HEIC metadata facilities. **[verified: https://developer.apple.com/documentation/imageio]**

**[assumption]** Use HEIC through Image I/O for the Mac vault, initially quality 0.65-0.75 subject to text-legibility tests. Benchmark WebP via a pinned encoder for cross-platform interchange; do not assume native WebP writing or universal HEIC rendering in the desktop UI. JPEG/PNG are explicit export/preview conversions. Retain independent stills for deletion simplicity. Strip metadata; store encrypted random filenames. Compress only redacted pixels. OCR uses precompression redacted pixels to avoid lossy text errors.

Vision's `VNRecognizeTextRequest` supports fast and accurate recognition, runs on-device, and returns text observations with confidence and normalized geometry. Recognition language support depends on revision and configuration. **[verified: https://developer.apple.com/documentation/vision/recognizing-text-in-images]**

**[assumption]** Run `.fast` on selected changed regions when needed, `.accurate` on an admitted settled frame; prefer DOM/AX labels when available. Query supported languages; default to the user's selected languages. Preserve OCR revision, confidence, bounds, and source provenance. Convert Vision's normalized coordinates through the actual crop/scale transform and test mixed-DPI displays. Group lines spatially into labels, columns, and probable tables; mark uncertain structure rather than fabricating field semantics. Apply local token/PII/credential rules to recognized text and pixel boxes, then discard the temporary OCR result. OCR is an extra privacy pass, not proof of complete redaction.

**[assumption]** Default retention: redacted images and detailed event/OCR records for seven calendar days, aggregated pseudonymous workflow statistics for 30 days, raw candidate buffers for seconds in memory only. Encrypt records and images with separately managed keys protected by Keychain; lock the vault when the session locks. Exclude vault/spool from backups where supported and explain that deletion cannot recall user exports or historical external backups. Delete indexes, OCR, cached previews, and derived links together. Offer delete last 15 minutes, delete app/site, and delete everything. No screenshots or titles in diagnostic logs/crash attachments.

## Permission onboarding and revocation

**[assumption]** Present a useful demo before asking for permissions. Choose allowed apps/sites, explain local processing, and display a live sanitized evidence preview. Each permission has a separate button and Skip action. Accessibility comes before continuous native pixels so privacy geometry is available; Screen Recording is the final core setup step to accommodate relaunch. Input Monitoring and Automation remain optional later steps.

| Permission | What it unlocks and request/check | Denied/revoked behavior |
|---|---|---|
| Accessibility | Focused window/control, labels, geometry, AX notifications. `AXIsProcessTrustedWithOptions` for a user-initiated prompt; `AXIsProcessTrusted` plus a real bounded read for health. | No AX. Browser-extension mode works. Automatic native pixels remain off when strict privacy requires unavailable field geometry. |
| Screen Recording / Screen & System Audio Recording | ScreenCaptureKit pixels. `CGRequestScreenCaptureAccess` at the explicit step; `CGPreflightScreenCaptureAccess` for checks. | Structured-only mode; stop/release streams and drop pending pixels. Explain an OS-requested app relaunch where needed. |
| Input Monitoring | Listen-only keyboard/click timing source. `CGRequestListenEventAccess` and `CGPreflightListenEventAccess`. | No global typing/shortcut metrics. Keep AX/DOM semantics and app switches; label idle estimates accordingly. |
| Automation / Apple Events | Optional URL adapter for an individually selected browser. `AEDeterminePermissionToAutomateTarget` without prompting for checks; explicit per-target authorization when enabled. | Disable that adapter only. Extension remains preferred; never ask for unrelated apps. |

The permission APIs and target-specific Apple Events checks are documented by Apple; a potentially prompting Apple Events check can block its thread. Apple's capture sample explicitly anticipates restarting after granting capture permission. **[verified: https://developer.apple.com/documentation/applicationservices/1459345-axuielementsetmessagingtimeout]** **[verified: https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess()]** **[verified: https://developer.apple.com/documentation/coregraphics/cgpreflightlisteneventaccess()]** **[verified: https://developer.apple.com/documentation/coregraphics/cgrequestlisteneventaccess()]** **[verified: https://developer.apple.com/videos/play/wwdc2019/701/]** **[verified: https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos]**

**[assumption]** Track `unknown`, `requested`, `granted`, `deniedOrRestricted`, `needsRelaunch`, and `unhealthy` without pretending every Boolean permission API distinguishes never-asked from denied. Preflight on start/resume, return from Settings, wake, and periodically while active. Combine status with stream errors, event-tap health, AX operation results, and extension disconnects. Do not classify an AX timeout in one hung application as global revocation. A stationary screen without new frames is not proof of denial either.

Apple DTS recommends documented preflight/request APIs rather than inferring Screen Recording permission from missing window-list dictionary fields. **[verified: https://developer.apple.com/forums/thread/839069?answerId=898801022]**

**[assumption]** Permission changes atomically increment the capture policy generation and invalidate queued results. Never automatically reopen repeated permission dialogs. Provide Settings instructions and retry after an explicit action. Persist onboarding progress across a relaunch; do not persist “permission granted” as authority. Capture only in the logged-in GUI user's session, without root or Full Disk Access. Do not claim notarization grants any TCC access.

## Distribution, helper lifecycle, and language

Mac App Store guideline 2.4.5 requires sandboxing. Apple lists assistive use of Accessibility APIs and sending Apple Events to arbitrary apps among sandbox-incompatible activities. **Conclusion: this broad observer is unsuitable for the Mac App Store; screen recording alone is not categorically forbidden there.** **[verified: https://developer.apple.com/app-store/review/guidelines/]** **[verified: https://developer.apple.com/documentation/security/protecting-user-data-with-app-sandbox]**

Developer ID notarization requires valid signatures on distributed executables, hardened runtime, secure timestamps, and no enabled `get-task-allow` entitlement. Notarization is an automated security process, not App Review. Apple's tools include `notarytool` and `stapler`. **[verified: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution]**

**[assumption]** Package `Macroscope.app` with a signed Swift observer login-item app in `Contents/Library/LoginItems`, signed native-messaging adapter, and signed IPC components. Sign nested components before the outer bundle; notarize and staple the distribution. Verify the installed/quarantined build, not only Xcode launches. Preserve bundle identifiers and signing identity across updates and test TCC attribution for this exact topology: do not assume helpers inherit every main-app grant.

The hardened-runtime Apple Events entitlement is `com.apple.security.automation.apple-events`; it permits prompting, not blanket approval. ScreenCaptureKit's documentation calls for `NSScreenCaptureUsageDescription`. **[verified: https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.automation.apple-events]** **[verified: https://developer.apple.com/documentation/screencapturekit]**

**[assumption]** Put Screen Recording purpose text on the actual capture identity, and `NSAppleEventsUsageDescription` plus the Automation entitlement only on the executable implementing the optional URL adapter. Keep the observer free of JIT, unsigned executable memory, library-validation exceptions, microphone, camera, and persistent-capture entitlements. Screen capture and AX still require user consent; do not invent a `com.apple.security.screen-capture` entitlement. Audit UI-framework-specific hardened-runtime exceptions separately so a UI runtime does not widen the helper's permissions.

`SMAppService.register()` can register a bundled login item, start it immediately, and launch it at subsequent logins subject to approval. `mainApp` supports starting the containing app without a separate helper. **[verified: https://developer.apple.com/documentation/servicemanagement/smappservice/register()]** **[verified: https://developer.apple.com/forums/thread/777520]**

**[assumption]** Use `SMAppService.loginItem(identifier:)` for the observer, enabled by a clear “Observe at login” toggle after a successful trial. Inspect service status, including approval requirements, and honor disabling it in System Settings. No root daemon or privileged helper. Define close-window as hide UI, Pause as stop capture, and Quit Macroscope as stop observer and UI; do not respawn after an intentional quit during the same session. Unregister login service on disabling auto-start and stop capture before updates. Recovery/backoff must not create permission-prompt or crash loops.

Rust has real options: `objc2-screen-capture-kit` supplies framework bindings, and `screencapturekit` offers a higher-level wrapper with macOS version gates. **[verified: https://docs.rs/objc2-screen-capture-kit/latest/objc2_screen_capture_kit/]** **[verified: https://docs.rs/screencapturekit/latest/screencapturekit/]**

**[assumption]** Choose Swift for the helper despite the owner's Rust familiarity. ScreenCaptureKit delegates, AX run loops, Vision, AppKit, ServiceManagement, TCC, and XPC are the core complexity, so direct Apple APIs reduce bridge/lifetime/availability debugging. Rust does not eliminate Objective-C ownership or OS permission problems. Keep shared hashing, schemas, storage, and workflow logic in Rust if the overall architecture favors it. A versioned IPC boundary makes a future helper rewrite possible. This is a small native platform component, not a commitment to a Swift-only application.

## Prototype gates and product-killing risks

**[assumption]** Before building the pattern engine, run a signed observer spike on macOS 15 and 26, Apple Silicon first. Add an Intel tester if Intel support is promised. Validate:

- Fresh onboarding, denial, Settings revocation, recurring confirmation, OS stop-sharing, relaunch, updates, and helper attribution; a 35-day dogfood run is needed to observe recurring behavior.
- Safari/Chromium normal versus private windows, multiple profiles/windows, password managers, revealed-password controls, custom Electron widgets, and rapid switch-to-secret races. Search every persisted artifact for seeded canary secrets.
- Two displays with different scales, negative coordinates, Spaces/fullscreen/Stage Manager, dock disconnect, lock/wake, remote sessions, and other simultaneous screen-sharing applications.
- Eight-hour CPU/RSS/WindowServer measurements and a one-week storage run; compare against the app stopped, not just helper-reported CPU. Test animation-heavy apps and hung AX clients.
- Three known workflows captured with extension/AX events and occasional screenshots; a human must reconstruct the repeated steps without access to typed content.

**[assumption]** Release blockers: secrets reach disk or a model before sanitization; private-browser content is collected by fallback paths; OS prompts or indicators make sustained opt-in unacceptable; capture destabilizes WindowServer; required consent is unavailable on managed work Macs; or useful workflow reconstruction requires retaining typed text. Resolve by narrowing supported apps and retaining structured signals, not hiding warnings or expanding collection.

**[assumption]** Treat screen/DOM/OCR strings as hostile evidence: they must never become instructions to the generator, shell, or permission controller. Cloud analysis receives selected sanitized records only. No on-device Meta model availability or unverified endpoint is assumed. The capture helper never receives the owner's key; the downstream API process must retrieve it from Keychain and use authenticated Meta API calls or the brief's verified `META_API_KEY` path for `muse exec`. `muse serve` authentication remains separately unverified as stated in the brief. **[local: API-key evidence supplied in plan/BRIEF.md, not retested here]**

## Open questions for the owner

- Which browser must be supported first: Safari or Chromium? This determines extension packaging and the first compatibility fixtures.
- Is native-window opt-in plus browser structured events acceptable for v1, with automatic browser screenshots off?
- Is seven-day evidence retention sufficient, or is remembering a weekly workflow worth a separately enabled longer retention period?
- Will v1 promise Intel Mac support, requiring another hardware tester, or Apple Silicon only?

## Decisions I made

- Swift observer with versioned IPC; shared cross-platform logic may remain Rust.
- Developer ID plus notarization outside the Mac App Store.
- Foreground native-window capture at two candidate fps; no default desktop-wide recording.
- Browser extension is the web-workflow authority; private browsing is excluded and automatic browser pixels stay off.
- No typed text, ordinary keycodes, clipboard payloads, audio, or raw persistent frame buffer.
- AX/DOM semantics outrank OCR; local Vision OCR is selective and budgeted.
- Seven-day evidence retention, 3 GB vault/workweek targets, and under 3% one-core CPU are prototype acceptance gates.
- Optional Input Monitoring and Apple Events adapters; denial leaves useful structured-only modes.
- OS indicators and recurring consent are product constraints to explain and measure.
