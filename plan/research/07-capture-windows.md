# Windows capture and the cross-platform observer contract

Research date: 2026-10-22. Scope: planning only; no Windows runtime was available for this research. [local: read `plan/BRIEF.md` fully; the brief says no application code exists and the owner can only test on Mac.]

Evidence convention: **[verified: link]** marks checked primary documentation; **[local: ...]** marks local evidence; **[assumption]** marks proposed designs, estimates, and inferences. No Windows behavior was runtime-tested.

## 1. Recommendation and product boundary

**[assumption]** Build a native Rust observer process with Windows-specific adapters for Windows.Graphics.Capture (WGC), WinEvent, UI Automation (UIA), and Windows.Media.Ocr. Share a versioned observer protocol and privacy pipeline with macOS. The observer runs as the signed-in user at normal integrity, has no model credentials, and cannot launch generated automations. Ship Windows after the macOS learning loop proves useful, initially as an explicitly enabled beta observing one selected work window.

**[assumption]** Support serviced Windows 11 releases first; build x64 and ARM64 artifacts. Defer Windows 10 compatibility. Initial Windows discovery will miss workflows spanning several apps. State this during onboarding; the shared engine must represent observation gaps and reduced confidence.

**[assumption]** Use an MSIX-packaged full-trust desktop app distributed directly during beta. Package identity is useful for supported OCR and borderless capture capabilities. “Full trust” here is a packaging term, not a request for administrator rights. Run the UI, native observer, and model/generation host as separate processes with separate responsibilities:

```text
Windows session, normal user
  Tray / onboarding UI -- explicit start, pause, exclusions, preview
             |
     policy epoch + control RPC
             v
  Native observer: WGC / WinEvent / UIA / optional input
             |
     local raw buffers, short-lived
             v
  Local privacy gate: exclusions -> UIA masks -> OCR -> redact/drop
             |
     sanitized observations only, bounded stream
             v
  Local pattern engine -> user reviews candidate -> model/generator host
                                                    |
                                         owner's Meta API key
```

**[assumption]** The process boundary is failure isolation, not protection from an administrator or malware running as the same user. Keep raw screenshots and UI text out of logs, crash dumps, analytics, and ordinary IPC to the UI. Default to no durable raw history; retain sanitized workflow evidence under an explicit retention setting. A dropped frame is preferable to unreviewed raw content reaching a model.

## 2. WGC: windows, monitors, borders, and frame scheduling

WGC exposes capture of a display or application window, a system selection UI, capture sessions, and Direct3D frame pools. The normal system indication is a yellow border around each captured item. Call `GraphicsCaptureSession::IsSupported()` before offering capture. [verified: [Microsoft screen capture guide](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture?source=recommendations)]

Desktop interoperability also exposes `IGraphicsCaptureItemInterop::CreateForWindow(HWND)` and `CreateForMonitor(HMONITOR)`; both are documented from Windows 10 version 1903. These are distinct from asking the user to pick an item. [verified: [CreateForWindow](https://learn.microsoft.com/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow), [CreateForMonitor](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createformonitor)]

**[assumption]** Beta starts through `GraphicsCapturePicker`, initialized with the UI's owner HWND. Keep one selected window session, stopping when it loses foreground focus, closes, minimizes, or becomes disallowed. Resume only after rechecking identity and policy. Pair HWNDs with process creation identity and a session generation to handle reuse. Dialogs and popups outside the selected item become missing context until tested support exists. Test whether per-window capture exposes content obscured by overlays; do not promise it represents only pixels visible to the user.

**[assumption]** Later foreground-following creates a capture item after each eligible transition, requiring explicit consent and race tests. Distinguish Win32 HWND interoperability from the newer `TryCreateFromWindowId`/programmatic-access path. Probe the chosen API/package; for the latter, declare `graphicsCaptureProgrammatic` and request access. Never silently substitute another backend after denial.

The manifest reference documents `graphicsCapture`, `graphicsCaptureWithoutBorder`, and `graphicsCaptureProgrammatic`, and distinguishes requirements for AppContainer and medium-integrity desktop packages. The programmatic capability specifically covers creating items from WindowId/DisplayId. [verified: [Capability reference](https://learn.microsoft.com/th-th/windows/apps/package-and-deploy/app-capability-declarations)]

### Yellow border: the supported Windows 11 route

To suppress the border, declare `<uap11:Capability Name="graphicsCaptureWithoutBorder"/>`, request `GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)`, and set the session's `IsBorderRequired = false` after consent. Denial can leave the border visible even though setting the property succeeds. Another application's session requesting a border on the same item can also keep it visible. The property is documented from build 20348, so it is not accurate to describe this as exclusively a Windows 11 invention. [verified: [IsBorderRequired](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired?view=winrt-22621), [uap11 manifest schema](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap11-capability)]

**[assumption]** Keep the border in beta. Later offer “Hide Windows capture border” in settings with an explanation and the system request, while retaining our tray indication and pause shortcut. Report `border_suppression_requested` separately from observed/effective indication; the property alone cannot prove no border is visible. Do not use registry patches, unsupported DWM flags, or ask users to weaken OS privacy policy. Borderless operation is a comfort feature, never a prerequisite for capture.

### Excluding Macroscope itself

`SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` applies to a top-level window owned by the calling process. The exclusion value is supported from Windows 10 version 2004; older versions behave like `WDA_MONITOR`. It depends on desktop composition and is not a DRM or comprehensive security guarantee. [verified: [SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)]

**[assumption]** Every Macroscope process owning top-level windows sets affinity on creation and checks the return code, including settings, preview, key-entry, terminal, and generated-tool review windows. The observer maintains a separate denylist of all our process identities and windows: affinity does not prevent collecting our UIA text, titles, or input metadata. Pause all observation while our credential or automation-review UI has focus. Our own generated tools are also excluded by default to prevent a feedback loop that discovers its own test activity.

### Capture only useful changes

WGC frames have `ContentSize`, a Direct3D surface, and QPC-based `SystemRelativeTime`; HDR capture needs care with pixel format and tone mapping. Recent session APIs include `MinUpdateInterval` and dirty-region controls, but availability varies by build. [verified: [Frame and HDR guidance](https://learn.microsoft.com/ka-ge/windows/apps/develop/media-authoring-processing/screen-capture), [Session members](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession?view=winrt-28000)]

**[assumption]** Use a small free-threaded D3D11 pool, drain promptly, copy only the newest eligible surface, and release frame ownership before asynchronous OCR. Start with a two-frame pool, a two-per-second processing ceiling, 200 ms settling debounce after changes, and one privacy-approved frame per second maximum into the engine. These are tuning hypotheses, not performance claims. Use low-resolution tile differences locally to reject duplicates, mask cursor/caret churn, and gate meaningful snapshots with foreground/UIA/input events. “Record only changes” describes emitted evidence; the OS can still deliver many frames. Feature-detect newer throttling/dirty-region APIs and retain a hash-based path.

**[assumption]** Recreate the pool on resize, device loss, or display changes; discard pending buffers across policy/focus changes. Each async result carries the policy epoch and foreground generation captured at acquisition. Check both again immediately before emission. Any mismatch discards the entire result, including OCR and tree data. Monitor capture remains later: an allowed foreground window does not make visible background windows, notifications, or another monitor safe. Full-monitor redaction requires conservative occlusion-aware exclusion handling and independent privacy tests.

## 3. Foreground context, UIA, and interaction evidence

Register `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, ..., WINEVENT_OUTOFCONTEXT)` on a dedicated message-loop thread. Out-of-context callbacks are queued into the registering process rather than injected into target applications, and the registering thread needs a message loop. Event callbacks can reenter. [verified: [SetWinEventHook](https://learn.microsoft.com/en-us/windows/desktop/api/winuser/nf-winuser-setwineventhook)]

**[assumption]** Initialize with `GetForegroundWindow`, then treat callbacks as reconciliation triggers. On transitions, invalidate old capture immediately, resolve the top-level window/process, apply exclusions before reading its title/tree, and publish a new context generation. Add narrowly filtered name/destroy/location events plus a slow foreground reconciliation timer to handle missed transitions. A `WINEVENT_SKIPOWNPROCESS` flag on the observer does not exclude windows owned by our separate UI process; use the explicit process registry. Never synchronously traverse UIA or invoke OCR in a WinEvent callback.

UIA represents the desktop as a tree with raw, control, and content views. Clients can subscribe to scoped events, but providers do not necessarily raise every event. Cached property requests reduce cross-process round trips. [verified: [UIA tree](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-treeoverview), [Event subscriptions](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-eventsforclients), [Caching](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients)]

**[assumption]** Start from the active window's control view; prioritize focused element and ancestors, then visible interactive children. Subscribe to focus changes and active-window structure, selection, invoke, and a small property set such as enabled/offscreen/bounds. Avoid value/text-change subscriptions that deliver typed content. Refresh on debounced events, with a slow fallback refresh while actively working. Bound traversal to 500 nodes, depth 12, and a target 150 ms budget. A budget limits our traversal between calls; it cannot cancel a hung third-party COM call.

UIA client calls should run away from the app UI thread. Microsoft's native guidance discusses COM apartment/thread requirements and handler registration. [verified: [UIA threading](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading)]

**[assumption]** Own UIA subscriptions and cached element references on a dedicated MTA worker. Use cached-only DTOs outside it. Since a provider can stall indefinitely, keep the UIA worker in a restartable helper process with a watchdog, not merely a cancellable Rust future. After a 500 ms deadline return `Partial/ProviderTimeout`; rate-limit helper restarts and open a per-app circuit breaker. Screen-only operation then requires its own privacy checks; missing UIA is never interpreted as “no sensitive controls.”

**[assumption]** Retain roles, relationships, bounds, focus/enabled/selected/password flags, supported pattern names, and sanitized labels. Do not dump `ValuePattern.Value`, document `TextPattern` ranges, clipboard contents, or edit control text. UIA `Name`, AutomationId, window titles, and labels can themselves contain customer data; treat all strings as content, cap lengths, sanitize locally, and remove edit-control names by default. Password/unknown edit controls cause masking or whole-frame rejection. OCR is needed for custom canvases and missing providers, but neither UIA nor OCR proves that a screen is safe.

### Low-level input hooks and endpoint protection

`WH_MOUSE_LL` and `WH_KEYBOARD_LL` callbacks run in the installing process's context and need a message loop. They can distinguish injected input and can suppress events, though Macroscope must never suppress them. Timeout can silently remove a hook; Microsoft recommends a dedicated quick callback thread or Raw Input for many monitoring cases. [verified: [LowLevelMouseProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc), [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)]

**[assumption]** Do not ship global keyboard hooks in initial Windows beta. Foreground dwell, UIA actions, screenshots, and optional mouse clicks are enough to validate utility. Later prefer Raw Input for aggregate typing activity, testing device and remote-session coverage explicitly. Raw Input is not a privacy exemption: raw key sequences can reconstruct secrets just as hooks can.

Raw Input can receive background device data when registered with `RIDEV_INPUTSINK` and a target window. [verified: [Raw Input overview](https://learn.microsoft.com/vi-vn/Windows/win32/inputdev/about-raw-input), [Microsoft implementation example](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input)]

**[assumption]** If hooks become necessary, first ship mouse-only click/wheel observation behind a separate toggle. The callback copies minimal data to a bounded queue, calls `CallNextHookEx`, and returns immediately; enrichment happens elsewhere. A future keyboard adapter reduces input inside the native callback to one-second activity buckets and a fixed approved shortcut vocabulary. Never export scan codes, printable virtual-key sequences, Unicode translation, exact inter-key timings, or clipboard values. Drop activity in excluded/unknown focus contexts, including during the focus-resolution race. Known shortcuts such as Copy/Paste indicate intent only, never what was copied.

**[assumption; risk inference]** A low-prevalence binary combining screen capture, keyboard monitoring, persistence, and cloud communication resembles spyware behavior to AV/EDR. There is no verified universal rule that installing `WH_KEYBOARD_LL` causes a detection, nor a way to guarantee signing will prevent one. Avoid injection, drivers, obfuscation, hidden persistence, and unnecessary privileges. Publish a precise behavior description; test signed release binaries under Defender and a Windows tester's actual EDR. If security software blocks the observer, show a clear stopped state rather than instructions to disable protection.

Microsoft distinguishes SmartScreen reputation from antivirus detection, offers no general developer false-positive-prevention list, and provides a submission/dispute process for incorrectly detected software. [verified: [Developer FAQ](https://learn.microsoft.com/en-us/defender-xdr/developer-faq), [Submission guide](https://learn.microsoft.com/en-us/defender-xdr/submission-guide)]

## 4. OCR, protected surfaces, and privilege boundaries

`Windows.Media.Ocr` is documented for desktop apps with package identity. `OcrEngine` supports language discovery, engine creation, asynchronous recognition of a `SoftwareBitmap`, and word-level positions; consult `MaxImageDimension` rather than hardcoding an image limit. [verified: [OCR namespace](https://learn.microsoft.com/pt-br/uwp/api/windows.media.ocr?view=winrt-28000), [OcrEngine](https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr.ocrengine?view=winrt-28000)]

**[assumption]** Use Windows OCR locally for redaction and text extraction, with no network fallback. Probe installed recognizer languages during setup; an unavailable language becomes a visible capability limitation. Split or downscale images to the reported limit while preserving coordinate transforms. Drop a frame on OCR failure when OCR is required to clear it. First apply known UIA masks, then recognize remaining pixels, classify sensitive text, expand masks, and produce a sanitized frame and sanitized text together. OCR confidence and detection coverage cannot justify a “guaranteed private” claim; use app allowlists and previews as primary controls.

Windows protected content and application capture exclusions can produce omitted or blank content. Microsoft states that Recall does not store DRM content and describes ordinary screenshot APIs and remote-desktop capture protection as relevant boundaries. [verified: [Manage Recall](https://learn.microsoft.com/windows/client-management/manage-recall)]

**[assumption]** Accept DRM/protection as an intentional gap; do not bypass it with injection, alternate APIs, OCR from a different source, or a driver. Black pixels alone are not proof of DRM: they could be a legitimate black window, a stale frame, minimization, or a GPU failure. Emit `Unavailable(Unknown)` when the reason cannot be established, and `ProtectedContent` only with reliable evidence. Exclude remote desktop/VM-client windows initially because they conceal another machine's app identities and privacy boundaries.

UAC consent/credential prompts use the secure desktop by default, separate from the user's ordinary desktop. UIAccess can bypass some UI privilege isolation restrictions, with signing and secure-installation requirements. These are not ordinary capture permissions. [verified: [How UAC works](https://learn.microsoft.com/en-gb/windows/security/application-security/application-control/user-account-control/how-it-works), [UIAccess policy](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/user-account-control-allow-uiaccess-applications-to-prompt-for-elevation-without-using-the-secure-desktop)]

**[assumption]** Distinguish elevated applications on the normal desktop from the secure desktop. Do not assert every elevated window's pixels are invisible to WGC; UIA access, input observation, and pixels have different constraints. Conservatively exclude elevated targets in v1 even if pixels remain obtainable. Pause and flush on lock, desktop/session switch, suspend, or inability to establish a safe foreground context. Resume only after validation. No administrator observer, SYSTEM service, UIAccess helper, UAC-policy changes, or credential-dialog automation is in scope. Windows limitations must not be presented as idle user time.

## 5. Recall: relevant precedent and its limits

Microsoft's June 2024 update announced changes following feedback: affirmative snapshot opt-in, Windows Hello enrollment/presence, and additional protection including just-in-time decryption. Its September architecture described TPM/Hello-bound keys, encrypted snapshots and semantic index, isolated processing in VBS enclaves, and protected services. [verified: [June update](https://blogs.windows.com/windowsexperience/2024/06/07/update-on-the-recall-preview-feature-for-copilot-pcs/), [September architecture](https://blogs.windows.com/windowsexperience/2024/09/27/update-on-recall-security-and-privacy-architecture/)]

Recall's documented controls include pause/delete, app and supported-browser website filters, private-browsing filtering, default sensitive-information filtering, and a tray status indicator. Website filtering has limits: sensitive sites can still appear as embedded/background content. Recall is described as locally processing and storing content on Copilot+ PCs. [verified: [Recall privacy and control](https://support.microsoft.com/en-gb/windows/privacy/privacy-and-control-over-your-recall-experience)]

Administrators have Recall-specific policies for feature availability, disabling snapshots, application/URI exclusions, and retention/storage. On managed devices an administrator permitting the feature does not replace individual user snapshot consent. [verified: [WindowsAI policy CSP](https://learn.microsoft.com/en-gb/windows/client-management/mdm/policy-csp-windowsai)]

**[assumption; inference from the changes]** The lesson is that a searchable screen archive is a sensitive credential-adjacent asset, including derived vectors and titles. Macroscope should adopt opt-in, a persistent status indicator, instant pause, short retention, deletion by app/time, and explicit local/cloud distinctions from the start. Its value proposition should be “review a recurring workflow and build a tool,” not “save everything forever.” Pause must cover UIA, OCR, and input as well as pixels; deletion must invalidate derived evidence and pending model queues.

**[assumption]** Do not read Recall's database or depend on Recall availability. Recall policy settings do not automatically govern an unrelated third-party recorder. Offer our own enforceable administrator disable/exclusion policy before an enterprise launch; consider honoring an explicit Recall-disable setting conservatively, with a clearly documented mapping, rather than claiming it is an OS requirement. Browser private-mode/site recognition needs a verified browser integration; until then exclude the entire browser by default or require explicit session observation with clear limitations.

**[assumption; product risk]** DPAPI-backed encryption for our local store is not equivalent to Recall's enclave architecture. A same-user process could still access decrypted data in our process. Never market parity with Recall's protections. Cloud Meta analysis also changes the trust boundary: send only locally cleared, minimized evidence after the user's informed enablement, subject to confirmed API terms. Screen text and UIA strings are untrusted evidence, never instructions to the coding agent. The observer cannot authorize execution, access secrets, or change model prompts based on onscreen instructions.

**[local: BRIEF.md]** The tested model-key route is `META_API_KEY` for `muse exec`; successful real-key use and `muse serve` inheritance remain unverified. **[assumption]** The separate model host retrieves the owner's key from the OS credential store and passes it only in the child Muse process's environment (or per-run stdin where supported), never argv, observer IPC, generated-tool environment, or logs. The Windows observer works without any Meta key and contains no speculative Meta endpoints or Muse model assumptions.

## 6. Packaging, signing, updates, and startup

Microsoft now documents Azure Trusted Signing under **Artifact Signing**. It manages signing keys/certificates and integrates with CI. Its current public-trust eligibility includes New Zealand organizations, while individual developers must be in the US or Canada. The owner's timezone is not proof of legal location or business eligibility. [verified: [Service overview](https://learn.microsoft.com/en-us/azure/artifact-signing/overview), [Eligibility](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)]

Current Microsoft guidance explicitly says **EV certificates no longer bypass SmartScreen**. Valid signing still identifies the publisher; new signed binaries may warn until reputation develops. Microsoft Store distribution avoids SmartScreen download warnings for Store-installed apps. [verified: [SmartScreen developer guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)]

**[assumption]** Prefer Artifact Signing if the owner's legal publisher qualifies; otherwise obtain ordinary publicly trusted Authenticode signing from a suitable CA. Do not buy EV solely for SmartScreen. Sign and timestamp the observer, UI executables, helpers, and final MSIX using a stable publisher identity; verify signatures after packaging. Restrict signing to approved release jobs with short-lived CI identity. A signed outer package must not become an excuse to distribute unsigned executables that EDR evaluates separately.

MSIX/App Installer supports configurable update behavior and enterprise deployment controls. [verified: [MSIX deployment and updates](https://learn.microsoft.com/en-us/windows/msix/desktop/managing-your-msix-deployment-enterprise)]

**[assumption]** Direct signed MSIX plus HTTPS `.appinstaller` is the initial distribution choice, with a tested previous-version recovery procedure. An EXE/MSI installer would offer flexible placement and bootstrap scripts, but would also add an updater/startup/uninstall system and would not, by itself, supply the documented OCR package identity. Sparse packaging is a later compatibility option, not a second v1 installation path. Prove packaged Rust helper activation, local IPC, Flutter/Tauri UI integration, and externally installed Muse execution in an early spike. Keep generated tools/workspaces outside the immutable package. Store submission is later and needs policy review of capabilities, generation, and external tool execution; MSIX does not imply Store approval.

Packaged desktop startup tasks can be declared in the manifest and toggled through `StartupTask`. A user's Task Manager/Settings disable cannot be overridden programmatically; packaged desktop apps do not necessarily get a UWP-style consent dialog. [verified: [StartupTask](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask?view=winrt-26100)]

**[assumption]** Default startup off; enable only through our own explicit onboarding switch. Launch a normal user tray process at sign-in, with no capture before completed consent and current policy checks. Show disabled-by-user and disabled-by-policy accurately. Preserve pause across reboot until the user resumes; do not recreate Run keys or scheduled tasks to defeat disablement. Clean uninstall removes startup registration and offers a separate explicit choice about retained user evidence.

## 7. Shared Rust observer protocol

**[assumption]** Share semantics, privacy invariants, sequencing, and serialization. Keep D3D textures/HWNDs/COM objects on Windows and native surfaces/accessibility handles on macOS. The public engine receives only sanitized DTOs and bounded frame buffers, not native handles. The following is an interface sketch, not compiled implementation; auxiliary identifiers/newtypes are described below.

```rust
pub trait Observer {
    fn capabilities(&self) -> Capabilities;
    fn permission_state(&self) -> PermissionState;
    fn request_access(&mut self, request: AccessRequest) -> RequestId;
    fn start(&mut self, config: ObserveConfig) -> Result<SessionId, Error>;
    fn set_policy(&mut self, policy: CapturePolicy) -> Result<Epoch, Error>;
    fn pause(&mut self, reason: PauseReason) -> Result<Barrier, Error>;
    fn resume(&mut self) -> Result<(), Error>;
    fn request_snapshot(&mut self, scope: Scope) -> RequestId;
    fn next(&mut self, deadline: MonoTime) -> Result<Option<Envelope>, Error>;
    fn stop(&mut self) -> Result<Barrier, Error>;
}

pub struct Envelope {
    pub schema_version: u16,
    pub session: SessionId,
    pub sequence: u64,
    pub observed_at: MonoTime,
    pub received_at: MonoTime,
    pub policy_epoch: Epoch,
    pub context_generation: u64,
    pub source: Source,
    pub event: ObserverEvent,
}

pub enum ObserverEvent {
    StateChanged(ObserverState),
    PermissionsChanged(PermissionState),
    ForegroundChanged(Option<WindowContext>),
    WindowChanged(WindowDelta),
    FocusChanged(Option<NodeRef>),
    ActionObserved(SemanticAction), // invoke/select/toggle, never execute
    Pointer(PointerAction),        // click/wheel, target-relative position
    TypingActivity(ActivityBucket),// counts/duration bucket, no key sequence
    Shortcut(ApprovedShortcut),
    Frame(SanitizedFrame),
    Tree(UiTreeSnapshot),
    SessionChanged(SessionState), // locked/suspended/desktop unavailable
    Gap { stream: StreamKind, reason: GapReason, dropped: u64 },
    Fault { component: Component, code: ErrorCode, recoverable: bool },
}

pub struct SanitizedFrame {
    pub id: FrameId,
    pub source: CaptureSource, // Window(window_id) or Display(display_id)
    pub captured_at: MonoTime,
    pub geometry: Geometry,   // pixel size, desktop bounds, scale, rotation
    pub format: PixelFormat,  // normalized BGRA8 SDR for v1
    pub stride: u32,
    pub pixels: FrameLease,   // bounded read-only buffer, explicit release
    pub dirty_regions: Option<Vec<PixelRect>>,
    pub redactions: Vec<Redaction>, // region + reason, no original value
    pub privacy: PrivacyReceipt,   // policy epoch, checks, Cleared status
    pub coverage: Coverage,
}

pub struct UiTreeSnapshot {
    pub id: SnapshotId,
    pub window: WindowId,
    pub sampled_from: MonoTime,
    pub sampled_until: MonoTime,
    pub root: NodeId,
    pub nodes: Vec<UiNode>,
    pub completeness: Completeness, // complete / capped / timeout / unavailable
    pub privacy: PrivacyReceipt,
}

pub struct UiNode {
    pub id: NodeId,             // valid only in this snapshot/generation
    pub parent: Option<NodeId>,
    pub role: Role,
    pub label: Option<SanitizedText>,
    pub selector_hint: Option<SanitizedText>,
    pub bounds: Option<DesktopRect>,
    pub flags: NodeFlags,      // focused/enabled/offscreen/password/etc.
    pub supported_actions: Vec<ActionKind>,
    pub origin: NodeOrigin,    // native accessibility / OCR-derived
}

pub struct PermissionState {
    pub app_consent: AccessState,
    pub screen: AccessState,
    pub ui_tree: AccessState,
    pub pointer: AccessState,
    pub typing_activity: AccessState,
    pub border_suppression: AccessState,
    pub startup: AccessState,
}

pub enum AccessState {
    Unknown, NotRequested, Requesting, Granted,
    Denied, RestrictedByPolicy, Unsupported,
    NotRequiredByOs, // does not substitute for app-level user consent
    Unavailable(AvailabilityReason),
}
```

**[assumption]** `Capabilities` reports supported sources, OCR languages, partial-tree support, input mechanisms, and optional features separately from current grants. `ObserveConfig` selects source, sampling budget, and enabled evidence streams. `CapturePolicy` contains allow/exclude rules and redaction requirements. Access requests are explicit UI-mediated operations, never surprise prompts from startup or frame callbacks. A missing screen grant can still allow an explicitly consented metadata-only session; the engine must label its reduced evidence.

**[assumption]** `WindowContext` contains an opaque window ID, normalized app identity, sanitized title if enabled, geometry, and per-stream coverage. Native identifiers remain private diagnostic data. Roles/actions are a small common vocabulary with `Unknown` and namespaced platform extensions; UIA Invoke support is evidence of a capability, not proof the user invoked it. Do not equate mouse clicks, focus changes, and semantic actions; correlate them without triple-counting and preserve source provenance.

**[assumption]** Define desktop geometry in signed physical pixels with an explicit coordinate-space/version identifier, per-display scale/rotation, and a transform into each frame. Negative coordinates and mixed DPI are normal inputs to the contract. Tree bounds and frames are not atomic: consumers compare sample intervals/context generations and refuse associations beyond a configurable 250 ms target. Use monotonic session time for ordering and duration; a separate periodically refreshed UTC anchor supports display without trusting wall-clock jumps.

**[assumption]** The protocol uses a bounded event channel plus bounded shared-memory frame leases, with named-pipe ACLs limited to the current user on Windows. Prioritize state/permission/pause/gap events; coalesce tree/frame updates and report drops instead of blocking OS callbacks. `pause`, `stop`, and a policy change acknowledge a barrier only after callbacks are disabled or fenced, stale work is discarded, and leases/queues are revoked as applicable. The recipient drops all older epochs. Bytes already copied or sent to a model cannot be retroactively erased; the UI must distinguish stopping future capture from deleting persisted history.

**[assumption]** Shared privacy types are constructible only by the gate after successful checks; raw native samples use a different private type. `PrivacyReceipt` records checks and versions, not a mathematical guarantee. Unknown sensitivity yields a dropped observation. `Fault` contains stable codes, never raw exception text that might embed content. Schema compatibility tests cover additive fields, unknown enum handling, cross-platform replay, timestamp ordering, cancellation, epoch changes, and dropped-stream semantics. Windows permission states must not mimic macOS permission prompts: `NotRequiredByOs` still requires Macroscope consent.

## 8. Verification without the owner hand-testing Windows

GitHub currently provides x64 Windows hosted runners and Windows 11 ARM64 labels, including `windows-11-arm`; runner images and availability must be pinned/rechecked when implementation begins. [verified: [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)]

**[assumption]** Every PR runs Rust format/lint/build/unit tests on a pinned x64 Windows image, plus ARM64 compilation and native ARM tests where available. Test DTO serialization/replay, redaction geometry, source identity reuse, policy races, COM cleanup, error mapping, and packaging manifests. Run package install/OCR synthetic-image smoke tests where the runner permits them; an unsupported session is reported as infrastructure-unavailable, not a capture pass. Hosted runners are not evidence for consumer GPU, consent-dialog, border, UAC, or sleep/wake quality.

Desktop UI testing needs an interactive session; Microsoft's CI guidance explicitly requires interactive agents for desktop tests and warns that ordinary RDP disconnection can lock the desktop and break tests. [verified: [UI testing considerations](https://learn.microsoft.com/nb-no/azure/devops/pipelines/test/ui-testing-considerations?view=azure-devops)]

**[assumption]** Maintain a disposable interactive Windows 11 x64 test environment, with a runner launched inside a dedicated test user's logged-in desktop rather than as a session-zero service. Permit only trusted workflows, use synthetic data, and never provide production model/signing secrets. Capture test artifacts from both inside the guest and the outer console: the guest's own capture cannot reliably prove a border's visibility or its own excluded UI. Scripts drive a deterministic fixture with buttons, password inputs, changing labels, scrolling, resizing, and recognizable pixel markers. Upload privacy-safe screenshots, protocol traces, assertions, OS build, GPU/driver, and signed artifact hashes for Mac-side review.

Microsoft documents Parallels Windows 11 ARM as an authorized option for specified Apple Silicon Macs, and Windows 365 as a Mac-accessible cloud option. These documents do not establish that the owner's exact Mac/current Parallels combination is supported. [verified: [Windows on Mac options](https://support.microsoft.com/en-us/windows/experience/platform-variants/options-for-using-windows-11-with-mac-computers-with-apple-m1-m2-and-m3-chips)]

UTM now documents experimental accelerated DirectX for Windows 11 QEMU guests, including possible instability. A blanket claim that UTM has no Windows 3D acceleration would be outdated. [verified: [UTM Windows guest support](https://docs.getutm.app/guest-support/windows/)]

**[assumption]** Prefer an agent-operated Parallels ARM VM for repeatable local guest tests if compatible with the owner's hardware and budget. This asks the owner to host infrastructure, not manually test Windows. UTM is acceptable for build/installation/COM exploration but its experimental graphics path cannot certify WGC behavior. Use a licensed cloud Windows 11 desktop or contracted tester's x64 machine for independent coverage; do not treat a Windows Server hosted runner or a cloud RDP session as equivalent to a physical laptop. Cloud hardware selection, licensing, GPU support, and pricing remain procurement checks. If neither interactive automation nor an external Windows tester is available, keep Windows preview-only.

**[assumption]** Required beta/release evidence:

| Scenario | Acceptance evidence |
| --- | --- |
| Start, picker cancel, denied/revoked access | No frames before consent; stable explicit state; no alternate backend |
| Foreground/exclusion race | Sensitive marker never appears in emitted frames, trees, titles, input, or logs |
| Password, OCR failure, missing language, hung UIA | Mask/drop/partial states; no raw text crosses gate; UI stays responsive |
| Our windows and protected fixture | Own pixels/UIA absent; protected fixture not bypassed; unknown blank reason preserved |
| Lock, UAC, elevated app, suspend, sign-out | Explicit gaps, cleared pending buffers, validated resume |
| DPI, resize, monitor change, device loss | Correct masks/transforms or safe stop; no out-of-bounds/stale imagery |
| Signed fresh install, update, uninstall, startup disabled | Working identity/OCR; signatures verified; startup choice respected |
| Border and AV/EDR | Independent outer-view recording; signed binary checked on actual tester device |

**[assumption]** Before public Windows availability, require a non-owner Windows tester to exercise the signed package on a real x64 Windows 11 laptop, ideally Intel/AMD integrated graphics, with normal Defender/SmartScreen settings. ARM VM success cannot certify x64 drivers, HDR, multi-monitor power behavior, or enterprise EDR. Use a 60-minute synthetic work session and eight-hour soak, targeting no silent observer stoppage, no unbounded memory growth, and no sensitive fixture leaks. Establish CPU/battery thresholds from measurements before promising background efficiency. These are future release gates; none has passed in this research.

## 9. Windows delivery sequence and kill risks

**[assumption]** Stage A, alongside macOS: implement protocol/replay and Windows compile checks; spike signed MSIX, one picker-selected WGC window, UIA bounded snapshots, and OCR under package identity. Stop expansion if packaging or safe redaction cannot be made reproducible.

**[assumption]** Stage B, closed Windows beta: selected-window observation, foreground gating, normal-user UIA, local OCR, explicit exclusions, tray pause/status, diagnostic coverage, signed updates, optional sign-in startup. Mouse/typing streams remain disabled unless separately verified. Require the interactive fixture suite and an external Windows tester before inviting users with real work data.

**[assumption]** Stage C, later Windows: foreground-following across allowlisted apps, popup/dialog coverage, multiple monitors, borderless consent, mouse hooks/Raw Input aggregates, browser integration, native ARM64 parity, HDR, RDP/virtual desktop coverage, Store distribution, and enterprise policies/EDR evaluation. Privileged/secure-desktop capture and DRM bypass remain out of scope, rather than merely delayed.

**[assumption; kill risks]** The product can fail if privacy filters leak a sensitive frame, security software reasonably rejects the monitoring behavior, users cannot understand or control observation, Microsoft policy/package changes break the supported path, or Windows cannot be independently verified. A workflow engine that treats inaccessible UI as complete evidence can also recommend the wrong automation. Address these through minimized capture, explicit coverage, supported APIs, release gates, and model execution isolation. If safe Windows observation requires an administrator background recorder or disabling endpoint protection, reduce Windows scope or stop that launch.

## Open questions for the owner

- **[assumption]** What recurring budget is available for an agent-operated Windows VM/cloud desktop and an independent Windows tester? Without both interactive evidence and hardware coverage, the proposed Windows release stays a preview.
- **[assumption]** Is there a legally eligible organization for Artifact Signing, or should the first signing purchase target an individual publisher through another CA?
- **[assumption]** Is a later, selected-window Windows beta acceptable while macOS provides broader workflow discovery, or is cross-app Windows discovery a launch requirement that changes staffing and timing?

## Decisions I made

- **[assumption]** Use a normal-user native Rust observer, with a shared sanitized protocol and no model credentials or execution authority.
- **[assumption]** Start Windows with signed MSIX, Windows 11, one explicitly selected foreground window, and the visible OS border.
- **[assumption]** Use WinEvent for foreground context, bounded UIA for semantics, and Windows OCR inside the local privacy gate.
- **[assumption]** Defer keyboard hooks, broad monitor capture, borderless capture, and automatic app following until independent verification exists.
- **[assumption]** Represent every coverage gap, consent limitation, queue drop, and policy boundary explicitly.
- **[assumption]** Require interactive automated tests plus a non-owner physical Windows test before general availability.
- **[assumption]** Do not buy EV for SmartScreen, reuse Recall data, bypass protected content, or elevate the observer.
