import Foundation

/// Application-owned caches and local usage history.
public enum AppSupport {
    public static var directory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support")
        let name = Bundle.main.object(forInfoDictionaryKey: "AgentHUDDataDirectory") as? String ?? "Agent HUD Open"
        return base.appendingPathComponent(name, isDirectory: true)
    }
}
