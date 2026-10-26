import Darwin
import Foundation
import Testing
@testable import ObserverCore

private enum HarnessError: Error { case timeout, prematureEOF, oversizedOutput, invalidJSON, failedExit }

/// Reads real NDJSON from the built helper, with deadlines so a lifecycle regression fails promptly.
private final class ChildSession {
    let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private let diagnostics = Pipe()
    private var pending = Data()
    private(set) var events: [[String: Any]] = []
    private var inputClosed = false

    init() throws {
        // An unexpected early child exit should fail the test through write(), not kill the runner.
        signal(SIGPIPE, SIG_IGN)
        let package = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        // SwiftPM builds the executable dependency alongside the debug tests.
        process.executableURL = package.appendingPathComponent(".build/debug/macroscope-observer")
        process.standardInput = input
        process.standardOutput = output
        process.standardError = diagnostics
        try process.run()
        // Close parent copies of the child's ends so stdout EOF is observable.
        try input.fileHandleForReading.close()
        try output.fileHandleForWriting.close()
        try diagnostics.fileHandleForWriting.close()
    }

    deinit {
        if !inputClosed { try? input.fileHandleForWriting.close() }
        if process.isRunning { process.terminate(); process.waitUntilExit() }
    }

    func start(maxSeconds: Int = 2) throws {
        try write("""
        {"type":"start","sessionId":"subprocess-synthetic","maxSeconds":\(maxSeconds),"rules":{"connectors":[]},"exclusions":{"bundleIds":[]},"policyEpoch":42}

        """)
    }

    func write(_ string: String) throws { try input.fileHandleForWriting.write(contentsOf: Data(string.utf8)) }

    func closeInput() throws {
        try input.fileHandleForWriting.close()
        inputClosed = true
    }

    @discardableResult
    func nextLine(timeout: Double = 5) throws -> [String: Any]? {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(timeout))
        while true {
            if let newline = pending.firstIndex(of: 0x0A) {
                let line = pending.prefix(through: newline)
                guard line.count <= WireLimits.lineBytes,
                      let event = try JSONSerialization.jsonObject(with: line) as? [String: Any] else {
                    throw HarnessError.invalidJSON
                }
                pending.removeSubrange(...newline)
                events.append(event)
                guard events.count <= 256 else { throw HarnessError.oversizedOutput }
                return event
            }
            guard clock.now < deadline else { throw HarnessError.timeout }
            var descriptor = pollfd(fd: output.fileHandleForReading.fileDescriptor, events: Int16(POLLIN), revents: 0)
            let result = poll(&descriptor, 1, 20)
            if result < 0 && errno == EINTR { continue }
            if result > 0 {
                var bytes = [UInt8](repeating: 0, count: 4_096)
                let count = Darwin.read(descriptor.fd, &bytes, bytes.count)
                if count == 0 {
                    guard pending.isEmpty else { throw HarnessError.invalidJSON }
                    return nil
                }
                guard count > 0 else { throw HarnessError.prematureEOF }
                pending.append(contentsOf: bytes.prefix(count))
                guard pending.count <= WireLimits.lineBytes + 4_096 else { throw HarnessError.oversizedOutput }
            }
        }
    }

    func collectToExit(timeout: Double = 6) throws {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(timeout))
        while try nextLine(timeout: max(0.01, seconds(clock.now.duration(to: deadline)))) != nil {
            guard clock.now < deadline else { throw HarnessError.timeout }
        }
        while process.isRunning {
            guard clock.now < deadline else { throw HarnessError.timeout }
            usleep(1_000)
        }
        process.waitUntilExit()
        #expect(process.terminationReason == .exit)
        #expect(process.terminationStatus == 0)
        let stderr = diagnostics.fileHandleForReading.readDataToEndOfFile()
        #expect(stderr.isEmpty, "Helper diagnostics must remain empty for synthetic lifecycle runs")
    }

    func assertEnvelopeAndStop(_ reason: String) throws {
        #expect(events.first?["type"] as? String == "ready")
        let permissions = try #require(events.first?["permissions"] as? [String: String])
        #expect(["granted", "denied"].contains(permissions["accessibility"] ?? ""))
        #expect(events.last?["type"] as? String == "stopped")
        #expect(events.last?["reason"] as? String == reason)
        #expect(events.filter { $0["type"] as? String == "stopped" }.count == 1)
        var previousSeq = 0
        var previousMono = 0
        for event in events {
            #expect(event["v"] as? Int == 1)
            #expect(event["sessionId"] as? String == "subprocess-synthetic")
            #expect(event["policyEpoch"] as? Int == 42)
            let seq = try #require(event["seq"] as? Int)
            let mono = try #require(event["monoMs"] as? Int)
            #expect(seq > previousSeq)
            #expect(mono >= previousMono)
            previousSeq = seq
            previousMono = mono
        }
    }
}

private func seconds(_ duration: Duration) -> Double {
    Double(duration.components.seconds) + Double(duration.components.attoseconds) / 1e18
}

@Suite(.serialized) struct SubprocessTests {
    @Test func EOFRevokesCaptureAndExitsQuickly() throws {
        let child = try ChildSession()
        try child.start()
        #expect(try child.nextLine()?["type"] as? String == "ready")
        let started = ContinuousClock.now
        try child.closeInput()
        try child.collectToExit(timeout: 2)
        #expect(seconds(started.duration(to: .now)) < 1.5)
        try child.assertEnvelopeAndStop("eof")
    }

    @Test func maxDurationExitsWhileStdinRemainsOpen() throws {
        let child = try ChildSession()
        let started = ContinuousClock.now
        try child.start(maxSeconds: 2)
        try child.collectToExit(timeout: 5)
        let elapsed = seconds(started.duration(to: .now))
        #expect(elapsed >= 1.8 && elapsed < 4.5)
        try child.assertEnvelopeAndStop("max_duration")
    }

    @Test func pauseResumeAndUserStop() throws {
        let child = try ChildSession()
        try child.start(maxSeconds: 5)
        #expect(try child.nextLine()?["type"] as? String == "ready")
        try child.write("{\"type\":\"pause\"}\n")
        var paused = false
        for _ in 0..<16 {
            let event = try #require(try child.nextLine())
            if event["type"] as? String == "gap", event["reason"] as? String == "paused" {
                paused = true
                break
            }
        }
        #expect(paused)
        try child.write("{\"type\":\"resume\"}\n{\"type\":\"stop\"}\n")
        try child.collectToExit()
        try child.assertEnvelopeAndStop("user")
    }

    @Test func EOFDiscardsPartialControlLine() throws {
        let child = try ChildSession()
        try child.start()
        #expect(try child.nextLine()?["type"] as? String == "ready")
        try child.write("{\"type\":\"stop\"}")
        try child.closeInput()
        try child.collectToExit()
        try child.assertEnvelopeAndStop("eof")
    }

    @Test(arguments: [false, true])
    func malformedAndOversizedInputStopWithError(oversized: Bool) throws {
        let child = try ChildSession()
        try child.start()
        #expect(try child.nextLine()?["type"] as? String == "ready")
        try child.write(oversized ? String(repeating: "x", count: WireLimits.lineBytes) : "{\"type\":\"unknown\"}\n")
        try child.collectToExit()
        try child.assertEnvelopeAndStop("error")
    }
}
