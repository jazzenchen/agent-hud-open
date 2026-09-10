import Foundation
import AgentHUDCore

/// Command-line switches used for development and visual verification.
public struct DesktopLaunchOptions: Equatable {
    /// `--snapshot <dir>`: render every screen to PNG files in `dir`, then quit.
    public var snapshotDirectory: String?
    /// `--show-onboarding`: show the first-launch window even if completed.
    public var showOnboarding = false
    /// `--open-panel`: start with the notch panel expanded.
    public var openPanel = false
    public var showSettings = false
    public var showStats = false
    /// `--reset-defaults`: wipe stored settings before starting.
    public var resetDefaults = false
    /// `--demo`: show the design's demo data instead of querying installed agents.
    public var demo = false
    /// `--probe`: run one real fetch, print a summary, quit (diagnostics).
    public var probe = false
    /// `--lang zh|en`: force the UI language for this launch (snapshots, manual checks).
    public var language: AppLanguage?

    public init() {}

    public static func parse(_ arguments: [String]) -> DesktopLaunchOptions {
        var options = DesktopLaunchOptions()
        var iterator = arguments.dropFirst().makeIterator()
        while let argument = iterator.next() {
            switch argument {
            case "--snapshot": options.snapshotDirectory = iterator.next()
            case "--show-onboarding": options.showOnboarding = true
            case "--open-panel": options.openPanel = true
            case "--show-settings": options.showSettings = true
            case "--show-stats": options.showStats = true
            case "--reset-defaults": options.resetDefaults = true
            case "--demo": options.demo = true
            case "--probe": options.probe = true
            case "--lang": options.language = iterator.next().flatMap(Self.language(from:))
            default: break
            }
        }
        return options
    }

    private static func language(from value: String) -> AppLanguage? {
        switch value.lowercased() {
        case "zh", "zh-hans", "cn": return .zhHans
        case "en": return .en
        case "system": return .system
        default: return nil
        }
    }
}
