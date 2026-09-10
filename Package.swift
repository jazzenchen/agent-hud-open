// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "AgentHUDOpen",
    platforms: [.macOS(.v14)],
    products: [.library(name: "AgentHUDSupport", targets: ["AgentHUDSupport"])],
    targets: [
        .target(name: "AgentHUDSupport", swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "AgentHUDSupportTests", dependencies: ["AgentHUDSupport"]),
    ]
)
