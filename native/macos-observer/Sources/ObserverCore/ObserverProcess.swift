import AppKit
@preconcurrency import ApplicationServices
import Darwin

@MainActor
public enum ObserverProcess {
    /// This function returns only after a session stops (or input fails before start).
    public static func run(requestAccessibility: Bool = false) {
        signal(SIGPIPE, SIG_IGN)
        let runtime = Runtime(requestAccessibility: requestAccessibility)
        runtime.run()
    }
}

@MainActor
private final class Runtime {
    private let session = ObserverSession()
    private let source = MacOSObservationSource()
    private let inbox = ControlInbox()
    private let clock = ContinuousClock()
    private let origin = ContinuousClock.now
    private let inputQueue = DispatchQueue(label: "macroscope.observer.stdin")
    private var input: DispatchSourceRead?
    private var diagnosticDrain: DispatchSourceRead?
    private var output: DispatchSourceWrite?
    private var outputResumed = false
    private var outputBuffer = Data()
    private var pollTimer: DispatchSourceTimer?
    private var deadlineTimer: DispatchSourceTimer?
    private var tokens: [NSObjectProtocol] = []
    private var finishing = false
    private var completed = false
    private let requestAccessibility: Bool

    init(requestAccessibility: Bool) { self.requestAccessibility = requestAccessibility }

    private var now: Double {
        let duration = origin.duration(to: clock.now).components
        return Double(duration.seconds) + Double(duration.attoseconds) / 1e18
    }

    func run() {
        // Nonblocking stdout prevents a stalled parent from delaying EOF or the hard deadline.
        let flags = fcntl(STDOUT_FILENO, F_GETFL)
        guard flags >= 0, fcntl(STDOUT_FILENO, F_SETFL, flags | O_NONBLOCK) == 0 else { return }
        guard containFrameworkDiagnostics() else { return }
        output = DispatchSource.makeWriteSource(fileDescriptor: STDOUT_FILENO, queue: .main)
        output?.setEventHandler { [weak self] in self?.flushOutput() }
        startInput()
        CFRunLoopRun()
        if let output, !outputResumed { output.resume() }
        output?.cancel()
        input?.cancel()
        diagnosticDrain?.cancel()
        NSWorkspace.shared.notificationCenter.removeObservers(tokens)
    }

    private func containFrameworkDiagnostics() -> Bool {
        // NSWorkspace can NSLog unsanitized framework diagnostics when services are unavailable.
        // Route stderr into a bounded anonymous pipe and discard it in-process. No file is opened
        // or written, and framework messages cannot become protocol output or external diagnostics.
        var descriptors: [Int32] = [-1, -1]
        guard pipe(&descriptors) == 0 else { return false }
        let readFD = descriptors[0]
        let writeFD = descriptors[1]
        guard dup2(writeFD, STDERR_FILENO) >= 0 else {
            close(readFD)
            close(writeFD)
            return false
        }
        close(writeFD)
        let drain = DispatchSource.makeReadSource(fileDescriptor: readFD, queue: inputQueue)
        let handler: @Sendable () -> Void = {
            var bytes = [UInt8](repeating: 0, count: 4_096)
            _ = Darwin.read(readFD, &bytes, bytes.count)
        }
        drain.setEventHandler(handler: handler)
        let cancel: @Sendable () -> Void = { close(readFD) }
        drain.setCancelHandler(handler: cancel)
        diagnosticDrain = drain
        drain.resume()
        return true
    }

    private func startInput() {
        let inbox = self.inbox
        // Reader owns framing state, and only one drain task can be outstanding.
        let reader = StdinReader(inbox: inbox) { [weak self] in
            DispatchQueue.main.async { self?.drainInput() }
        }
        let input = DispatchSource.makeReadSource(fileDescriptor: STDIN_FILENO, queue: inputQueue)
        self.input = input
        // DispatchSource's legacy handler type can inherit MainActor dynamically; explicitly
        // make this closure Sendable because it runs on the dedicated stdin queue.
        let handler: @Sendable () -> Void = { reader.readAvailable() }
        input.setEventHandler(handler: handler)
        input.resume()
    }

    private func drainInput() {
        guard !finishing else { return }
        let batch = inbox.take()
        do {
            for message in batch.controls {
                let wasIdle = session.phase == .idle
                if wasIdle, case .start = message, requestAccessibility {
                    // This is the only prompting API, and only an explicit flag enables it.
                    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
                    _ = AXIsProcessTrustedWithOptions(options)
                }
                send(try session.control(message, now: now, accessibility: source.accessibilityPermission()))
                if session.phase == .stopped { finish(); return }
                if wasIdle && session.phase != .idle { beginMonitoring() }
            }
        } catch {
            fail()
            return
        }
        if let end = batch.end {
            send(session.stop(end == .eof ? .eof : .error, now: now))
            finish()
            return
        }
        inbox.enableCapture(session.phase == .active)
        observe()
    }

    private func beginMonitoring() {
        let center = NSWorkspace.shared.notificationCenter
        func listen(_ name: Notification.Name, _ action: @escaping @MainActor () -> Void) {
            tokens.append(center.addObserver(forName: name, object: nil, queue: .main) { _ in
                MainActor.assumeIsolated { action() }
            })
        }
        listen(NSWorkspace.didActivateApplicationNotification) { [weak self] in self?.observe() }
        listen(NSWorkspace.sessionDidResignActiveNotification) { [weak self] in
            guard let self else { return }
            self.send(self.session.setSessionActive(false, now: self.now))
        }
        listen(NSWorkspace.sessionDidBecomeActiveNotification) { [weak self] in
            guard let self else { return }
            self.send(self.session.setSessionActive(true, now: self.now))
            self.observe()
        }
        listen(NSWorkspace.screensDidSleepNotification) { [weak self] in
            guard let self else { return }
            self.send(self.session.setScreensAwake(false, now: self.now))
        }
        listen(NSWorkspace.screensDidWakeNotification) { [weak self] in
            guard let self else { return }
            self.send(self.session.setScreensAwake(true, now: self.now))
            self.observe()
        }
        listen(NSWorkspace.willSleepNotification) { [weak self] in
            guard let self else { return }
            self.send(self.session.setScreensAwake(false, now: self.now))
        }
        listen(NSWorkspace.didWakeNotification) { [weak self] in
            guard let self else { return }
            self.send(self.session.setScreensAwake(true, now: self.now))
            self.observe()
        }

        let poll = DispatchSource.makeTimerSource(queue: .main)
        poll.schedule(deadline: .now() + 2, repeating: 2)
        poll.setEventHandler { [weak self] in self?.observe() }
        pollTimer = poll
        poll.resume()

        let deadline = DispatchSource.makeTimerSource(queue: .main)
        deadline.schedule(deadline: .now() + max(0, (session.deadline ?? now) - now), leeway: .milliseconds(1))
        deadline.setEventHandler { [weak self] in self?.checkDeadline() }
        deadlineTimer = deadline
        deadline.resume()
    }

    private func checkDeadline() {
        guard !finishing else { return }
        if let events = session.tick(now: now) {
            send(events)
            finish()
        } else if let deadline = session.deadline {
            // Dispatch and ContinuousClock can differ slightly; an early wake must not lose the stop.
            deadlineTimer?.schedule(deadline: .now() + max(0.001, deadline - now))
        }
    }

    private func observe() {
        guard !finishing else { return }
        if let expired = session.tick(now: now) {
            send(expired)
            finish()
            return
        }
        guard inbox.captureAllowed() else { return }
        let events = session.sample(source: source, now: { self.now }, authorized: { self.inbox.captureAllowed() })
        send(events)
        if session.phase == .stopped { finish() }
    }

    private func send(_ events: [ObserverEvent]) {
        guard !finishing else { return }
        do {
            for event in events {
                let line = try event.encodedLine()
                // Reserve one maximum-size event for the terminal error; retain partial writes intact.
                guard outputBuffer.count + line.count <= WireLimits.outputBytes - WireLimits.lineBytes else {
                    throw WireError.output
                }
                outputBuffer.append(line)
            }
            flushOutput()
        } catch {
            // Preserve line framing under backpressure, including any partially written first line.
            if let stopped = session.stop(.error, now: now).first,
               let line = try? stopped.encodedLine() { outputBuffer.append(line) }
            finish()
        }
    }

    private func flushOutput() {
        while !outputBuffer.isEmpty {
            let count = outputBuffer.withUnsafeBytes { bytes in
                Darwin.write(STDOUT_FILENO, bytes.baseAddress!, bytes.count)
            }
            if count > 0 { outputBuffer.removeFirst(count); continue }
            if count < 0 && errno == EINTR { continue }
            if count < 0 && (errno == EAGAIN || errno == EWOULDBLOCK) {
                if !outputResumed { outputResumed = true; output?.resume() }
                return
            }
            outputBuffer.removeAll(keepingCapacity: false)
            if !finishing { fail() }
            complete()
            return
        }
        if outputResumed { output?.suspend(); outputResumed = false }
        if finishing { complete() }
    }

    private func fail() {
        send(session.stop(.error, now: now))
        finish()
    }

    private func finish() {
        guard !finishing else { return }
        finishing = true
        inbox.enableCapture(false)
        input?.cancel()
        pollTimer?.cancel()
        deadlineTimer?.cancel()
        NSWorkspace.shared.notificationCenter.removeObservers(tokens)
        tokens.removeAll()
        flushOutput()
        // A parent that stops reading stdout must never keep the helper alive.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in self?.complete() }
    }

    private func complete() {
        guard !completed else { return }
        completed = true
        CFRunLoopStop(CFRunLoopGetMain())
    }
}

private extension NotificationCenter {
    func removeObservers(_ observers: [NSObjectProtocol]) {
        for observer in observers { removeObserver(observer) }
    }
}

/// Confined to the stdin serial queue; the lock-protected inbox handles cross-thread authority.
private final class StdinReader: @unchecked Sendable {
    private let inbox: ControlInbox
    private let wake: @Sendable () -> Void
    private var framer = ControlLineFramer()
    private var ended = false

    init(inbox: ControlInbox, wake: @escaping @Sendable () -> Void) {
        self.inbox = inbox
        self.wake = wake
    }

    func readAvailable() {
        guard !ended else { return }
        var bytes = [UInt8](repeating: 0, count: 4_096)
        let count = Darwin.read(STDIN_FILENO, &bytes, bytes.count)
        if count > 0 {
            do {
                try framer.append(Data(bytes.prefix(count))) { message in
                    if try inbox.enqueue(message) { wake() }
                }
            } catch {
                ended = true
                if inbox.finish(.error) { wake() }
            }
        } else if count == 0 {
            ended = true
            if inbox.finish(.eof) { wake() }
        } else if errno != EINTR && errno != EAGAIN {
            ended = true
            if inbox.finish(.error) { wake() }
        }
    }
}
