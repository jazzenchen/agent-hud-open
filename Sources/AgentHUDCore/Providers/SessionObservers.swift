import Foundation

/// The host opts into local adapter installation and supplies the executable that handles callbacks.
public enum SessionObservers {
    /// Adds this installation's handlers to every client installed here, or, with `enabled` false, takes them out.
    /// A handler another installation added stays with it, and every other entry in the clients' files is left alone.
    public static func configure(executable: URL, enabled: Bool,
                                 home: URL = FileManager.default.homeDirectoryForCurrentUser) {
        do {
            if enabled { try PiSessionObserver.configureIfAvailable(home: home) }
            else { try PiSessionObserver.configure(enabled: false, home: home) }
        } catch { NSLog("[AgentHUD] Pi observer setup failed: %@", error.localizedDescription) }
        for source in AttentionHooks.Source.allCases where source.isInstalled(home: home) {
            do { try AttentionHooks.configure(source, enabled: enabled, executable: executable, home: home) }
            catch { NSLog("[AgentHUD] Notification hook setup failed for %@: %@", source.rawValue, error.localizedDescription) }
        }
        for source in PermissionHooks.Source.allCases where source.isInstalled(home: home) {
            do { try PermissionHooks.configure(source, enabled: enabled, executable: executable, home: home) }
            catch { NSLog("[AgentHUD] Permission hook setup failed for %@: %@", source.rawValue, error.localizedDescription) }
        }
        for source in CompletionHooks.Source.allCases {
            guard AdditionalSource(rawValue: source.rawValue)?.isInstalled(home: home) == true else { continue }
            do { try CompletionHooks.configure(source, enabled: enabled, executable: executable, home: home) }
            catch { NSLog("[AgentHUD] Completion hook setup failed for %@: %@", source.rawValue, error.localizedDescription) }
        }
    }
}
