import Foundation

/// SwiftPM resources are nested inside Resources when the executable is bundled as a macOS app.
enum AppResources {
    static var applicationName: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? "Agent HUD Open" }

    static let bundle = Bundle.main.url(forResource: "AgentHUDOpen_AgentHUDDesktop", withExtension: "bundle")
        .flatMap(Bundle.init(url:)) ?? Bundle.module
}
