import Foundation
import Testing
@testable import ObserverCore

let gmailRules = ConnectorRules(connectors: [
    ConnectorSignature(id: "gmail", titleSuffixes: [" - Gmail"],
                       titleContains: ["@gmail.com - Gmail"],
                       browserBundleIds: Array(RuleMatcher.knownBrowserBundleIds).sorted()),
])

func configuration(maxSeconds: Int = 300, rules: ConnectorRules = gmailRules,
                   exclusions: [String] = []) -> StartConfiguration {
    StartConfiguration(sessionId: "synthetic-session", maxSeconds: maxSeconds, rules: rules,
                       exclusions: Exclusions(bundleIds: exclusions), policyEpoch: 7)
}

@Suite struct RuleMatcherTests {
    @Test(arguments: Array(RuleMatcher.knownBrowserBundleIds).sorted())
    func gmailMatchesOnlyExplicitBrowsers(browser: String) {
        let matches = RuleMatcher(rules: gmailRules).match(bundleId: browser, title: "Inbox - Gmail")
        #expect(matches == [Match(connectorId: "gmail", signalKind: .windowTitle,
                                 ruleId: "gmail:titleSuffixes:0")])
    }

    @Test(arguments: ["com.apple.mail", "com.example.editor", "com.example.unknown-browser"])
    func gmailTitleInNonBrowserDoesNotMatch(bundleId: String) {
        #expect(RuleMatcher(rules: gmailRules).match(bundleId: bundleId, title: "person@gmail.com - Gmail").isEmpty)
    }

    @Test func appleMailDoesNotImplyGmail() {
        #expect(RuleMatcher(rules: gmailRules).match(bundleId: "com.apple.mail", title: nil).isEmpty)
    }

    @Test func containsAndSuffixHaveDistinctContentFreeRuleIds() {
        let matches = RuleMatcher(rules: gmailRules).match(bundleId: "com.google.Chrome",
                                                          title: "Inbox person@gmail.com - Gmail")
        #expect(matches.map(\.ruleId) == ["gmail:titleSuffixes:0", "gmail:titleContains:0"])
    }

    @Test func suffixIsAnchoredAndCaseSensitive() {
        let matcher = RuleMatcher(rules: gmailRules)
        #expect(matcher.match(bundleId: "com.google.Chrome", title: "Inbox - Gmail - Other").isEmpty)
        #expect(matcher.match(bundleId: "com.google.Chrome", title: "Inbox - gmail").isEmpty)
        #expect(matcher.match(bundleId: "com.google.Chrome", title: nil).isEmpty)
    }

    @Test func nativeBundleRulesDoNotNeedTitles() {
        let rules = ConnectorRules(connectors: [ConnectorSignature(id: "slack", bundleIds: ["com.tinyspeck.slackmacgap"])])
        let matcher = RuleMatcher(rules: rules)
        #expect(matcher.match(bundleId: "com.tinyspeck.slackmacgap", title: nil) == [
            Match(connectorId: "slack", signalKind: .foregroundApp, ruleId: "slack:bundleIds:0"),
        ])
        #expect(!matcher.needsTitle(bundleId: "com.tinyspeck.slackmacgap"))
        #expect(matcher.match(bundleId: "com.tinyspeck.slackmacgap.extra", title: nil).isEmpty)
    }

    @Test func bundleRulesCannotTurnBrowserLaunchIntoServiceDetection() {
        let rules = ConnectorRules(connectors: [
            ConnectorSignature(id: "gmail", bundleIds: ["com.google.Chrome", "com.example.browser"],
                               browserBundleIds: ["com.example.browser"]),
        ])
        for bundle in ["com.google.Chrome", "com.example.browser"] {
            #expect(RuleMatcher(rules: rules).match(bundleId: bundle, title: nil).isEmpty)
        }
    }

    @Test func browserListsAreScopedPerConnector() {
        let rules = ConnectorRules(connectors: [
            ConnectorSignature(id: "gmail", titleSuffixes: [" - Gmail"], browserBundleIds: ["com.apple.Safari"]),
            ConnectorSignature(id: "other", titleSuffixes: [" - Other"], browserBundleIds: ["com.google.Chrome"]),
        ])
        let matcher = RuleMatcher(rules: rules)
        #expect(matcher.match(bundleId: "com.google.Chrome", title: "Inbox - Gmail").isEmpty)
        #expect(matcher.match(bundleId: "com.apple.Safari", title: "Inbox - Other").isEmpty)
    }

    @Test func emptyPatternsCannotMatchEverything() {
        let rules = ConnectorRules(connectors: [
            ConnectorSignature(id: "gmail", titleSuffixes: [""], titleContains: [""],
                               browserBundleIds: ["com.google.Chrome"]),
        ])
        #expect(throws: WireError.self) { try configuration(rules: rules).validate() }
        #expect(RuleMatcher(rules: rules).match(bundleId: "com.google.Chrome", title: "anything").isEmpty)
    }

    @Test func maxDurationIsCappedAt300() throws {
        #expect(configuration(maxSeconds: 999_999).cappedMaxSeconds == 300)
        #expect(configuration(maxSeconds: 2).cappedMaxSeconds == 2)
        #expect(throws: WireError.self) { try configuration(maxSeconds: 0).validate() }
        #expect(throws: WireError.self) { try configuration(maxSeconds: -1).validate() }
    }

    @Test func duplicateIdsAndUnboundedRulesAreRejected() {
        let duplicate = ConnectorRules(connectors: [ConnectorSignature(id: "gmail"), ConnectorSignature(id: "gmail")])
        #expect(throws: WireError.self) { try configuration(rules: duplicate).validate() }
        let tooMany = ConnectorRules(connectors: (0..<129).map { ConnectorSignature(id: "c\($0)") })
        #expect(throws: WireError.self) { try configuration(rules: tooMany).validate() }
        let huge = ConnectorRules(connectors: [ConnectorSignature(id: "gmail", titleSuffixes: [String(repeating: "x", count: 1_025)])])
        #expect(throws: WireError.self) { try configuration(rules: huge).validate() }
    }
}
