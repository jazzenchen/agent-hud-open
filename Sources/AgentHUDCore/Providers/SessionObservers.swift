import Foundation

/// The host opts into local adapter installation and supplies the executable that handles callbacks.
public enum SessionObservers {
    public static func configure(executable: URL) {
        do { try PiSessionObserver.configureIfAvailable() }
        catch { NSLog("[AgentHUD] Pi observer setup failed: %@", error.localizedDescription) }
        for source in CompletionHooks.Source.allCases {
            guard AdditionalSource(rawValue: source.rawValue)?.isInstalled() == true else { continue }
            do { try CompletionHooks.configure(source, enabled: true, executable: executable) }
            catch { NSLog("[AgentHUD] Completion hook setup failed for %@: %@", source.rawValue, error.localizedDescription) }
        }
    }
}
