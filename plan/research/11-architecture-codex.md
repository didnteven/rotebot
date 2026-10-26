# Macroscope architecture: independent take A

Date: 2026-10-22. Scope: desktop architecture and delivery, before app implementation.

Evidence convention: **[verified: URL]** identifies external documentation; **[local: path]** identifies repository evidence I read. **[assumption]** identifies a proposal, engineering judgment, target, or unresolved dependency, rather than measured behavior. Unless explicitly attributed otherwise, implementation choices below are **[assumption]**. No resource figures are benchmarks.

## 1. Decision: Tauri 2, React, TypeScript, Rust, a small Swift capture helper

Choose **(a) Tauri 2**, with **React + TypeScript + Vite**, a Rust application core, and a signed Swift observer helper on macOS. Implement the Windows observer using Rust and Windows APIs. Do not build two native application interfaces, embed Flutter, or ship a permanent Python/Node service. This recommendation prioritizes low background overhead and one maintainable interface while accepting WebKit/WebView2 differences and some native integration work. **[assumption]**

Tauri's core manages windows and tray interfaces; its web content uses operating-system webviews, specifically WKWebView on macOS and WebView2 on Windows. It does not bundle those webview libraries. That establishes the structural opportunity to keep a small native resident core and destroy unused interface windows; it does **not** prove a particular RAM saving. [verified: https://v2.tauri.app/concept/process-model/] [verified: https://v2.tauri.app/reference/webview-versions/]

React wins over Svelte/Solid here because I expect the broadest useful agent-generated examples, UI components, terminal integrations, and test fixtures. That is a productivity judgment, not a measured model benchmark. Keep React out of the capture and execution layers. Prefer ordinary hooks and a small state store to a large application framework; no SSR server. Pin dependencies and build reproducibly from lockfiles. **[assumption]**

### Serious comparison

The following assessments are architectural judgments **[assumption]**, grounded where noted in documented platform capabilities.

| Criterion | Tauri 2 + React: selected | Electron + native helpers | Flutter + Rust bridge | SwiftUI + WinUI 3 + Rust |
|---|---|---|---|---|
| Menu bar/tray | Native tray/menu; custom popover needs focus and positioning work | Strong tray support; custom popover still needs platform work | Requires platform/plugin integration around Flutter surfaces | Best platform fit and control |
| Multiple windows | One shared web UI, explicit window lifecycle | Particularly mature web-window model | Feasible, but more plugin/platform coordination | Excellent per platform; duplicate UI implementations |
| Embedded terminal | xterm.js directly in existing UI | Best fit for xterm.js and familiar editor patterns | Dart terminal or another webview bridge | Native terminal library or webview on each platform |
| Micro-app hosting | Existing web stack, separate unprivileged host windows | Consistent Chromium; strong embedding primitives | Desktop webview coverage becomes another dependency | Separate WKWebView/WebView2 integration |
| Capture | Swift helper still required by this design | Native helper still preferable for this workload | Native plugin/helper still required | Native capture easy on Mac; Windows remains separate |
| Idle footprint | Favorable architecture if windows/workers truly exit | Chromium/Node baseline is an accepted cost | Resident rendering engine plus webviews for web tools | Best potential minimum; more UI code |
| Updating/signing | Official updater and platform signing guides | Mature ecosystem; browser security updates add cadence | Assemble updater strategy and Rust/native packaging | Two platform release stacks and an updater decision |
| Windows parity | Same UI, different browser/native capture implementation | Strongest rendering consistency | Same Flutter UI; web embedding/native gaps remain | Lowest probability of maintained parity for one Mac owner |
| AI-agent productivity | Common TS/Rust; constrain native surfaces | Strongest likely TS/JS familiarity | Owner familiarity helps; Dart/Rust/native seams cost | Agents can write both, owner must validate both |
| Testing | Strong core tests; real webview and native tests necessary | Consistent browser automation; native helpers still separate | Strong widget tests; bridge/platform integration tests | Strong native tests, doubled interface coverage |

Electron is the close second, and its advantage is substantial: its documented main/renderer model supplies a known Chromium-based environment, with a renderer for each BrowserWindow. For a terminal-heavy IDE, I would accept its baseline cost. Macroscope spends most hours observing with no visible editor. Choosing Electron to improve the minority editing workload would make its permanent runtime a product trade-off; I choose Tauri instead. No claim that every Tauri app uses less memory is warranted. [verified: https://www.electronjs.org/docs/latest/tutorial/process-model] **[assumption: product weighting]**

For generated content, Electron recommends alternatives such as WebContentsView rather than its webview tag, and its security guide cautions about untrusted content and privileged APIs. Electron would still need a deliberately constrained bridge and execution policy; switching frameworks does not solve generated-code security. [verified: https://www.electronjs.org/docs/latest/api/webview-tag] [verified: https://www.electronjs.org/docs/latest/tutorial/security]

Flutter is credible because the owner knows it and it supports Windows/macOS desktop. flutter_rust_bridge provides the Dart/Rust binding route. However, the official webview_flutter package currently lists Android, iOS, and macOS, leaving Windows web hosting to a different integration. A Flutter shell plus Rust plus native capture plus web-hosted tools creates more boundaries than this product needs. Reject it for this app, not for desktop generally. [verified: https://docs.flutter.dev/platform-integration/desktop] [verified: https://github.com/fzyzcjy/flutter_rust_bridge] [verified: https://pub.dev/packages/webview_flutter] **[assumption: complexity assessment]**

SwiftUI/WinUI gives the best native polish and smallest plausible idle UI, but makes the owner responsible for two interface implementations, two accessibility behaviors, two terminal hosts, and two sets of UI tests. Shared Rust cannot share those obligations. Reject that maintenance load. Also reject a Python/Qt alternative: Python remains useful for experiments, but adds packaging/runtime work without an advantage over the selected stack for web tools. **[assumption]**

## 2. User-visible architecture and performance contract

The native tray menu contains: Observing/Paused, Pause for 15 minutes, Open Macroscope, Pending approvals, and Quit Macroscope. A recording state must come from the observer's acknowledged state, never an optimistic toggle. Closing the dashboard leaves observation running; Quit stops capture, workers, terminals, and schedules. Login launch is explicit opt-in and starts only the native shell plus observer. No hidden machine-wide service, root daemon, or Windows administrator requirement. **[assumption]**

Tauri documents native tray construction and menu callbacks in Rust; implement those there so no webview is necessary for Pause or Quit. Create Dashboard, Builder, and each pinned tool as distinct logical windows. Keep ordinary title bars and standard keyboard shortcuts initially; a borderless imitation of native chrome buys little. Destroy the Dashboard/Builder webviews after closure, retaining compact navigation state in Rust. Test this lifecycle because operating-system webview resource release is not a contractual memory guarantee. [verified: https://v2.tauri.app/learn/system-tray/] **[assumption: lifecycle]**

Initial acceptance targets: after ten minutes with all windows closed, no Muse host/model/runtime worker remains; aggregate application-private footprint below 180 MiB on the reference Mac and 250 MiB on the reference Windows VM, with stable memory over eight hours. Report macOS physical footprint and Windows private working set separately, including observer and attributed webview processes; these metrics are not directly interchangeable. Target below 1% of one CPU core averaged over a static-screen interval; benchmark active work separately. Capture and OCR will likely dominate energy, so coalesce events, throttle frame processing, and suspend on lock/sleep. These are provisional budgets, not predictions. **[assumption]**

Do not promise capture occurs only when the screen changes. A stream may still deliver frames; cheap hashes/dirty metadata and interaction hints decide which frames deserve redaction/OCR/storage. Set initial processing to at most one selected frame per two seconds, lower under thermal/battery pressure, then measure missed workflow evidence. **[assumption]**

## 3. Component and process model

```text
                         TRUSTED PRODUCT CODE
 +--------------------------------------------------------------+
 | Resident Tauri Rust core: tray, policy, scheduler, DB writer   |
 | - UI command authorization       - credential broker          |
 | - worker supervision             - artifact registry          |
 +----+----------------+-------------------+----------------+----+
      | typed Tauri    | private stdio     | private stdio  |
      | IPC           |                   |                |
 +----v----------+ +---v---------------+ +-v-------------+  |
 | React windows| | Observer helper   | | Model worker  |  |
 | Dashboard    | | Swift: SCK + AX   | | Rust, on demand|  |
 | Builder      | | Win: Rust WGC/UIA | | sanitized input|  |
 | approvals    | | redact BEFORE IPC| +--+---------+----+  |
 +--------------+ +-------------------+    |         |       |
                                        HTTPS    MSP stdio  |
                                           |         |       |
                                  +--------v--+ +----v-----+ |
                                  | Meta API  | |muse serve| |
                                  |owner's key| |per build | |
                                  +-----------+ +----------+ |
                                                            |
                   GENERATED / UNTRUSTED CONTENT             |
 +----------------------------+        +---------------------v-+
 | Separate micro-app window  |<------->| Executor, per run      |
 | no Tauri product authority | scoped | WASI + brokered actions|
 | separate origin/data store | bridge | limits, manifest grants|
 +----------------------------+        +-----------------------+
```

Arrows represent permitted routes, not shared authority. Model-generated output and captured screen text are untrusted inputs even when displayed inside trusted product UI. **[assumption]**

1. **Resident core / UI process boundary.** The Tauri Rust core is the per-user coordinator and sole database writer. OS webview subprocesses render UI. The core contains no OCR loop, model loop, or generated code. It owns the real tray menu, user intent, capability grants, and worker startup. Keeping this coordinator resident avoids inventing a second always-on supervisor. **[assumption]**
2. **Observer helper.** A separate, signed native child owns capture/AX on Mac and capture/UI Automation on Windows. Raw frames never enter React, the model worker, or logs. Masking, excluded-app checks, and coarse keystroke-category aggregation happen before its output boundary. Initially capture only a user-selected scope and store no raw key values, text, or recoverable key sequences. Failure or unknown sensitivity drops the frame. Process separation limits accidental exposure; by itself it does not stop arbitrary same-user native code reading files. **[assumption]**
3. **Model worker.** A short-lived Rust executable consumes sanitized workflow bundles, batches pattern classification, handles API requests/cost accounting, and owns Muse supervision during builds. It cannot query the entire capture archive: the core supplies a job-specific bundle. Worker restart must not restart capture. No local inference server is required: the brief mandates billed API-key model use. **[assumption]** [local: plan/BRIEF.md]
4. **Muse host.** One `muse serve` child per active build workspace and immutable sandbox posture, not a global unrestricted host. Reuse within that build only; stop after an idle timeout. MSP offers session operations and approvals, while serve's sandbox posture is fixed at host construction. Keep analysis-only jobs separate from shell-enabled build jobs. [local: plan/reference/muse-cli-help.txt; plan/reference/msp/msp.d.ts] **[assumption: host policy]**
5. **Executor.** A Rust process per active generated-tool run owns resource limits and brokered file/network operations. It receives neither capture permissions nor the Meta key. It survives a closed tool window only for an explicitly pinned/background job. Separate it from the model worker so a build agent cannot silently acquire a deployed tool's grants. **[assumption]**

On macOS use a small Swift package with ScreenCaptureKit, AppKit/workspace notifications, Accessibility calls, and on-device redaction. ScreenCaptureKit is Apple's capture framework; AXIsProcessTrustedWithOptions checks whether the calling process is a trusted accessibility client. Prefer Swift over objc2 here because native samples and debugging are more direct; keep the transport boundary language-neutral. [verified: https://developer.apple.com/documentation/screencapturekit] [verified: https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions] **[assumption: implementation choice]**

A major early spike must establish which signed app/helper identity macOS attributes permissions to, and verify grant/revoke, update, relaunch, and login behavior with the packaged app. Do not assume the dashboard's grant transfers to its helper. On Windows use Windows.Graphics.Capture with user-selected capture plus a UI Automation adapter. Microsoft documents system selection UI and visible capture indication; preserve them rather than promising invisible parity with Mac. [verified: https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture] **[assumption: acceptance gate]**

## 4. IPC: private stdio JSON-RPC first

Use Tauri typed commands/channels for trusted frontend-to-core communication. Use **JSON-RPC 2.0 over inherited stdin/stdout pipes** between core and child workers; use MSP's actual framing for Muse, verified against recorded binary transcripts rather than guessed from TypeScript types. Own-protocol messages use newline-delimited JSON, a versioned handshake, request IDs, cancellation, structured errors, maximum sizes, and bounded queues. stderr is reserved for scrubbed diagnostics. **[assumption]**

Do not introduce gRPC: no network service needs HTTP/2, protobuf generation, endpoint discovery, or another code generator. Do not introduce Unix sockets for the initial parent/child topology: pipes eliminate discoverable listening endpoints and have an exact Windows equivalent. If independent observer reconnection later requires persistent transport, add Unix-domain sockets with owner-only directory permissions on Mac and ACL-restricted named pipes on Windows, protocol negotiation, peer validation, and a per-launch secret. A Unix socket alone is not a Windows design. **[assumption]**

Frames remain in the observer until sanitized. Small sanitized JPEGs can initially cross as bounded base64 payloads; large media should use authenticated, length-delimited binary pipes with metadata references once measurements justify it. Do not stream raw pixels through JSON or send machine-wide paths to frontend callers. Drop/coalesce capture backlog; never block capture on a webview. Critical Pause/Quit control gets a separate priority queue. **[assumption]**

Own RPC contracts include `observer.setPolicy`, `observer.pause`, `analysis.submit`, `build.start`, `build.cancel`, `tool.invoke`, and `job.status`. Persist durable intent before acknowledging externally consequential actions. UI events carry sequence numbers and can be replaced by a snapshot after reconnect. Validate arguments on the Rust side even when TypeScript types exist. **[assumption]**

## 5. Muse, API keys, and the terminal

The repository records Muse Code 1.4.11, headless exec, stdio MSP, API-key precedence via META_API_KEY, and an observed structured rejection with a bogus key. It does **not** establish successful real-key use or serve environment authentication. Preserve that distinction. Do not invent an endpoint, npm SDK, model capability, or context limit. Discover model availability through the pinned host's catalog and expose a model picker; treat the locally observed model IDs as observations, not contractual availability. [local: plan/BRIEF.md; plan/reference/muse-cli-help.txt]

Concrete key flow: the native settings command accepts the owner's key, stores it in macOS Keychain or Windows Credential Manager, and returns only a credential identifier. Apple documents Keychain as secure credential storage; Windows exposes CredWrite for the current user credential set. The key field is transient UI state, cleared immediately and excluded from analytics and state persistence. [verified: https://developer.apple.com/documentation/security/keychain-services] [verified: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew] **[assumption: credential flow]**

For direct Meta requests, the core retrieves the key and sends it through a dedicated inherited credential pipe to the model worker; the adapter supplies the provider's documented authorization scheme after verification. For `muse exec --api-key-stdin`, a separate one-shot child receives it through stdin. For `muse serve --provider meta`, launch with an explicitly constructed environment containing META_API_KEY; its stdin must remain exclusively MSP. Never use argv, a checked-in dotenv file, terminal export commands, or consumer login. On key rotation kill the old host and start a new one. [local: plan/reference/muse-cli-help.txt; plan/BRIEF.md] **[assumption: serve path pending verification]**

Release gate: prove a real-key serve turn is billed to the supplied key, bogus-key failure remains structured, and logged-in consumer credentials cannot become a silent fallback. Also test whether Muse passes META_API_KEY into tool subprocesses; an environment key is exposed to its host and may propagate. Require a scrubbed tool environment or supported secret-isolating adapter before enabling shell tools with real credentials. A protocol wrapper alone cannot enforce that inside a closed child binary. If serve fails auth verification, ship the documented exec key-stdin path while keeping the MSP adapter behind a feature flag. **[assumption]**

Use MSP for the main builder interface: progress, files changed, preview, test output, and approvals. The local schema distinguishes acknowledging presentation of an approval from actually deciding it, and exposes typed auth/quota errors. Follow those semantics; never turn a displayed approval into consent. [local: plan/reference/msp/msp.d.ts: RequestReceipt, SessionStartParams, QuotaDetail, TurnErrorKind]

Use **xterm.js** only for an advanced terminal drawer backed by a real PTY/ConPTY adapter in Rust. Native terminal widgets would require two integration paths; an MSP userShell request is a command operation, not proof of an interactive PTY. Initially the terminal is a user-started workspace shell; Muse runs headlessly through MSP. Disable automatic clipboard/escape integrations and URL opening, bound scrollback, and keep terminal bytes off analytics. xterm's security guidance explicitly treats terminal access and privileges as application responsibilities. [verified: https://xtermjs.org/docs/guides/security/] [local: plan/reference/msp/msp.d.ts: SessionUserShellParams] **[assumption: terminal design]**

## 6. Generated micro-apps and execution boundary

V1 supports static HTML/CSS/JS tools and restricted WASI command tools, not arbitrary always-on npm/Python servers. Serve each tool from its own loopback origin on an ephemeral port, with separate webview storage and no Tauri capability grant. Bind only loopback, reject invalid Host/Origin, disable remote navigation/popups/downloads, and deny external network by default through CSP plus broker policy. Do not grant generated windows wildcard product commands or inject product credentials. Tauri capabilities scope access by window/webview and have explicit limits; they do not sandbox arbitrary Rust or native children. [verified: https://v2.tauri.app/security/capabilities/] **[assumption: hosting design]**

A custom bridge issues short-lived, tool-specific invocation tokens for named actions, never general shell commands. Broker HTTP requests to approved domains and expose user-selected file handles/working copies. A web app's fetch restrictions alone do not constrain an associated native executable. Distinct webviews/origins are logical security boundaries; do not assume a separate OS process for every webview. **[assumption]**

Use Wasmtime inside the executor for bounded command tools: explicit directory preopens, no ambient credentials/network, memory and execution limits, and output staging before user-approved export. Wasmtime documents WASI filesystem capability isolation. This does not make every host callback safe; test path traversal, symlinks, resource exhaustion, and broker authorization. [verified: https://docs.wasmtime.dev/security.html] **[assumption: runtime policy]**

Full native scripts are a later runtime tier requiring reviewed platform enforcement. Windows Job Objects manage process groups and limits; they are not a filesystem sandbox. AppContainer provides a separate isolation mechanism, whose compatibility must be proven. Do not label a child process plus a workspace path as a sandbox. [verified: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects] [verified: https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation] **[assumption: scope boundary]**

## 7. Disk layout, configuration, and retention

Use per-user local application data, never a cloud-synced default directory. Proposed layout **[assumption]**:

```text
macOS: ~/Library/Application Support/Macroscope/
Windows: %LOCALAPPDATA%\Macroscope\
  config.toml                    # versioned non-secret preferences
  state.sqlite                   # metadata, candidates, grants, jobs, costs
  media/YYYY-MM-DD/<id>.enc       # sanitized, encrypted, short retention
  workspaces/<build-id>/          # isolated generation workspace
  tools/<tool-id>/versions/<hash>/# immutable published artifact + manifest
  tool-data/<tool-id>/            # mutable data; separate from code
  runs/<run-id>/                  # staged inputs/outputs, execution journal
  sessions/<build-id>/            # Muse profile/session artifacts, if supported
  updates/                       # bounded staging; immutable signed payloads
  backups/                       # pre-migration snapshot, bounded retention
macOS: ~/Library/Logs/Macroscope/ # operational JSONL
Windows: ...\Macroscope\logs\   # operational JSONL
```

SQLite stores small events and relationships; media and build trees stay outside it. One writer in the core avoids multi-process migration and transaction ownership. Add indexes for time/app/workflow and schema migrations; defer a vector database until retrieval evidence requires one. Encrypt sensitive records/media using a maintained authenticated-encryption implementation and a random local key in the OS credential store. Keep sanitized content sensitive: redaction is imperfect. Loss of the local key must produce an explicit recovery/reset flow, never silent plaintext fallback. **[assumption]**

Default retention: raw frames never persisted; sanitized frames seven days, aggregated activity thirty days, generated tools until deleted. Cap frame storage at 1 GiB initially. Disk pressure deletes expired media first and pauses capture before database failure. Deletion removes linked transcripts and thumbnails too; encryption-key rotation and retention handle residual backups better than promising physical SSD erasure. Test actual Muse session locations/profile isolation: the exported help does not establish an app-owned profile-directory flag. If unsupported, disclose external session storage and implement supported deletion or disable persistence. **[assumption]** [local: plan/reference/muse-cli-help.txt]

Config includes schema version, capture scope/exclusions, retention, battery policy, selected model, concurrency, spending limits, update channel, and telemetry consent. Validate and atomically replace it through the core. Config is the source for preferences; SQLite is the source for workflow state/grants. Keep a policy revision with every capture bundle/run. Snapshot effective policy at job admission. Model output cannot edit settings, relax runtime grants, or approve its own artifacts. **[assumption]**

## 8. Logs, recovery, and updates

Local structured logs contain timestamp, component/build version, job ID, duration, error code, and resource counters. Exclude screenshots, window titles, typed content, prompts, transcripts, terminal output, paths containing personal names, and secrets by default. Rotate to a 50 MiB total budget. Telemetry and crash upload are separate opt-ins, disabled initially. Diagnostic export shows an inspectable manifest; omit memory dumps because they can contain keys or frames. Display model usage/cost locally even when telemetry is off. **[assumption]**

Job lifecycle: queued, admitted, running, awaitingApproval, completed, failed, interrupted, outcomeUnknown. Persist run ID, input/artifact hashes, capability revision, and external-action receipts. On restart, mark unfinished externally consequential runs outcomeUnknown; never automatically replay a send/payment/file overwrite. Resume Muse sessions only after reconciling the durable session snapshot and pending approvals; do not infer success from the last streamed delta. **[assumption]**

Observer death changes the tray state to Observation stopped immediately. Restart with bounded exponential backoff, then remain paused after repeated failure. A coordinator crash stops child observation via pipe closure/watchdog; login startup may relaunch the shell but must not resurrect an explicit Quit. On lock/sleep stop capture, expire action tokens, and recheck permissions on wake. Kill whole worker process trees during cancellation, using Unix groups and Windows Job Objects. **[assumption]**

Use official Tauri updater artifacts, separate **stable** and **beta** HTTPS manifests, and manual developer builds. Tauri documents signed update payloads and platform/architecture manifests. The updater signature, Apple signing/notarization, and Windows Authenticode are distinct release responsibilities. Daily jittered checks download in the background; installation waits for builds/runs to finish and presents Restart to update. No unattended mid-run replacement. [verified: https://v2.tauri.app/plugin/updater/] [verified: https://v2.tauri.app/distribute/sign/macos/] [verified: https://v2.tauri.app/distribute/sign/windows/] **[assumption: update policy]**

Update all app helpers as one compatible bundle, with startup protocol checks rejecting mixed versions. Back up the database through a consistent snapshot before migration. Prefer additive migrations and a forward hotfix; binary rollback is safe only with compatible schema and helpers. Stable promotion follows beta soak, permission checks, and N-1 update tests. Keep emergency download links if in-app updating breaks. **[assumption]**

## 9. Monorepo and AI-agent workflow

Proposed repository **[assumption]**:

```text
macroscope/
  apps/desktop/
    src/                         # React dashboard, builder, terminal
    src-tauri/                   # thin shell, commands, capabilities
  crates/
    domain/                      # workflow/job/tool types, pure rules
    protocol/                    # own IPC schemas and generated TS types
    coordinator/                 # supervision, policy, scheduler
    storage/                     # SQLite, encryption, retention, migrations
    model-worker/                # Meta HTTP adapter + usage accounting
    muse-client/                 # MSP codec, conformance fixtures
    executor/                    # WASI runtime, action broker, limits
    observer-windows/            # capture/UIA + local sanitation
  native/macos-observer/         # Swift package, plists, entitlements
  packages/
    ui/                          # shared design primitives
    tool-sdk/                    # narrow generated-tool bridge
    tool-templates/              # HTML + WASI starter projects
  contracts/msp/                 # pinned vendor schemas + provenance/hash
  tests/
    fixtures/                    # synthetic/redacted activity + MSP traces
    integration/                 # process, migration, cancellation tests
    desktop/                     # real-window automation, screenshots
    adversarial/                 # prompt injection, paths, bridge abuse
  scripts/                       # doctor, build helpers, package, verify
  .github/workflows/             # PR checks, platform builds, release
  docs/adr/                      # decision records and threat boundaries
  plan/                          # research only
```

Cargo workspace plus pnpm workspace; Swift Package Manager only for the Mac helper. Pin Rust, Node, package manager, Xcode expectations, MSP schema, and tested Muse versions. Provide one `doctor` command reporting missing SDKs, CLI compatibility, permission state, and credential presence without printing credentials. Give agents small contract-bound tasks, deterministic fixture inputs, and explicit ownership of native/security boundaries. Do not ask agents to rewrite transport, UI, and signing simultaneously. **[assumption]**

## 10. CI and build/release pipeline

Every PR: formatting/lint/type checks, Rust unit/integration tests, React component/accessibility tests, Swift tests, protocol fixtures, lockfile/license checks, and dependency review. Build on macOS ARM64 and Windows x64 runners from the first architecture spike; fail Windows compilation immediately rather than accumulating platform debt. Initial supported targets are Apple Silicon on macOS 14+ and Windows 11 x64; Intel Mac/Windows ARM64 are deferred product targets, not assumed impossible. **[assumption]**

Use Playwright browser tests for rapid UI behavior, plus packaged-app automation for IPC, real menus, focus, window creation, tool origin isolation, and PTY resizing. Browser WebKit tests do not establish WKWebView parity. Current Tauri documentation describes WebdriverIO's embedded server path on macOS/Windows; direct tauri-driver is limited to Windows/Linux. Keep embedded automation plugins debug/test-only and verify their absence from releases. Native capture permission tests still require an interactive signed session. [verified: https://v2.tauri.app/develop/tests/webdriver/] **[assumption: test strategy]**

Hosted CI cannot establish Windows capture, protected-app behavior, multi-monitor scaling, suspend, and permission UX. Add an interactive Windows VM/desktop test environment, screenshot/video artifacts, and recruited Windows beta testers. The owner can inspect recordings from the Mac. Windows release remains preview until that evidence exists; a green cross-platform build is insufficient. Test synthetic observation fixtures on every PR and signed real capture in scheduled platform sessions. **[assumption]**

Release sequence **[assumption]**:

1. Create a versioned release candidate; run full tests, dependency/license audit, and packaging matrix. Release jobs alone receive signing credentials through protected CI environments; untrusted PR jobs receive none.
2. Compile Rust executables, React assets, and Swift helper. Verify bundled sidecar versions/hashes. Initially detect a compatible user-installed Muse binary rather than redistribute it; installer and onboarding explain the dependency. Bundling requires redistribution rights and Windows availability verification.
3. On Mac, sign nested helpers and app with stable bundle identifiers and correct entitlements, enable hardened runtime, notarize, staple, and validate the downloaded DMG/app on a clean account. Test permission persistence across the N-1 update. Tauri documents Developer ID notarization and available credential methods. [verified: https://v2.tauri.app/distribute/sign/macos/]
4. On Windows, build on Windows, sign executables and installer through a managed signing identity, timestamp, verify signatures, and test clean install/uninstall/update including WebView2 provisioning. A valid signature is not a promise that reputation warnings never occur. [verified: https://v2.tauri.app/distribute/sign/windows/] **[assumption: validation policy]**
5. Generate updater signatures for final immutable payloads, checksums, an SBOM, release notes, and platform manifests. Upload artifacts first, verify download/signatures, then publish the beta manifest atomically.
6. Run signed installer and N-1 update smoke tests, memory/energy soak, pause/quit verification, and real-key Muse acceptance. Promote those exact bytes to stable after soak; do not rebuild between channels.

## 11. Product-killing risks and first proof

**Permissions/privacy:** if native permission attribution is unstable across updates or privacy masking misses ordinary sensitive content, continuous observation may be unacceptable. Start with deliberate capture scope, visible state, exclusions, and user-approved outbound evidence. Raw screen content is hostile input, never instructions for a tool. **[assumption]**

**Distribution:** ship direct-download notarized macOS builds, not a Mac App Store dependency. Apple's review guidelines restrict downloaded/executed code that changes functionality; the exact treatment of this product requires review, but a generated-tool architecture should not depend on a favorable store interpretation. Notarization is neither sandbox enforcement nor privacy certification. [verified: https://developer.apple.com/app-store/review/guidelines/] **[assumption: distribution choice]**

**Meta dependency/cost:** successful API-key auth for serve, provider terms for captured third-party data, Muse redistribution, and Windows binary/sandbox support remain explicit gates. Do not route screen data to an unverified endpoint or build a business model on unverified prices/context sizes. Enforce queue concurrency and admission budgets locally, while admitting that a running provider turn may overshoot a locally estimated budget. **[assumption]** [local: plan/BRIEF.md]

First proof: a signed Tauri app with real tray Pause/Quit, a separate Swift helper emitting sanitized fixtures and one consented capture, an eight-hour idle measurement, two real windows, one isolated HTML tool, and a real-key Muse run. In parallel CI terms, build the same shell and synthetic observer contract on Windows; then obtain an interactive Windows test session. If these fail, resolve the specific boundary before building the pattern engine. **[assumption]**

## Open questions for the owner

- Can v1 require Apple Silicon/macOS 14+ and Windows 11 x64, with Windows explicitly preview until interactive beta coverage exists?
- Is requiring a compatible user-installed Muse acceptable initially, pending redistribution rights and a verified Windows package?
- Is HTML plus restricted WASI sufficient for first pinned tools, with unrestricted native Python/shell deployment deferred?
- What monthly API spending ceiling should onboarding default to, and can sanitized evidence leave the device only after per-workflow review initially?

## Decisions I made

- Choose Tauri 2 + React/TypeScript/Vite + Rust; keep Swift limited to native Mac observation.
- Keep the tray/core resident, destroy unused webviews, and launch model/executor/Muse processes on demand.
- Use inherited-pipe JSON-RPC for owned workers and actual MSP framing for Muse; defer gRPC and reconnecting sockets.
- Put raw capture sanitation before every storage/model/UI boundary and give generated tools no observation authority.
- Use OS credential storage and API-key-only Meta routing; require real-key serve and credential-inheritance verification.
- Use MSP for the main builder, xterm.js for an explicitly opened advanced workspace terminal.
- Start generated runtime support with isolated web tools and capability-limited WASI.
- Use one SQLite writer, encrypted sensitive payloads, bounded retention, and no default telemetry uploads.
- Ship signed direct downloads with stable/beta channels and atomically compatible helper versions.
- Require Windows CI immediately and interactive Windows evidence before claiming release parity.
