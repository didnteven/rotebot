import AppKit
import ApplicationServices
import Darwin

struct AccessibilityEvidence {
    var labels: [String] = []
    var masks: [CaptureMask] = []
    var uncertain = false
    var why: String?
}

// No app/window enumeration: only the selected application's focused AX window is inspected.
@MainActor
enum CaptureAccessibility {
    static func read(pid: pid_t, selectedWindow: UInt32, screenRect: CGRect?, width: Int, height: Int,
                     allowed: () -> Bool) -> AccessibilityEvidence {
        var evidence = AccessibilityEvidence()
        func mark(_ code: String) { evidence.uncertain = true; if evidence.why == nil { evidence.why = code } }
        func value(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
            guard allowed(), AXIsProcessTrusted(),
                  AXUIElementSetMessagingTimeout(element, 0.1) == .success else { mark("a1"); return nil }
            var result: CFTypeRef?
            let error = AXUIElementCopyAttributeValue(element, key as CFString, &result)
            guard allowed() else { mark("a2"); return nil }
            if error != .success {
                if error != .noValue && error != .attributeUnsupported { mark("a3") }
                return nil
            }
            return result
        }
        func bounds(_ element: AXUIElement) -> CGRect? {
            guard let p = value(element, kAXPositionAttribute), CFGetTypeID(p) == AXValueGetTypeID(),
                  let s = value(element, kAXSizeAttribute), CFGetTypeID(s) == AXValueGetTypeID() else { return nil }
            var point = CGPoint.zero, size = CGSize.zero
            guard AXValueGetValue(p as! AXValue, .cgPoint, &point),
                  AXValueGetValue(s as! AXValue, .cgSize, &size), size.width > 0, size.height > 0 else { return nil }
            return CGRect(origin: point, size: size)
        }
        let app = AXUIElementCreateApplication(pid)
        guard AXIsProcessTrusted(), let raw = value(app, kAXFocusedWindowAttribute),
              CFGetTypeID(raw) == AXUIElementGetTypeID() else {
            mark("a4"); return evidence
        }
        let window = raw as! AXUIElement
        // Fail closed if the provider cannot bind AX to the picker-selected CGWindowID.
        // Some AX providers expose AXWindowNumber. The OS AX bridge supplies the exact
        // CGWindowID otherwise; absent bridge/provider support closes admission. This
        // optional SPI must be validated for each supported macOS release (see Stage C).
        func windowID(_ element: AXUIElement) -> UInt32? {
            var raw: CFTypeRef?
            if AXUIElementCopyAttributeValue(element, "AXWindowNumber" as CFString, &raw) == .success,
               let number = raw as? NSNumber { return number.uint32Value }
            guard let handle = dlopen(nil, RTLD_LAZY) else { return nil }
            defer { dlclose(handle) }
            guard let symbol = dlsym(handle, "_AXUIElementGetWindow") else { return nil }
            typealias Lookup = @convention(c) (AXUIElement, UnsafeMutablePointer<CGWindowID>) -> AXError
            let lookup = unsafeBitCast(symbol, to: Lookup.self)
            var id: CGWindowID = 0
            guard allowed(), lookup(element, &id) == .success, id != 0 else { return nil }
            return id
        }
        guard windowID(window) == selectedWindow else { mark("window_id"); return evidence }
        guard let windowRect = bounds(window), let screenRect else { mark("window_bounds"); return evidence }
        guard CaptureGeometry.sameWindowBounds(windowRect, screenRect) else { mark("bounds_differ"); return evidence }
        var visited = 0, bytes = 0
        let deadline = ProcessInfo.processInfo.systemUptime + 0.8
        func walk(_ element: AXUIElement, depth: Int) {
            guard allowed(), depth <= 32, visited < 1024,
                  ProcessInfo.processInfo.systemUptime < deadline else { mark("a6"); return }
            visited += 1
            guard let role = value(element, kAXRoleAttribute) as? String else { mark("a7"); return }
            let subrole = value(element, kAXSubroleAttribute) as? String
            if role == "AXSecureTextField" || subrole == "AXSecureTextField" {
                guard let rect = bounds(element),
                      let mask = CaptureGeometry.secureField(rect, window: windowRect, width: width, height: height) else {
                    mark("a8"); return
                }
                evidence.masks.append(mask)
                return // Never read any text/value/children of a secure field.
            }
            // Labels are only extra notes (masking comes from pixels and secure fields), so an element
            // without readable geometry just contributes no label; its children are still walked.
            if element != window && role != "AXWindow", let rect = bounds(element) {
                if !rect.intersects(windowRect) { return }
                for key in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
                    if let raw = value(element, key), CFGetTypeID(raw) == CFStringGetTypeID() {
                        let string = raw as! CFString
                        // Over-long or too many labels are simply not collected (they are notes only).
                        guard CFStringGetLength(string) <= 4096 else { continue }
                        let text = string as String
                        guard bytes + text.utf8.count <= 24_000, evidence.labels.count < 1024 else { continue }
                        bytes += text.utf8.count
                        if !text.isEmpty { evidence.labels.append(text) }
                    }
                }
            }
            // AXChildren may be absent for leaves. Any other read failure closes admission.
            var childrenRaw: CFTypeRef?
            let result = AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &childrenRaw)
            if result == .noValue || result == .attributeUnsupported { return }
            guard result == .success, let children = childrenRaw as? [AXUIElement], children.count <= 1024 else {
                mark("a12"); return
            }
            for child in children { walk(child, depth: depth+1) }
        }
        walk(window, depth: 0)
        guard allowed(), AXIsProcessTrusted(),
              let after = value(app, kAXFocusedWindowAttribute), CFEqual(after, window),
              windowID(window) == selectedWindow, bounds(window) == windowRect else { mark("a13"); return evidence }
        return evidence
    }
}
