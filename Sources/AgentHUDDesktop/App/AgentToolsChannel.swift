import AgentHUDCore
import Foundation

/// Opens and closes the MCP tool channel with the switch in Settings → Experimental: while it is off nothing answers on
/// `mcp.sock`, and the `--mcp` process tells a client's agent that the tools are off. A demo started with
/// `--demo-tools` serves them whatever the switch says.
@MainActor
final class AgentToolsChannel {
    enum Mode: Equatable {
        /// A demo without `--demo-tools`, which leaves the socket to the real copy.
        case never
        /// While `Settings.hudTools` is on.
        case whileSwitchedOn
        /// A demo started with `--demo-tools`.
        case always
    }

    let channel: MCPAppChannel
    private let settings: SettingsStore
    private let mode: Mode
    private var stopped = false

    init(channel: MCPAppChannel, settings: SettingsStore, mode: Mode) {
        self.channel = channel
        self.settings = settings
        self.mode = mode
    }

    /// Serves the channel when it should be served now, and from then on follows the switch.
    func start() {
        stopped = false
        apply()
        guard mode == .whileSwitchedOn else { return }
        trackChanges({ [weak self] in self?.settings.settings.hudTools }, onChange: { [weak self] in self?.apply() })
    }

    /// Closes the channel for good, as the application quits.
    func stop() {
        stopped = true
        channel.stop()
    }

    private var serves: Bool {
        switch mode {
        case .never: return false
        case .whileSwitchedOn: return settings.settings.hudTools
        case .always: return true
        }
    }

    private func apply() {
        guard !stopped else { return }
        if serves { channel.start() } else { channel.stop() }
    }
}
