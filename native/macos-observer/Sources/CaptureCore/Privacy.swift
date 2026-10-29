import Foundation
import ObserverCore

public enum CaptureAdmission {
    public static let denied = ExclusionPolicy.defaults.union([
        "dev.macroscope.desktop", "dev.macroscope.capture", "com.apple.systempreferences",
        "com.apple.SecurityAgent", "com.apple.loginwindow", "com.apple.ScreenSaver.Engine",
        "com.apple.systemuiserver", "com.dashlane.Dashlane", "com.lastpass.LastPass",
        "com.enpass.Enpass", "com.keepersecurity.keeper", "org.keepassxc.keepassxc",
    ])
    public static let browsers = RuleMatcher.knownBrowserBundleIds.union([
        "com.apple.SafariTechnologyPreview", "com.google.Chrome.canary", "org.chromium.Chromium",
        "com.operasoftware.Opera", "com.vivaldi.Vivaldi", "app.zen-browser.zen",
        "org.mozilla.firefoxdeveloperedition", "com.kagi.kagimacOS",
    ])
    public enum BrowserPrivacy: Sendable { case publicVerified, `private`, unknown }
    public static func refusal(bundleId: String?, isBrowser: Bool, privacy: BrowserPrivacy) -> String? {
        guard let bundleId else { return "unknown_application" }
        if denied.contains(bundleId) || bundleId.hasPrefix("dev.macroscope.") { return "excluded" }
        // Runtime supplies .unknown for every browser: picker/title/AX do not attest public status.
        if isBrowser || browsers.contains(bundleId) {
            switch privacy { case .publicVerified: return nil; default: return "private_or_unknown_window" }
        }
        return nil
    }
}

public enum CaptureGeometry {
    public static func sameWindowBounds(_ ax: CGRect, _ captured: CGRect) -> Bool {
        let values = [ax.minX, ax.minY, ax.width, ax.height, captured.minX, captured.minY, captured.width, captured.height]
        return values.allSatisfy { $0.isFinite } && ax.width > 0 && ax.height > 0
            && abs(ax.minX-captured.minX) <= 0.5 && abs(ax.minY-captured.minY) <= 0.5
            && abs(ax.width-captured.width) <= 0.5 && abs(ax.height-captured.height) <= 0.5
    }
    // Round outward to cover the whole glyph/secure field. Invalid/empty boxes close the gate.
    public static func topLeft(_ rect: CGRect, width: Int, height: Int) -> PixelRect? {
        guard width > 0, height > 0, rect.minX.isFinite, rect.minY.isFinite,
              rect.width.isFinite, rect.height.isFinite, rect.width > 0, rect.height > 0 else { return nil }
        let x0 = floor(max(0, min(CGFloat(width), rect.minX)))
        let y0 = floor(max(0, min(CGFloat(height), rect.minY)))
        let x1 = ceil(max(0, min(CGFloat(width), rect.maxX)))
        let y1 = ceil(max(0, min(CGFloat(height), rect.maxY)))
        guard x1 > x0, y1 > y0 else { return nil }
        return PixelRect(x: Int(x0), y: Int(y0), width: Int(x1-x0), height: Int(y1-y0))
    }
    public static func vision(_ normalized: CGRect, width: Int, height: Int) -> PixelRect? {
        topLeft(CGRect(x: normalized.minX * CGFloat(width),
                       y: (1-normalized.maxY) * CGFloat(height),
                       width: normalized.width * CGFloat(width), height: normalized.height * CGFloat(height)),
                width: width, height: height)
    }
    public static func secureField(_ screenRect: CGRect, window: CGRect, width: Int, height: Int) -> CaptureMask? {
        guard window.width > 0, window.height > 0 else { return nil }
        let sx = CGFloat(width)/window.width, sy = CGFloat(height)/window.height
        return topLeft(CGRect(x: (screenRect.minX-window.minX)*sx, y: (screenRect.minY-window.minY)*sy,
                              width: screenRect.width*sx, height: screenRect.height*sy), width: width, height: height)
            .map { CaptureMask(rect: $0) }
    }
}

public struct FrameDifference {
    private var previous: [UInt8]?
    public init() {}
    // Small luma grid, plus a changed-pixel fraction: catches small edits without retaining raw frames.
    public mutating func changed(_ rgba: Data, width: Int, height: Int) -> Bool {
        guard width > 0, height > 0, width <= 1600, height <= 1600,
              rgba.count == width*height*4 else { return false }
        var grid: [UInt8] = []
        rgba.withUnsafeBytes { (p: UnsafeRawBufferPointer) in
            for y in 0..<64 { for x in 0..<64 {
                let i = ((y*height/64)*width + x*width/64)*4
                let red = Int(p[i])*77
                let green = Int(p[i+1])*150
                let blue = Int(p[i+2])*29
                grid.append(UInt8((red+green+blue)/256))
            }}
        }
        guard let previous else { self.previous = grid; return true }
        let changed = zip(previous, grid).filter { abs(Int($0)-Int($1)) >= 8 }.count >= 3
        if changed { self.previous = grid }
        return changed
    }
}
