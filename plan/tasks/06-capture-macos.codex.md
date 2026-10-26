Track: CAPTURE LAYER ON macOS. Output: plan/research/06-capture-macos.md
Use web search for current (macOS 15/26) APIs and gotchas. Plan the macOS observer in detail:
- Screen: ScreenCaptureKit (SCStream, per-window vs per-display, excluding our own windows, frame rate, the recurring "screen recording" permission re-prompt behaviour in macOS 15+ and the purple indicator, private window detection).
- Structure: Accessibility API (AXUIElement, AXObserver notifications, focused element, window titles, browser URL via AX or AppleScript), NSWorkspace app activation, CGEventTap / NSEvent global monitors for clicks and keystroke *timing/shortcuts* without logging typed content, secure input mode detection (password fields).
- Browser: is a companion browser extension a better source of DOM events/URLs than screenshots? (Probably yes for web apps.) Plan it.
- Change detection: when to keep a frame (perceptual hash diff, AX tree change, app switch, idle), target storage/CPU budget (e.g. under 3% CPU, under X GB/week), compression (HEIC/WebP), retention.
- On-device OCR with Apple Vision (VNRecognizeTextRequest) and text-to-structure.
- Permissions onboarding sequence (Screen Recording, Accessibility, Input Monitoring, Automation/Apple Events), what each one unlocks, graceful degraded modes when denied, how to detect revocation.
- Distribution constraints: Developer ID + notarization outside the Mac App Store (the App Store sandbox almost certainly forbids this; confirm), hardened runtime entitlements, helper/login item (SMAppService), launch at login.
- Which language for the capture helper: Swift helper vs Rust (objc2 / screencapturekit-rs) and why.
