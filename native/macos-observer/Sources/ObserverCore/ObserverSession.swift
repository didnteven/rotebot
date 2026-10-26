import Foundation

public struct FocusedApplication: Sendable, Equatable {
    public let bundleId: String
    public let processId: Int32
    public init(bundleId: String, processId: Int32) {
        self.bundleId = bundleId
        self.processId = processId
    }
}

public enum TitleRead {
    case title(String)
    case unavailable
    case timeout
    case revoked
}

@MainActor
public protocol ObservationSource: AnyObject {
    func accessibilityPermission() -> AccessibilityPermission
    func focusedBundleId() -> String?
    func focusedApplication(bundleId: String) -> FocusedApplication?
    func appName(for application: FocusedApplication) -> String?
    func secureInputEnabled() -> Bool
    func readTitle(for application: FocusedApplication, authorized: () -> Bool) -> TitleRead
}

/// Single-session reducer and privacy gate. All platform access goes through ObservationSource.
@MainActor
public final class ObserverSession {
    public enum Phase { case idle, active, paused, stopped }
    public private(set) var phase: Phase = .idle
    public private(set) var configuration: StartConfiguration?
    public private(set) var deadline: Double?
    private var matcher: RuleMatcher?
    private var exclusions = ExclusionPolicy.defaults
    private var startedAt: Double = 0
    private var seq: UInt64 = 0
    private var lastMonoMs: UInt64 = 0
    private var lastMatches: [Match: Double] = [:]
    private var lastFocus: FocusedApplication?
    private var sessionActive = true
    private var screensAwake = true

    public init() {}

    public func control(_ message: ControlMessage, now: Double,
                        accessibility: AccessibilityPermission) throws -> [ObserverEvent] {
        if phase == .stopped { return [] }
        if let expired = tick(now: now), !expired.isEmpty { return expired }
        switch message {
        case .start(let config):
            guard phase == .idle else { return stop(.error, now: now) }
            try config.validate()
            configuration = config
            matcher = RuleMatcher(rules: config.rules)
            exclusions.formUnion(config.exclusions.bundleIds)
            startedAt = now
            deadline = now + Double(config.cappedMaxSeconds)
            phase = .active
            return [event(.ready(accessibility), now: now)]
        case .pause:
            guard phase != .idle else { throw WireError.invalidControl }
            phase = .paused
            return [event(.gap(.paused), now: now)]
        case .resume:
            guard phase != .idle else { throw WireError.invalidControl }
            phase = .active
            lastFocus = nil
            return []
        case .stop:
            guard phase != .idle else { throw WireError.invalidControl }
            return stop(.user, now: now)
        }
    }

    public func tick(now: Double) -> [ObserverEvent]? {
        guard phase != .idle, phase != .stopped, let deadline, now >= deadline else { return nil }
        return stop(.maxDuration, now: now)
    }

    public func stop(_ reason: StopReason, now: Double) -> [ObserverEvent] {
        guard phase != .idle, phase != .stopped else { return [] }
        phase = .stopped
        matcher = nil
        lastMatches.removeAll(keepingCapacity: false)
        lastFocus = nil
        return [event(.stopped(reason), now: now)]
    }

    static let lockScreenBundleIds: Set<String> = ["com.apple.loginwindow", "com.apple.ScreenSaver.Engine"]

    public func setSessionActive(_ active: Bool, now: Double) -> [ObserverEvent] {
        sessionActive = active
        lastFocus = nil
        return boundaryGap(active, now: now)
    }

    public func setScreensAwake(_ awake: Bool, now: Double) -> [ObserverEvent] {
        screensAwake = awake
        lastFocus = nil
        return boundaryGap(awake, now: now)
    }

    private func boundaryGap(_ opening: Bool, now: Double) -> [ObserverEvent] {
        if let expired = tick(now: now) { return expired }
        guard !opening, phase == .active || phase == .paused else { return [] }
        return [event(.gap(.sessionInactive), now: now)]
    }

    public func sample(source: any ObservationSource, now: () -> Double,
                       authorized: () -> Bool) -> [ObserverEvent] {
        if let expired = tick(now: now()) { return expired }
        guard phase == .active || phase == .paused, authorized() else { return [] }
        if phase == .paused { return [event(.gap(.paused), now: now())] }
        guard sessionActive && screensAwake else { return [event(.gap(.sessionInactive), now: now())] }
        guard let bundleId = source.focusedBundleId() else {
            return [event(.gap(.sessionInactive), now: now())]
        }
        // A locked screen or screen saver is frontmost while the session is not the
        // user's: that is a lock boundary, never an app the user worked in.
        guard !ObserverSession.lockScreenBundleIds.contains(bundleId) else {
            lastFocus = nil
            return [event(.gap(.sessionInactive), now: now())]
        }
        // The bundle ID is the only application metadata obtained before exclusion.
        guard !exclusions.contains(bundleId) else {
            lastFocus = nil
            return [event(.gap(.excluded), now: now())]
        }
        guard authorized() else { return [] }
        guard !source.secureInputEnabled() else { return [event(.gap(.secureInput), now: now())] }
        guard let application = source.focusedApplication(bundleId: bundleId) else {
            return [event(.gap(.sessionInactive), now: now())]
        }
        guard let matcher else { return [] }

        var payloads: [EventPayload] = []
        if lastFocus != application {
            guard authorized() else { return [] }
            guard let name = source.appName(for: application), name.utf8.count <= 512,
                  application.bundleId.utf8.count <= 256 else {
                return [event(.gap(.axUnavailable), now: now())]
            }
            payloads.append(.appFocus(bundleId: application.bundleId, appName: name))
        }
        var matches = matcher.match(bundleId: application.bundleId, title: nil)
        if matcher.needsTitle(bundleId: application.bundleId) {
            if source.accessibilityPermission() != .granted {
                payloads.append(.gap(.axUnavailable))
            } else {
                // Check secure input again immediately before the bounded AX read.
                guard authorized(), !source.secureInputEnabled() else {
                    return authorized() ? [event(.gap(.secureInput), now: now())] : []
                }
                switch source.readTitle(for: application, authorized: {
                    authorized() && self.deadline.map { now() < $0 } == true
                }) {
                case .title(let title):
                    // Raw title is consumed here and is never retained in session state.
                    matches += matcher.match(bundleId: application.bundleId, title: title)
                case .unavailable: payloads.append(.gap(.axUnavailable))
                case .timeout: payloads.append(.gap(.timeout))
                case .revoked: return tick(now: now()) ?? []
                }
            }
        }
        if let expired = tick(now: now()) { return expired }
        guard authorized(), phase == .active else { return [] }
        lastFocus = application
        for match in matches {
            if let last = lastMatches[match], now() - last < 30 { continue }
            lastMatches[match] = now()
            payloads.append(.connectorMatch(match))
        }
        return payloads.map { event($0, now: now()) }
    }

    private func event(_ payload: EventPayload, now: Double) -> ObserverEvent {
        precondition(configuration != nil)
        seq += 1
        lastMonoMs = max(lastMonoMs, UInt64(max(0, (now - startedAt) * 1_000)))
        return ObserverEvent(sessionId: configuration!.sessionId, seq: seq, monoMs: lastMonoMs,
                             policyEpoch: configuration!.policyEpoch, payload: payload)
    }
}
