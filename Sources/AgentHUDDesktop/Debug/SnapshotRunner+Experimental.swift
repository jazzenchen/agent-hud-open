import AppKit
import SwiftUI
import AgentHUDCore

#if DEBUG
/// The stills of Settings → Experimental, named `settings-experimental-…`.
extension SnapshotRunner {
    /// The Experimental page with the switch on and Codex added; with no client added; in light; at the smallest window;
    /// and with the switch off while Codex has the server, so only its check mark can be taken off. A sample Mac stands
    /// in for this one: Claude Code and Codex installed, the application in Applications, and no command runs.
    static func saveExperimental(settings: SettingsStore, store: UsageStore, folder: URL) {
        if let prefix = ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"],
           !"settings-experimental".hasPrefix(prefix) && !prefix.hasPrefix("settings-experimental") { return }
        let saved = settings.settings.hudTools
        defer { settings.update { $0.hudTools = saved } }
        func shoot(_ name: String, added: Set<MCPClient> = [.codex], toolsOn: Bool, scheme: ColorScheme = .dark,
                   size: CGSize = SettingsWindowLayout.size) {
            settings.update { $0.hudTools = toolsOn }
            let setup = MCPClientSetup(environment: .sample(added: added))
            setup.refresh()
            save(name, SettingsView(settings: settings, store: store, initialTab: .experimental, clientSetup: setup)
                .frame(width: size.width, height: size.height), folder: folder, scheme: scheme)
        }
        shoot("settings-experimental-dark", toolsOn: true)
        shoot("settings-experimental-none-dark", added: [], toolsOn: true)
        shoot("settings-experimental-light", toolsOn: true, scheme: .light)
        shoot("settings-experimental-small-dark", toolsOn: true, size: SettingsWindowLayout.minimum)
        shoot("settings-experimental-off-dark", toolsOn: false)
    }
}

extension MCPClientSetup.Environment {
    /// A Mac with Claude Code and Codex, those named already given the server, and Agent HUD in Applications. Nothing
    /// runs and nothing is copied.
    static func sample(added: Set<MCPClient> = [.codex]) -> Self {
        Self(executable: URL(fileURLWithPath: "/Applications/Agent HUD Open.app/Contents/MacOS/Agent HUD Open"),
             locate: { URL(fileURLWithPath: "/usr/local/bin/\($0.command)") },
             hasServer: { added.contains($0) },
             run: { _, _ in MCPCommandResult(status: 0) },
             copy: { _ in })
    }
}
#endif
