import Foundation

/// One bounded mailbox shared by the stdin queue and the main run loop.
/// Enqueueing *any* command closes capture admission synchronously. EOF closes it permanently,
/// even if the main thread is inside an AX call. There is at most one scheduled drain callback.
final class ControlInbox: @unchecked Sendable {
    enum End: Sendable { case eof, error }
    struct Batch: Sendable {
        let controls: [ControlMessage]
        let end: End?
    }

    private let lock = NSLock()
    private var controls: [ControlMessage] = []
    private var end: End?
    private var scheduled = false
    private var enabled = false

    func enqueue(_ message: ControlMessage) throws -> Bool {
        lock.lock()
        defer { lock.unlock() }
        enabled = false
        guard end == nil, controls.count < WireLimits.pendingControls else { throw WireError.backlog }
        controls.append(message)
        return scheduleLocked()
    }

    func finish(_ reason: End) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        enabled = false
        if end == nil { end = reason }
        return scheduleLocked()
    }

    func take() -> Batch {
        lock.lock()
        defer { lock.unlock() }
        let batch = Batch(controls: controls, end: end)
        controls.removeAll(keepingCapacity: true)
        scheduled = false
        return batch
    }

    func enableCapture(_ active: Bool) {
        lock.lock()
        defer { lock.unlock() }
        enabled = active && end == nil && controls.isEmpty
    }

    func captureAllowed() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return enabled && end == nil
    }

    private func scheduleLocked() -> Bool {
        guard !scheduled else { return false }
        scheduled = true
        return true
    }
}
