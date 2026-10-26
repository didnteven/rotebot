import Foundation

public enum WireLimits {
    // Includes the terminating LF. No control or event line can exceed 64 KiB.
    public static let lineBytes = 65_536
    public static let pendingControls = 8
    public static let outputBytes = 262_144
}

public struct ConnectorSignature: Codable, Sendable, Equatable {
    public let id: String
    public let bundleIds: [String]
    public let titleSuffixes: [String]
    public let titleContains: [String]
    public let browserBundleIds: [String]

    public init(id: String, bundleIds: [String] = [], titleSuffixes: [String] = [],
                titleContains: [String] = [], browserBundleIds: [String] = []) {
        self.id = id
        self.bundleIds = bundleIds
        self.titleSuffixes = titleSuffixes
        self.titleContains = titleContains
        self.browserBundleIds = browserBundleIds
    }
}

public struct ConnectorRules: Codable, Sendable, Equatable {
    public let connectors: [ConnectorSignature]
    public init(connectors: [ConnectorSignature]) { self.connectors = connectors }
}

public struct Exclusions: Codable, Sendable, Equatable {
    public let bundleIds: [String]
    public init(bundleIds: [String] = []) { self.bundleIds = bundleIds }
}

public struct StartConfiguration: Codable, Sendable, Equatable {
    public let sessionId: String
    public let maxSeconds: Int
    public let rules: ConnectorRules
    public let exclusions: Exclusions
    public let policyEpoch: UInt64

    public var cappedMaxSeconds: Int { min(300, maxSeconds) }

    public init(sessionId: String, maxSeconds: Int, rules: ConnectorRules,
                exclusions: Exclusions = Exclusions(), policyEpoch: UInt64) {
        self.sessionId = sessionId
        self.maxSeconds = maxSeconds
        self.rules = rules
        self.exclusions = exclusions
        self.policyEpoch = policyEpoch
    }

    public func validate() throws {
        func valid(_ value: String, limit: Int) -> Bool {
            !value.isEmpty && value.utf8.count <= limit &&
                !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
        }
        guard valid(sessionId, limit: 128), maxSeconds > 0, rules.connectors.count <= 128,
              exclusions.bundleIds.count <= 256,
              exclusions.bundleIds.allSatisfy({ valid($0, limit: 256) }) else {
            throw WireError.invalidControl
        }
        var ids = Set<String>()
        var ruleCount = 0
        for connector in rules.connectors {
            let bundles = connector.bundleIds + connector.browserBundleIds
            let titles = connector.titleSuffixes + connector.titleContains
            ruleCount += bundles.count + titles.count
            guard valid(connector.id, limit: 128), ids.insert(connector.id).inserted,
                  ruleCount <= 1_024, bundles.allSatisfy({ valid($0, limit: 256) }),
                  titles.allSatisfy({ valid($0, limit: 1_024) }) else {
                throw WireError.invalidControl
            }
        }
    }
}

public enum ControlMessage: Decodable, Sendable, Equatable {
    case start(StartConfiguration)
    case pause
    case resume
    case stop

    private enum CodingKeys: String, CodingKey { case type }
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "start":
            let configuration = try StartConfiguration(from: decoder)
            try configuration.validate()
            self = .start(configuration)
        case "pause": self = .pause
        case "resume": self = .resume
        case "stop": self = .stop
        default: throw WireError.invalidControl
        }
    }
}

public enum WireError: Error { case invalidControl, oversizedLine, backlog, output }

public enum GapReason: String, Codable, Sendable {
    case paused, excluded, axUnavailable = "ax_unavailable", secureInput = "secure_input"
    case sessionInactive = "session_inactive", timeout
}

public enum StopReason: String, Codable, Sendable { case user, maxDuration = "max_duration", eof, error }
public enum SignalKind: String, Codable, Sendable, Hashable { case foregroundApp = "foreground_app", windowTitle = "window_title" }
public enum AccessibilityPermission: String, Codable, Sendable { case granted, denied }

public struct Match: Codable, Sendable, Equatable, Hashable {
    public let connectorId: String
    public let signalKind: SignalKind
    public let ruleId: String
}

public enum EventPayload: Sendable, Equatable {
    case ready(AccessibilityPermission)
    case appFocus(bundleId: String, appName: String)
    case connectorMatch(Match)
    case gap(GapReason)
    case stopped(StopReason)
}

// Only these cleared fields are encodable. No title-bearing DTO exists on the event path.
public struct ObserverEvent: Encodable, Sendable, Equatable {
    public let sessionId: String
    public let seq: UInt64
    public let monoMs: UInt64
    public let policyEpoch: UInt64
    public let payload: EventPayload

    private enum CodingKeys: String, CodingKey {
        case v, sessionId, seq, monoMs, policyEpoch, type, permissions, accessibility
        case bundleId, appName, connectorId, signalKind, ruleId, reason
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(1, forKey: .v)
        try container.encode(sessionId, forKey: .sessionId)
        try container.encode(seq, forKey: .seq)
        try container.encode(monoMs, forKey: .monoMs)
        try container.encode(policyEpoch, forKey: .policyEpoch)
        switch payload {
        case .ready(let accessibility):
            try container.encode("ready", forKey: .type)
            var permissions = container.nestedContainer(keyedBy: CodingKeys.self, forKey: .permissions)
            try permissions.encode(accessibility, forKey: .accessibility)
        case .appFocus(let bundleId, let appName):
            try container.encode("app_focus", forKey: .type)
            try container.encode(bundleId, forKey: .bundleId)
            try container.encode(appName, forKey: .appName)
        case .connectorMatch(let match):
            try container.encode("connector_match", forKey: .type)
            try container.encode(match.connectorId, forKey: .connectorId)
            try container.encode(match.signalKind, forKey: .signalKind)
            try container.encode(match.ruleId, forKey: .ruleId)
        case .gap(let reason):
            try container.encode("gap", forKey: .type)
            try container.encode(reason, forKey: .reason)
        case .stopped(let reason):
            try container.encode("stopped", forKey: .type)
            try container.encode(reason, forKey: .reason)
        }
    }

    public func encodedLine() throws -> Data {
        var bytes = try JSONEncoder().encode(self)
        bytes.append(0x0A)
        guard bytes.count <= WireLimits.lineBytes else { throw WireError.oversizedLine }
        return bytes
    }
}

/// Streaming framing retains at most one incomplete line. EOF never executes a partial command.
public struct ControlLineFramer: Sendable {
    private var buffer = Data()
    public init() {}

    public mutating func append(_ bytes: Data, receive: (ControlMessage) throws -> Void) throws {
        for byte in bytes {
            if byte == 0x0A {
                let control = try JSONDecoder().decode(ControlMessage.self, from: buffer)
                buffer.removeAll(keepingCapacity: true)
                try receive(control)
            } else {
                guard buffer.count < WireLimits.lineBytes - 1 else { throw WireError.oversizedLine }
                buffer.append(byte)
            }
        }
    }
}
