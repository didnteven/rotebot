import AppKit
import ApplicationServices
import Carbon.HIToolbox

@MainActor
final class MacOSObservationSource: ObservationSource {
    func accessibilityPermission() -> AccessibilityPermission {
        AXIsProcessTrusted() ? .granted : .denied
    }

    func focusedBundleId() -> String? {
        NSWorkspace.shared.frontmostApplication?.bundleIdentifier
    }

    func focusedApplication(bundleId: String) -> FocusedApplication? {
        guard let app = NSWorkspace.shared.frontmostApplication, app.bundleIdentifier == bundleId else {
            return nil
        }
        return FocusedApplication(bundleId: bundleId, processId: app.processIdentifier)
    }

    func appName(for application: FocusedApplication) -> String? {
        guard let app = NSRunningApplication(processIdentifier: application.processId),
              app.bundleIdentifier == application.bundleId else { return nil }
        return app.localizedName
    }

    func secureInputEnabled() -> Bool { IsSecureEventInputEnabled() }

    func readTitle(for application: FocusedApplication, authorized: () -> Bool) -> TitleRead {
        func safe() -> Bool {
            authorized() && AXIsProcessTrusted() && !secureInputEnabled() &&
                focusedApplication(bundleId: application.bundleId) == application
        }
        guard safe() else { return .revoked }
        let app = AXUIElementCreateApplication(application.processId)
        guard AXUIElementSetMessagingTimeout(app, 0.25) == .success else { return .unavailable }
        var windowValue: CFTypeRef?
        guard safe() else { return .revoked }
        let windowError = AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &windowValue)
        guard safe() else { return .revoked }
        guard windowError == .success else { return classify(windowError) }
        guard let windowValue, CFGetTypeID(windowValue) == AXUIElementGetTypeID() else { return .unavailable }
        let window = windowValue as! AXUIElement
        guard AXUIElementSetMessagingTimeout(window, 0.25) == .success else { return .unavailable }
        guard safe() else { return .revoked }
        var titleValue: CFTypeRef?
        let titleError = AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &titleValue)
        guard safe() else { return .revoked }
        guard titleError == .success else { return classify(titleError) }
        guard let titleValue, CFGetTypeID(titleValue) == CFStringGetTypeID() else { return .unavailable }
        let title = titleValue as! CFString
        // Reject oversized values before making a Swift String copy; never inspect other attributes.
        guard CFStringGetLength(title) <= 8_192 else { return .unavailable }
        return .title(title as String)
    }

    private func classify(_ error: AXError) -> TitleRead {
        error == .cannotComplete ? .timeout : .unavailable
    }
}
