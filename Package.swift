// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "AgentHUDOpen",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "AgentHUDSupport", targets: ["AgentHUDSupport"]),
        .library(name: "AgentHUDCore", targets: ["AgentHUDCore"]),
    ],
    targets: [
        .target(name: "AgentHUDSupport", swiftSettings: [.swiftLanguageMode(.v6)]),
        .target(name: "AgentHUDCore", dependencies: ["AgentHUDSupport"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "AgentHUDSupportTests", dependencies: ["AgentHUDSupport"]),
        .testTarget(name: "AgentHUDCoreTests", dependencies: ["AgentHUDCore", "AgentHUDSupport"]),
    ]
)
