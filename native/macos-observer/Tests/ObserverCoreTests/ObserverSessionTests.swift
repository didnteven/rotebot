import Foundation
import Testing
@testable import ObserverCore

@MainActor
final class SyntheticSource: ObservationSource {
    var permission: AccessibilityPermission = .granted
    var application: FocusedApplication? = FocusedApplication(bundleId: "com.google.Chrome", processId: 123)
    var name = "Google Chrome"
    var secure = false
    var titleRead: TitleRead = .title("Inbox person@gmail.com - Gmail")
    var onRead: (() -> Void)?
    var focusReads = 0
    var applicationReads = 0
    var nameReads = 0
    var secureChecks = 0
    var titleReads = 0
    func accessibilityPermission() -> AccessibilityPermission { permission }
    func focusedBundleId() -> String? { focusReads += 1; return application?.bundleId }
    func focusedApplication(bundleId: String) -> FocusedApplication? { applicationReads += 1; return application }
    func appName(for application: FocusedApplication) -> String? { nameReads += 1; return name }
    func secureInputEnabled() -> Bool { secureChecks += 1; return secure }
    func readTitle(for application: FocusedApplication, authorized: () -> Bool) -> TitleRead {
        titleReads += 1
        onRead?()
        return titleRead
    }
}

@Suite @MainActor struct ObserverSessionTests {
    func started(_ config: StartConfiguration = configuration()) throws -> ObserverSession {
        let session = ObserverSession()
        let events = try session.control(.start(config), now: 10, accessibility: .granted)
        #expect(events.map(\.payload) == [.ready(.granted)])
        return session
    }

    func sample(_ session: ObserverSession, _ source: SyntheticSource, at now: Double = 11) -> [ObserverEvent] {
        session.sample(source: source, now: { now }, authorized: { true })
    }

    @Test(arguments: Array(ExclusionPolicy.defaults).sorted() + ["com.example.private"])
    func excludedAppYieldsOnlyGapAndNoFurtherReads(bundleId: String) throws {
        let session = try started(configuration(exclusions: ["com.example.private"]))
        let source = SyntheticSource()
        source.application = FocusedApplication(bundleId: bundleId, processId: 123)
        #expect(sample(session, source).map(\.payload) == [.gap(.excluded)])
        #expect(source.focusReads == 1)
        #expect(source.applicationReads == 0)
        #expect(source.nameReads == 0)
        #expect(source.secureChecks == 0)
        #expect(source.titleReads == 0)
    }

    @Test(arguments: ["com.apple.loginwindow", "com.apple.ScreenSaver.Engine"])
    func lockScreenIsAGapNotAnAppFocus(bundleId: String) throws {
        let session = try started()
        let source = SyntheticSource()
        source.application = FocusedApplication(bundleId: bundleId, processId: 1)
        #expect(sample(session, source).map(\.payload) == [.gap(.sessionInactive)])
        #expect(source.nameReads == 0)
        #expect(source.titleReads == 0)
    }

    @Test func pauseAndResumeCloseAndReopenAdmission() throws {
        let session = try started()
        let source = SyntheticSource()
        #expect(try session.control(.pause, now: 11, accessibility: .granted).map(\.payload) == [.gap(.paused)])
        #expect(sample(session, source).map(\.payload) == [.gap(.paused)])
        #expect(source.focusReads == 0)
        _ = try session.control(.resume, now: 12, accessibility: .granted)
        #expect(sample(session, source, at: 13).contains { if case .connectorMatch = $0.payload { true } else { false } })
    }

    @Test func sessionAndScreenGatesMustBothBeOpen() throws {
        let session = try started()
        let source = SyntheticSource()
        #expect(session.setSessionActive(false, now: 11).map(\.payload) == [.gap(.sessionInactive)])
        _ = session.setScreensAwake(false, now: 11)
        _ = session.setSessionActive(true, now: 12)
        #expect(sample(session, source, at: 13).map(\.payload) == [.gap(.sessionInactive)])
        #expect(source.focusReads == 0)
        _ = session.setScreensAwake(true, now: 14)
        #expect(!sample(session, source, at: 15).isEmpty)
    }

    @Test func secureInputPreventsTitleAndNameRead() throws {
        let session = try started()
        let source = SyntheticSource()
        source.secure = true
        #expect(sample(session, source).map(\.payload) == [.gap(.secureInput)])
        #expect(source.titleReads == 0)
        #expect(source.nameReads == 0)
    }

    @Test func deniedAccessibilityStillAllowsAppFocus() throws {
        let session = ObserverSession()
        let ready = try session.control(.start(configuration()), now: 10, accessibility: .denied)
        #expect(ready.map(\.payload) == [.ready(.denied)])
        let source = SyntheticSource()
        source.permission = .denied
        #expect(sample(session, source).map(\.payload) == [
            .appFocus(bundleId: "com.google.Chrome", appName: "Google Chrome"), .gap(.axUnavailable),
        ])
        #expect(source.titleReads == 0)
        source.permission = .granted
        #expect(sample(session, source, at: 12).contains { if case .connectorMatch = $0.payload { true } else { false } })
        source.permission = .denied
        #expect(sample(session, source, at: 13).map(\.payload) == [.gap(.axUnavailable)])
        #expect(source.titleReads == 1)
    }

    @Test func nativeDetectionWorksWithoutAccessibilityOrTitleAccess() throws {
        let rules = ConnectorRules(connectors: [ConnectorSignature(id: "mail", bundleIds: ["com.apple.mail"])])
        let session = try started(configuration(rules: rules))
        let source = SyntheticSource()
        source.application = FocusedApplication(bundleId: "com.apple.mail", processId: 123)
        source.permission = .denied
        #expect(sample(session, source).map(\.payload).contains(.connectorMatch(
            Match(connectorId: "mail", signalKind: .foregroundApp, ruleId: "mail:bundleIds:0"))))
        #expect(source.titleReads == 0)
    }

    @Test func nonBrowserGmailTitleIsNeverRead() throws {
        let session = try started()
        let source = SyntheticSource()
        source.application = FocusedApplication(bundleId: "com.example.editor", processId: 123)
        #expect(sample(session, source).count == 1)
        #expect(source.titleReads == 0)
    }

    @Test func unavailableAndTimeoutAreContentFreeGaps() throws {
        for (result, reason) in [(TitleRead.unavailable, GapReason.axUnavailable), (.timeout, .timeout)] {
            let session = try started()
            let source = SyntheticSource()
            source.titleRead = result
            #expect(sample(session, source).map(\.payload).contains(.gap(reason)))
        }
    }

    @Test func emailTitleNeverAppearsInAnyEmittedJSON() throws {
        let session = ObserverSession()
        var events = try session.control(.start(configuration()), now: 10, accessibility: .granted)
        let source = SyntheticSource()
        source.titleRead = .title("Inbox (10) confidential.person@gmail.com - Gmail")
        events += sample(session, source)
        events += try session.control(.pause, now: 12, accessibility: .granted)
        events += session.stop(.user, now: 13)
        #expect(events.contains { if case .connectorMatch = $0.payload { true } else { false } })
        for event in events {
            let line = try event.encodedLine()
            let json = String(decoding: line, as: UTF8.self)
            #expect(!json.contains("confidential.person"))
            #expect(!json.contains("@gmail.com"))
            #expect(!json.contains("Inbox (10)"))
            #expect(!json.contains("titleSuffixes\":") && !json.contains("\"title\":"))
            #expect(line.count <= WireLimits.lineBytes)
            let object = try #require(JSONSerialization.jsonObject(with: line) as? [String: Any])
            #expect(object["v"] as? Int == 1)
            #expect(object["sessionId"] as? String == "synthetic-session")
            #expect(object["policyEpoch"] as? Int == 7)
        }
        #expect(events.map(\.seq) == Array(1...UInt64(events.count)))
        #expect(events.map(\.monoMs) == events.map(\.monoMs).sorted())
    }

    @Test func exactMatchesAreDebouncedFor30SecondsAcrossPause() throws {
        let session = try started()
        let source = SyntheticSource()
        func count(_ time: Double) -> Int {
            sample(session, source, at: time).filter { if case .connectorMatch = $0.payload { true } else { false } }.count
        }
        #expect(count(11) == 2)
        #expect(count(13) == 0)
        _ = try session.control(.pause, now: 14, accessibility: .granted)
        _ = try session.control(.resume, now: 15, accessibility: .granted)
        #expect(count(40.999) == 0)
        #expect(count(41) == 2)
        #expect(count(43) == 0)
    }

    @Test func differentRulesAndConnectorsHaveIndependentDebounce() throws {
        let rules = ConnectorRules(connectors: [
            ConnectorSignature(id: "one", titleSuffixes: ["One", "Two"], browserBundleIds: ["com.google.Chrome"]),
            ConnectorSignature(id: "two", titleSuffixes: ["Two"], browserBundleIds: ["com.google.Chrome"]),
        ])
        let session = try started(configuration(rules: rules))
        let source = SyntheticSource()
        source.titleRead = .title("One")
        #expect(sample(session, source).filter { if case .connectorMatch = $0.payload { true } else { false } }.count == 1)
        source.titleRead = .title("Two")
        #expect(sample(session, source, at: 12).filter { if case .connectorMatch = $0.payload { true } else { false } }.count == 2)
    }

    @Test func noPermissionOrFocusReadBeforeStartOrAfterStop() throws {
        let session = ObserverSession()
        let source = SyntheticSource()
        #expect(sample(session, source).isEmpty)
        _ = try session.control(.start(configuration()), now: 10, accessibility: .granted)
        #expect(session.stop(.eof, now: 11).map(\.payload) == [.stopped(.eof)])
        #expect(sample(session, source).isEmpty)
        #expect(session.stop(.user, now: 12).isEmpty)
        #expect(source.focusReads == 0)
    }

    @Test func durationCapIncludesPauseAndIsCheckedBeforeReads() throws {
        let session = try started(configuration(maxSeconds: 9_999))
        #expect(session.deadline == 310)
        _ = try session.control(.pause, now: 11, accessibility: .granted)
        let source = SyntheticSource()
        #expect(sample(session, source, at: 310).map(\.payload) == [.stopped(.maxDuration)])
        #expect(source.focusReads == 0)
    }

    @Test func authorizationRevokedDuringAXDiscardsCandidate() throws {
        let session = try started()
        let source = SyntheticSource()
        var authorized = true
        source.onRead = { authorized = false }
        #expect(session.sample(source: source, now: { 11 }, authorized: { authorized }).isEmpty)
        #expect(session.stop(.eof, now: 12).map(\.payload) == [.stopped(.eof)])
    }

    @Test func deadlineDuringAXDiscardsCandidateAndStops() throws {
        let session = try started(configuration(maxSeconds: 2))
        let source = SyntheticSource()
        var now = 11.0
        source.onRead = { now = 12 }
        #expect(session.sample(source: source, now: { now }, authorized: { true }).map(\.payload) == [.stopped(.maxDuration)])
    }

    @Test func noAuthorityMeansNoReads() throws {
        let session = try started()
        let source = SyntheticSource()
        #expect(session.sample(source: source, now: { 11 }, authorized: { false }).isEmpty)
        #expect(source.focusReads == 0)
    }

    @Test func secondStartFailsClosed() throws {
        let session = try started()
        #expect(try session.control(.start(configuration()), now: 11, accessibility: .granted).map(\.payload) == [.stopped(.error)])
    }
}
