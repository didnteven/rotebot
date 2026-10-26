import Foundation
import Testing
@testable import ObserverCore

@Suite struct FramingTests {
    @Test func fragmentedAndMultipleControlLines() throws {
        var framer = ControlLineFramer()
        var received: [ControlMessage] = []
        try framer.append(Data("{\"ty".utf8)) { received.append($0) }
        #expect(received.isEmpty)
        try framer.append(Data("pe\":\"pause\"}\n{\"type\":\"resume\"}\r\n".utf8)) { received.append($0) }
        #expect(received == [.pause, .resume])
    }

    @Test func exact64KiBLineIsAcceptedAndOneExtraByteIsRejected() throws {
        let prefix = "{\"type\":\"pause\"}"
        let valid = prefix + String(repeating: " ", count: WireLimits.lineBytes - 1 - prefix.utf8.count) + "\n"
        #expect(valid.utf8.count == WireLimits.lineBytes)
        var framer = ControlLineFramer()
        var received: [ControlMessage] = []
        try framer.append(Data(valid.utf8)) { received.append($0) }
        #expect(received == [.pause])
        var other = ControlLineFramer()
        #expect(throws: WireError.self) {
            try other.append(Data((" " + valid).utf8)) { _ in }
        }
    }

    @Test(arguments: ["{\"type\":\"unknown\"}\n", "[]\n", "broken\n", "\n"])
    func invalidJSONAndUnknownControlsFailClosed(line: String) {
        var framer = ControlLineFramer()
        #expect(throws: (any Error).self) { try framer.append(Data(line.utf8)) { _ in } }
    }

    @Test func partialLineIsNeverDelivered() throws {
        var framer = ControlLineFramer()
        var count = 0
        try framer.append(Data("{\"type\":\"stop\"}".utf8)) { _ in count += 1 }
        #expect(count == 0)
    }

    @Test func inboxRevokesImmediatelyOnPendingControlAndPermanentlyOnEOF() throws {
        let inbox = ControlInbox()
        inbox.enableCapture(true)
        #expect(inbox.captureAllowed())
        #expect(try inbox.enqueue(.pause))
        #expect(!inbox.captureAllowed())
        inbox.enableCapture(true)
        #expect(!inbox.captureAllowed())
        #expect(!inbox.finish(.eof))
        let batch = inbox.take()
        #expect(batch.controls == [.pause])
        #expect(batch.end == .eof)
        inbox.enableCapture(true)
        #expect(!inbox.captureAllowed())
    }

    @Test func inboxBacklogIsBoundedAndWakeupsCoalesce() throws {
        let inbox = ControlInbox()
        #expect(try inbox.enqueue(.pause))
        for _ in 1..<WireLimits.pendingControls { #expect(try !inbox.enqueue(.resume)) }
        #expect(throws: WireError.self) { try inbox.enqueue(.resume) }
        #expect(inbox.take().controls.count == WireLimits.pendingControls)
    }
}
