import AppKit
@preconcurrency import ApplicationServices
@preconcurrency import ScreenCaptureKit
@preconcurrency import Vision
import CoreImage
import Carbon.HIToolbox
import Darwin

// The reader revokes authority immediately, even during synchronous OCR/AX calls on main.
private final class Authority: @unchecked Sendable {
    private let lock = NSLock()
    private var revoked = false
    private var pending = 0
    func enqueue() -> Bool { lock.lock(); defer { lock.unlock() }; pending += 1; return pending <= 8 }
    func drained() { lock.lock(); pending -= 1; lock.unlock() }
    func revoke() { lock.lock(); revoked = true; lock.unlock() }
    func allowed() -> Bool { lock.lock(); defer { lock.unlock() }; return !revoked }
}

@MainActor
public enum CaptureProcess {
    public static func run() {
        signal(SIGPIPE, SIG_IGN)
        // Suppress framework diagnostics before interacting with any user content.
        var diagnosticPipe: [Int32] = [-1, -1]
        guard pipe(&diagnosticPipe) == 0, dup2(diagnosticPipe[1], STDERR_FILENO) >= 0 else { return }
        close(diagnosticPipe[1])
        let fd = diagnosticPipe[0]
        DispatchQueue.global().async {
            var bytes = [UInt8](repeating: 0, count: 4096)
            while Darwin.read(fd, &bytes, bytes.count) > 0 {}
            close(fd)
        }
        guard CaptureContainment.install() else { return }
        let runtime = CaptureRuntime()
        runtime.run()
    }
}

@MainActor
private final class CaptureRuntime: NSObject, SCContentSharingPickerObserver, @preconcurrency SCStreamOutput, SCStreamDelegate {
    private let authority = Authority()
    private var session = ""
    private var normalWindowConfirmed = false
    private var epoch: UInt64 = 0
    private var began = ProcessInfo.processInfo.systemUptime
    private var stream: SCStream?
    private var window: SCWindow?
    private var timer: Timer?
    private var ended = false
    private var seq: UInt64 = 0
    private var lastFrame = -Double.infinity
    private var diff = FrameDifference()
    private var appName = ""
    private var notifications: [NSObjectProtocol] = []
    private let imageContext = CIContext(options: [.cacheIntermediates: false])

    func run() {
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.accessory)
        let flags = fcntl(STDOUT_FILENO, F_GETFL)
        guard flags >= 0, fcntl(STDOUT_FILENO, F_SETFL, flags | O_NONBLOCK) == 0 else { return }
        ready()
        let authority = authority
        let wake: @Sendable (CaptureHeader?) -> Void = { [weak self] header in
            DispatchQueue.main.async { authority.drained(); self?.control(header) }
        }
        DispatchQueue.global().async {
            func exact(_ count: Int) -> Data? {
                var data = Data(count: count)
                var offset = 0
                while offset < count {
                    let n = data.withUnsafeMutableBytes { Darwin.read(STDIN_FILENO, $0.baseAddress!.advanced(by: offset), count-offset) }
                    if n < 0 && errno == EINTR { continue }
                    guard n > 0 else { return nil }
                    offset += n
                }
                return data
            }
            var started = false
            while let prefix = exact(4) {
                let n = prefix.reduce(0) { ($0 << 8) | Int($1) }
                guard n > 0, n <= CaptureWire.maxHeader, let json = exact(n),
                      let control = try? JSONDecoder().decode(CaptureHeader.self, from: json),
                      control.v == 1, control.payloadBytes == 0,
                      ["start", "request_permission", "stop", "pause"].contains(control.type),
                      authority.enqueue() else { break }
                if control.type == "start" || control.type == "request_permission" {
                    guard !started else { break }
                    started = true
                }
                if control.type == "stop" || control.type == "pause" { authority.revoke() }
                wake(control)
                if control.type == "stop" || control.type == "pause" {
                    DispatchQueue.global().asyncAfter(deadline: .now()+0.5) { Darwin.exit(0) }
                    return
                }
            }
            authority.revoke()
            DispatchQueue.global().asyncAfter(deadline: .now()+0.5) { Darwin.exit(0) }
            _ = authority.enqueue()
            wake(nil)
        }
        // Idle probe cannot remain alive indefinitely; start replaces the origin.
        timer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        CFRunLoopRun()
        timer?.invalidate()
    }
    private func ready() {
        var event = CaptureHeader(type: "ready")
        if #available(macOS 15.2, *) { event.captureSupported = true } else { event.captureSupported = false }
        event.screenRecordingGranted = CGPreflightScreenCaptureAccess()
        event.accessibilityGranted = AXIsProcessTrusted()
        send(event)
    }
    private func control(_ header: CaptureHeader?) {
        guard !ended else { return }
        guard let header else { finish("eof"); return }
        switch header.type {
        case "request_permission":
            guard session.isEmpty else { finish("error"); return }
            _ = CGRequestScreenCaptureAccess()
            let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
            ready(); finish("permission_requested")
        case "start":
            guard session.isEmpty, !header.sessionId.isEmpty, header.sessionId.utf8.count <= 128 else { finish("error"); return }
            normalWindowConfirmed = header.normalWindowConfirmed == true
            session = header.sessionId; epoch = header.policyEpoch; began = ProcessInfo.processInfo.systemUptime
            let authority = self.authority
            DispatchQueue.global().asyncAfter(deadline: .now()+600) { [weak self] in
                authority.revoke()
                DispatchQueue.main.async { self?.finish("max_duration") }
                DispatchQueue.global().asyncAfter(deadline: .now()+0.5) { Darwin.exit(0) }
            }
            guard CGPreflightScreenCaptureAccess() else { finish("screen_recording_denied"); return }
            guard #available(macOS 15.2, *) else { finish("picker_identity_unavailable"); return }
            var config = SCContentSharingPickerConfiguration()
            config.allowedPickerModes = [.singleWindow]
            config.allowsChangingSelectedContent = false
            config.excludedBundleIDs = Array(CaptureAdmission.denied)
            let picker = SCContentSharingPicker.shared
            picker.defaultConfiguration = config
            picker.maximumStreamCount = 1
            picker.add(self)
            picker.isActive = true
            picker.present(using: .window)
            for name in [NSWorkspace.sessionDidResignActiveNotification, NSWorkspace.screensDidSleepNotification,
                         NSWorkspace.willSleepNotification] {
                notifications.append(NSWorkspace.shared.notificationCenter.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                    MainActor.assumeIsolated { self?.finish("session_inactive") }
                })
            }
        case "stop": finish("user")
        case "pause": finish("paused") // Pause permanently closes this session; never broadens authority.
        default: finish("error")
        }
    }
    private func tick() {
        guard !ended else { return }
        let age = ProcessInfo.processInfo.systemUptime-began
        if age >= (session.isEmpty ? 10 : 600) { finish("max_duration") }
        else if !session.isEmpty && !CGPreflightScreenCaptureAccess() { finish("screen_recording_denied") }
    }
    private func allowed() -> Bool {
        !ended && authority.allowed() && !session.isEmpty && ProcessInfo.processInfo.systemUptime-began < 600
    }
    nonisolated func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
        Task { @MainActor in finish("picker_cancelled") }
    }
    nonisolated func contentSharingPickerStartDidFailWithError(_ error: any Error) {
        Task { @MainActor in finish("picker_failed") }
    }
    nonisolated func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter, for stream: SCStream?) {
        Task { @MainActor in await picked(filter) }
    }
    private func picked(_ filter: SCContentFilter) async {
        guard allowed(), self.stream == nil, window == nil else { finish("selection_changed"); return }
        guard #available(macOS 15.2, *), filter.style == .window, filter.includedWindows.count == 1,
              let selected = filter.includedWindows.first, let app = selected.owningApplication else { finish("invalid_selection"); return }
        // Treat any application declaring itself a web browser as a browser too. No title is read.
        let running = NSRunningApplication(processIdentifier: app.processID)
        guard let info = running?.bundleURL.flatMap({ Bundle(url: $0)?.infoDictionary }) else { finish("unknown_application"); return }
        let types = info["CFBundleURLTypes"] as? [[String: Any]] ?? []
        let webHandler = types.contains { (($0["CFBundleURLSchemes"] as? [String]) ?? []).contains { ["http", "https"].contains($0) } }
        if let reason = CaptureAdmission.refusal(bundleId: app.bundleIdentifier, isBrowser: webHandler || (info["LSApplicationCategoryType"] as? String) == "public.app-category.web-browser", privacy: .publicVerified) { // the person chose this window in the system picker; the review step is the backstop
            finish(reason); return
        }
        window = selected
        appName = String(app.applicationName.prefix(256))
        var event = CaptureHeader(type: "picked"); event.appName = appName; send(event)
        let rect = filter.contentRect
        guard rect.width > 0, rect.height > 0, rect.width.isFinite, rect.height.isFinite else { finish("invalid_geometry"); return }
        var scale = min(1, 1600/max(rect.width, rect.height))
        scale = min(scale, sqrt(Double(CaptureWire.maxPayload-65_536)/(rect.width*rect.height*4)))
        let config = SCStreamConfiguration()
        config.width = max(1, Int(floor(rect.width*scale)))
        config.height = max(1, Int(floor(rect.height*scale)))
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        config.queueDepth = 3
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.showsCursor = false
        config.showMouseClicks = false
        config.includeChildWindows = false
        config.capturesAudio = false
        config.captureMicrophone = false
        config.ignoreShadowsSingleWindow = true
        config.scalesToFit = true
        // Map the whole window into the whole buffer, including after a resize;
        // no letterbox offset can silently invalidate secure-field coordinates.
        config.preservesAspectRatio = false
        let capture = SCStream(filter: filter, configuration: config, delegate: self)
        self.stream = capture
        do {
            try capture.addStreamOutput(self, type: .screen, sampleHandlerQueue: .main)
            try await capture.startCapture()
            if !allowed() { try? await capture.stopCapture() }
        } catch { finish("capture_failed") }
    }
    nonisolated func stream(_ stream: SCStream, didStopWithError error: any Error) {
        Task { @MainActor in finish("stream_stopped") }
    }
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        // This conformance is main-isolated: output is registered exclusively on .main.
        receive(sampleBuffer)
    }
    private func receive(_ sample: CMSampleBuffer) {
        let now = ProcessInfo.processInfo.systemUptime
        let secureAtReceipt = IsSecureEventInputEnabled()
        let sampleAge = CMTimeGetSeconds(CMClockGetTime(CMClockGetHostTimeClock()) - CMSampleBufferGetPresentationTimeStamp(sample))
        guard allowed(), now-lastFrame >= 1, let window,
              let app = window.owningApplication, let buffer = sample.imageBuffer,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let status = attachments.first?[.status] as? Int,
              status == SCFrameStatus.complete.rawValue else { return }
        lastFrame = now
        guard CGPreflightScreenCaptureAccess() else { finish("screen_recording_denied"); return }
        let width = CVPixelBufferGetWidth(buffer), height = CVPixelBufferGetHeight(buffer)
        guard width > 0, height > 0, width <= 1600, height <= 1600,
              width*height*4 <= CaptureWire.maxPayload-CaptureWire.maxHeader else { finish("oversized"); return }
        guard let image = imageContext.createCGImage(CIImage(cvPixelBuffer: buffer), from: CGRect(x: 0, y: 0, width: width, height: height)) else { finish("image_failed"); return }
        guard let pixels = try? CapturePixels.rgba(image), allowed() else { return }
        // Content change is the only trigger. No taps, event monitors, keys, clicks or clipboard.
        guard diff.changed(pixels, width: width, height: height) else { return }
        var header = CaptureHeader(type: "frame")
        seq += 1; header.seq = seq; header.monoMs = UInt64((now-began)*1000)
        header.width = width; header.height = height; header.appName = appName
        header.payloadBytes = pixels.count
        let secureBefore = secureAtReceipt || IsSecureEventInputEnabled()
        func frameRect(_ raw: Any?) -> CGRect? {
            if let rect = raw as? CGRect { return rect }
            guard let dictionary = raw as? [String: Any] else { return nil }
            return CGRect(dictionaryRepresentation: dictionary as CFDictionary)
        }
        let screenRect = frameRect(attachments.first?[.screenRect])
        let evidence = CaptureAccessibility.read(pid: app.processID, selectedWindow: window.windowID,
                                                  screenRect: screenRect, width: width, height: height, allowed: { self.allowed() && !IsSecureEventInputEnabled() })
        header.axLabels = evidence.labels; header.masks = evidence.masks
        header.uncertain = evidence.uncertain || !AXIsProcessTrusted() || !sampleAge.isFinite || sampleAge < 0 || sampleAge > 1
        header.secureInputGap = secureBefore || IsSecureEventInputEnabled()
        if header.secureInputGap == true { var gap = CaptureHeader(type: "gap"); gap.reason = "secure_input_gap"; send(gap) }
        var boxes: [CaptureOCR] = []
        if header.secureInputGap != true {
            do {
                let request = VNRecognizeTextRequest()
                request.recognitionLevel = .accurate
                request.usesLanguageCorrection = false
                try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
                var textBytes = 0
                guard let results = request.results, results.count <= 1024 else { throw CaptureWire.Failure.oversized }
                for result in results {
                    guard let candidate = result.topCandidates(1).first, candidate.confidence >= 0.4,
                          let rect = CaptureGeometry.vision(result.boundingBox, width: width, height: height) else { throw CaptureWire.Failure.invalid }
                    textBytes += candidate.string.utf8.count
                    guard textBytes <= 24_000 else { throw CaptureWire.Failure.oversized }
                    boxes.append(CaptureOCR(text: candidate.string, rect: rect))
                }
            } catch { header.uncertain = true; boxes = [] }
        }
        header.ocrBoxes = boxes
        let after = CaptureAccessibility.read(pid: app.processID, selectedWindow: window.windowID,
                                              screenRect: screenRect, width: width, height: height, allowed: { self.allowed() && !IsSecureEventInputEnabled() })
        header.uncertain = header.uncertain == true || after.uncertain || after.masks != evidence.masks
            || ProcessInfo.processInfo.systemUptime-now > 6
        header.secureInputGap = header.secureInputGap == true || IsSecureEventInputEnabled()
        if header.uncertain == true {
            // Fixed codes only (no window content) so a rejected frame can be explained to the person.
            let why = evidence.why ?? (!AXIsProcessTrusted() ? "ax_untrusted" : (!sampleAge.isFinite || sampleAge < 0 || sampleAge > 1) ? "stale_frame"
                : (after.why ?? (after.masks != evidence.masks ? "window_changed"
                : (ProcessInfo.processInfo.systemUptime-now > 6 ? "too_slow" : "text_scan_failed"))))
            var note = CaptureHeader(type: "gap"); note.reason = "uncertain_" + why; send(note)
        }
        guard allowed(), CGPreflightScreenCaptureAccess() else { return }
        send(header, pixels: pixels)
        if header.uncertain == true || header.secureInputGap == true { diff = FrameDifference() }
        if seq >= 60 { finish("frame_limit") }
    }
    private func send(_ input: CaptureHeader, pixels: Data = Data()) {
        var header = input; header.sessionId = session; header.policyEpoch = epoch
        guard let packet = try? CaptureWire.encode(header, pixels: pixels) else { finish("protocol_error"); return }
        let deadline = ProcessInfo.processInfo.systemUptime+0.5
        var offset = 0
        while offset < packet.count {
            let n = packet.withUnsafeBytes { Darwin.write(STDOUT_FILENO, $0.baseAddress!.advanced(by: offset), packet.count-offset) }
            if n > 0 { offset += n; continue }
            if n < 0 && errno == EINTR { continue }
            if n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK) && ProcessInfo.processInfo.systemUptime < deadline {
                var descriptor = pollfd(fd: STDOUT_FILENO, events: Int16(POLLOUT), revents: 0)
                _ = poll(&descriptor, 1, 10); continue
            }
            // Do not append a terminal event to a partial frame. Exit without diagnostics.
            authority.revoke(); Darwin.exit(0)
        }
    }
    private func finish(_ reason: String) {
        guard !ended else { return }
        ended = true; authority.revoke()
        SCContentSharingPicker.shared.isActive = false
        SCContentSharingPicker.shared.remove(self)
        if let stream { Task { try? await stream.stopCapture() } }
        var stopped = CaptureHeader(type: "stopped"); stopped.reason = reason; send(stopped)
        for token in notifications { NSWorkspace.shared.notificationCenter.removeObserver(token) }
        notifications = []
        CFRunLoopStop(CFRunLoopGetMain())
    }
}
