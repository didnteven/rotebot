import AppKit
import ApplicationServices
import Darwin

struct AccessibilityEvidence {
    var labels: [String] = []
    var masks: [CaptureMask] = []
    var uncertain = false
}

// No app/window enumeration: only the selected application's focused AX window is inspected.
@MainActor
enum CaptureAccessibility {
    static func read(pid: pid_t, selectedWindow: UInt32, screenRect: CGRect?, width: Int, height: Int,
                     allowed: () -> Bool) -> AccessibilityEvidence {
        var evidence = AccessibilityEvidence()
        func value(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
            guard allowed(), AXIsProcessTrusted(),
                  AXUIElementSetMessagingTimeout(element, 0.1) == .success else { evidence.uncertain = true; return nil }
            var result: CFTypeRef?
            let error = AXUIElementCopyAttributeValue(element, key as CFString, &result)
            guard allowed() else { evidence.uncertain = true; return nil }
            if error != .success {
                if error != .noValue && error != .attributeUnsupported { evidence.uncertain = true }
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
            evidence.uncertain = true; return evidence
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
        guard windowID(window) == selectedWindow, let windowRect = bounds(window), let screenRect,
              CaptureGeometry.sameWindowBounds(windowRect, screenRect) else {
            evidence.uncertain = true; return evidence
        }
        var visited = 0, bytes = 0
        let deadline = ProcessInfo.processInfo.systemUptime + 0.8
        func walk(_ element: AXUIElement, depth: Int) {
            guard allowed(), depth <= 32, visited < 1024,
                  ProcessInfo.processInfo.systemUptime < deadline else { evidence.uncertain = true; return }
            visited += 1
            guard let role = value(element, kAXRoleAttribute) as? String else { evidence.uncertain = true; return }
            let subrole = value(element, kAXSubroleAttribute) as? String
            if role == "AXSecureTextField" || subrole == "AXSecureTextField" {
                guard let rect = bounds(element),
                      let mask = CaptureGeometry.secureField(rect, window: windowRect, width: width, height: height) else {
                    evidence.uncertain = true; return
                }
                evidence.masks.append(mask)
                return // Never read any text/value/children of a secure field.
            }
            // A missing/ambiguous visible-control geometry invalidates this frame.
            if element != window && role != "AXWindow" {
                guard let rect = bounds(element) else { evidence.uncertain = true; return }
                if !rect.intersects(windowRect) { return }
                for key in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
                    if let raw = value(element, key), CFGetTypeID(raw) == CFStringGetTypeID() {
                        let string = raw as! CFString
                        guard CFStringGetLength(string) <= 4096 else { evidence.uncertain = true; return }
                        let text = string as String
                        bytes += text.utf8.count
                        guard bytes <= 24_000, evidence.labels.count < 1024 else { evidence.uncertain = true; return }
                        if !text.isEmpty { evidence.labels.append(text) }
                    }
                }
            }
            // AXChildren may be absent for leaves. Any other read failure closes admission.
            var childrenRaw: CFTypeRef?
            let result = AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &childrenRaw)
            if result == .noValue || result == .attributeUnsupported { return }
            guard result == .success, let children = childrenRaw as? [AXUIElement], children.count <= 1024 else {
                evidence.uncertain = true; return
            }
            for child in children { walk(child, depth: depth+1) }
        }
        walk(window, depth: 0)
        guard allowed(), AXIsProcessTrusted(),
              let after = value(app, kAXFocusedWindowAttribute), CFEqual(after, window),
              windowID(window) == selectedWindow, bounds(window) == windowRect else { evidence.uncertain = true; return evidence }
        return evidence
    }
}
