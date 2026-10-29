import Testing
import Foundation
import CoreGraphics
@testable import CaptureCore

@Test func visionCoordinatesClampAndRoundOutward() {
    #expect(CaptureGeometry.vision(CGRect(x: 0.125, y: 0.25, width: 0.5, height: 0.5), width: 128, height: 128) == PixelRect(x: 16, y: 32, width: 64, height: 64))
    #expect(CaptureGeometry.vision(CGRect(x: -0.25, y: -0.25, width: 0.5, height: 0.5), width: 100, height: 100) == PixelRect(x: 0, y: 75, width: 25, height: 25))
    #expect(CaptureGeometry.vision(CGRect(x: 2, y: 2, width: 1, height: 1), width: 100, height: 100) == nil)
    #expect(CaptureGeometry.vision(CGRect(x: 0, y: 0, width: 0, height: 1), width: 100, height: 100) == nil)
    #expect(CaptureGeometry.topLeft(CGRect(x: CGFloat.nan, y: 0, width: 1, height: 1), width: 100, height: 100) == nil)
}
@Test func secureFieldMapping() {
    let mask = CaptureGeometry.secureField(CGRect(x: 150, y: 125, width: 50, height: 10),
        window: CGRect(x: 100, y: 100, width: 400, height: 200), width: 800, height: 400)
    #expect(mask == CaptureMask(rect: PixelRect(x: 100, y: 50, width: 100, height: 20)))
    #expect(mask?.reason == "secure_field")
    #expect(CaptureGeometry.secureField(CGRect(x: 0, y: 0, width: 1, height: 1), window: .zero, width: 100, height: 100) == nil)
}
@Test func perceptualDiff() {
    var diff = FrameDifference()
    let black = Data(repeating: 0, count: 64*64*4)
    let first = diff.changed(black, width: 64, height: 64)
    #expect(first)
    let same = diff.changed(black, width: 64, height: 64)
    #expect(!same)
    let white = Data(repeating: 255, count: black.count)
    let changed = diff.changed(white, width: 64, height: 64)
    #expect(changed)
    let invalid = diff.changed(Data([1]), width: 64, height: 64)
    #expect(!invalid)
}
@Test func protocolRoundTripAndCaps() throws {
    var h = CaptureHeader(type: "frame"); h.payloadBytes = 4; h.sessionId = "synthetic"; h.policyEpoch = 8
    h.width = 1; h.height = 1
    let data = try CaptureWire.encode(h, pixels: Data([1,2,3,255]))
    let (decoded, pixels) = try CaptureWire.decode(data)
    #expect(decoded.sessionId == "synthetic" && decoded.policyEpoch == 8)
    #expect(pixels == Data([1,2,3,255]))
    for n in 0..<data.count { #expect(throws: (any Error).self) { try CaptureWire.decode(data.prefix(n)) } }
    h.payloadBytes = CaptureWire.maxPayload
    #expect(throws: (any Error).self) { try CaptureWire.encode(h, pixels: Data(repeating: 0, count: h.payloadBytes)) }
    #expect(throws: (any Error).self) { try CaptureWire.decode(Data([0,1,0,1])) }
    h.payloadBytes = 0; h.v = 2
    #expect(throws: (any Error).self) { try CaptureWire.encode(h) }
}
@Test func exclusionsAndBrowserRefusal() {
    for id in CaptureAdmission.denied { #expect(CaptureAdmission.refusal(bundleId: id, isBrowser: false, privacy: .unknown) == "excluded") }
    for id in CaptureAdmission.browsers {
        #expect(CaptureAdmission.refusal(bundleId: id, isBrowser: false, privacy: .unknown) == "private_or_unknown_window")
        #expect(CaptureAdmission.refusal(bundleId: id, isBrowser: false, privacy: .private) == "private_or_unknown_window")
    }
    #expect(CaptureAdmission.refusal(bundleId: "new.browser", isBrowser: true, privacy: .unknown) == "private_or_unknown_window")
    #expect(CaptureAdmission.refusal(bundleId: nil, isBrowser: false, privacy: .unknown) != nil)
    #expect(CaptureAdmission.refusal(bundleId: "com.apple.TextEdit", isBrowser: false, privacy: .unknown) == nil)
}

@Test func movedResizedOrUnknownWindowGeometryClosesAdmission() {
    let frame = CGRect(x: -100, y: 40, width: 800, height: 600)
    #expect(CaptureGeometry.sameWindowBounds(frame, frame))
    #expect(!CaptureGeometry.sameWindowBounds(frame, frame.offsetBy(dx: 1, dy: 0)))
    #expect(!CaptureGeometry.sameWindowBounds(frame, CGRect(x: -100, y: 40, width: 799, height: 600)))
    #expect(!CaptureGeometry.sameWindowBounds(frame, .zero))
}


@Test func rgbaRowsStayAlignedWithVisionTopLeftBoxes() throws {
    // Fabricated red top row and blue bottom row. No image files or capture APIs.
    let source = Data([255,0,0,255, 255,0,0,255, 0,0,255,255, 0,0,255,255])
    let provider = try #require(CGDataProvider(data: source as CFData))
    let image = try #require(CGImage(width: 2, height: 2, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: 8,
        space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
        provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent))
    let pixels = try CapturePixels.rgba(image)
    #expect(pixels == source)
    let topRow = CaptureGeometry.vision(CGRect(x: 0, y: 0.5, width: 1, height: 0.5), width: 2, height: 2)
    #expect(topRow == PixelRect(x: 0, y: 0, width: 2, height: 1))
    #expect(pixels[0] == 255 && pixels[2] == 0)
}
