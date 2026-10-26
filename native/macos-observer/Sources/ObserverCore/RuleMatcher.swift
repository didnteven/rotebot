import Foundation

public struct RuleMatcher: Sendable {
    public static let knownBrowserBundleIds: Set<String> = [
        "com.google.Chrome", "com.apple.Safari", "company.thebrowser.Browser",
        "org.mozilla.firefox", "com.microsoft.edgemac", "com.brave.Browser",
    ]

    private let rules: ConnectorRules
    private let browsers: Set<String>

    public init(rules: ConnectorRules) {
        self.rules = rules
        browsers = Self.knownBrowserBundleIds.union(rules.connectors.flatMap(\.browserBundleIds))
    }

    public func needsTitle(bundleId: String) -> Bool {
        rules.connectors.contains {
            $0.browserBundleIds.contains(bundleId) && (!$0.titleSuffixes.isEmpty || !$0.titleContains.isEmpty)
        }
    }

    /// Case-sensitive literal rules, scoped to each connector's explicit browsers.
    /// Rule identifiers describe positions, never the matched text.
    public func match(bundleId: String, title: String?) -> [Match] {
        var matches: [Match] = []
        for connector in rules.connectors {
            func append(_ kind: SignalKind, _ field: String, _ index: Int) {
                matches.append(Match(connectorId: connector.id, signalKind: kind,
                                     ruleId: "\(connector.id):\(field):\(index)"))
            }
            if !browsers.contains(bundleId) {
                for (index, bundle) in connector.bundleIds.enumerated() where bundle == bundleId {
                    append(.foregroundApp, "bundleIds", index)
                }
            }
            guard connector.browserBundleIds.contains(bundleId), let title else { continue }
            for (index, suffix) in connector.titleSuffixes.enumerated()
                where !suffix.isEmpty && title.hasSuffix(suffix) {
                append(.windowTitle, "titleSuffixes", index)
            }
            for (index, substring) in connector.titleContains.enumerated()
                where !substring.isEmpty && title.contains(substring) {
                append(.windowTitle, "titleContains", index)
            }
        }
        return matches
    }
}

public enum ExclusionPolicy {
    public static let defaults: Set<String> = [
        "com.1password.1password", "com.agilebits.onepassword7", "com.bitwarden.desktop",
        "com.apple.keychainaccess", "com.apple.Passwords",
        "com.apple.Terminal", "com.googlecode.iterm2", "dev.warp.Warp-Stable",
        "com.apple.MobileSMS", "net.whatsapp.WhatsApp", "ru.keepcoder.Telegram",
        // Reserved synthetic placeholders, not a claim of universal category detection.
        "com.example.finance", "com.example.health",
    ]
}
