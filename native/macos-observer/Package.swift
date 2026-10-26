// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MacroscopeObserver",
    platforms: [.macOS(.v15)],
    products: [
        .executable(name: "macroscope-observer", targets: ["MacroscopeObserver"]),
        .library(name: "ObserverCore", targets: ["ObserverCore"]),
    ],
    targets: [
        .target(name: "ObserverCore"),
        .executableTarget(name: "MacroscopeObserver", dependencies: ["ObserverCore"]),
        .testTarget(name: "ObserverCoreTests", dependencies: ["ObserverCore", "MacroscopeObserver"]),
    ]
)
