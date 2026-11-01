// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MacroscopeObserver",
    platforms: [.macOS(.v15)],
    products: [
        .executable(name: "macroscope-bot-matte", targets: ["MacroscopeBotMatte"]),
        .executable(name: "macroscope-observer", targets: ["MacroscopeObserver"]),
        .library(name: "ObserverCore", targets: ["ObserverCore"]),
        .executable(name: "macroscope-capture", targets: ["MacroscopeCapture"]),
        .library(name: "CaptureCore", targets: ["CaptureCore"]),
    ],
    targets: [
        .executableTarget(name: "MacroscopeBotMatte"),
        .target(name: "ObserverCore"),
        .executableTarget(name: "MacroscopeObserver", dependencies: ["ObserverCore"]),
        .testTarget(name: "ObserverCoreTests", dependencies: ["ObserverCore", "MacroscopeObserver"]),
        .target(name: "CaptureCore", dependencies: ["ObserverCore"]),
        .executableTarget(name: "MacroscopeCapture", dependencies: ["CaptureCore"]),
        .testTarget(name: "CaptureCoreTests", dependencies: ["CaptureCore"]),
    ]
)
