import Foundation

public enum CaptureWire {
    public static let version = 1
    public static let maxHeader = 65_536
    public static let maxPayload = 8 * 1024 * 1024
    public enum Failure: Error { case invalid, oversized }

    // Both directions: u32 BE JSON length, JSON with payloadBytes, then exactly that many bytes.
    public static func encode(_ header: CaptureHeader, pixels: Data = Data()) throws -> Data {
        guard header.v == version, header.payloadBytes == pixels.count else { throw Failure.invalid }
        let json = try JSONEncoder().encode(header)
        guard !json.isEmpty, json.count <= maxHeader,
              json.count + pixels.count <= maxPayload else { throw Failure.oversized }
        var length = UInt32(json.count).bigEndian
        var packet = withUnsafeBytes(of: &length) { Data($0) }
        packet.append(json)
        packet.append(pixels)
        return packet
    }

    public static func decode(_ packet: Data) throws -> (CaptureHeader, Data) {
        guard packet.count >= 4 else { throw Failure.invalid }
        let n = packet.prefix(4).reduce(0) { ($0 << 8) | Int($1) }
        guard n > 0, n <= maxHeader, packet.count >= 4 + n else { throw Failure.oversized }
        let header = try JSONDecoder().decode(CaptureHeader.self, from: packet.subdata(in: 4..<(4+n)))
        guard header.v == version, header.payloadBytes >= 0,
              header.payloadBytes <= maxPayload - n else { throw Failure.oversized }
        guard packet.count == 4 + n + header.payloadBytes else { throw Failure.invalid }
        return (header, packet.subdata(in: (4+n)..<packet.count))
    }
}

// Never log these values: frame metadata contains raw on-device OCR and AX labels.
public struct PixelRect: Codable, Equatable, Sendable {
    public var x: Int, y: Int, width: Int, height: Int
    public init(x: Int, y: Int, width: Int, height: Int) {
        self.x = x; self.y = y; self.width = width; self.height = height
    }
}
public struct CaptureOCR: Codable, Sendable {
    public var text: String
    public var rect: PixelRect
    public init(text: String, rect: PixelRect) { self.text = text; self.rect = rect }
}
public struct CaptureMask: Codable, Equatable, Sendable {
    public var rect: PixelRect
    public var reason: String
    public init(rect: PixelRect) { self.rect = rect; self.reason = "secure_field" }
}
public struct CaptureHeader: Codable, Sendable {
    public var v = 1
    public var type: String
    public var sessionId = ""
    public var policyEpoch: UInt64 = 0
    public var payloadBytes = 0
    public var captureSupported: Bool?
    public var screenRecordingGranted: Bool?
    public var accessibilityGranted: Bool?
    public var reason: String?
    public var appName: String?
    public var seq: UInt64?
    public var monoMs: UInt64?
    public var width: Int?
    public var height: Int?
    public var axLabels: [String]?
    public var ocrBoxes: [CaptureOCR]?
    public var masks: [CaptureMask]?
    public var uncertain: Bool?
    public var secureInputGap: Bool?
    public var normalWindowConfirmed: Bool?
    public init(type: String) { self.type = type }
}
