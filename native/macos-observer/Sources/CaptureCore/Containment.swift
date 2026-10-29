import Darwin

// Applied only in the capture executable, before AppKit/Vision/TCC interaction.
// No networking is permitted even if a framework attempts it. A nested/unsupported
// sandbox fails closed. Error strings are freed without inspection or logging.
enum CaptureContainment {
    static func install() -> Bool {
        var coreLimit = rlimit(rlim_cur: 0, rlim_max: 0)
        guard setrlimit(RLIMIT_CORE, &coreLimit) == 0,
              let library = dlopen("/usr/lib/libsandbox.1.dylib", RTLD_NOW) else { return false }
        defer { dlclose(library) }
        guard let initSymbol = dlsym(library, "sandbox_init"),
              let freeSymbol = dlsym(library, "sandbox_free_error") else { return false }
        typealias Initialize = @convention(c) (UnsafePointer<CChar>, UInt64, UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Int32
        typealias FreeError = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
        let initialize = unsafeBitCast(initSymbol, to: Initialize.self)
        let freeError = unsafeBitCast(freeSymbol, to: FreeError.self)
        var error: UnsafeMutablePointer<CChar>?
        let result = "(version 1)(allow default)(deny network*)".withCString { initialize($0, 0, &error) }
        freeError(error)
        return result == 0
    }
}
