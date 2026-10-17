import AppKit
import SwiftUI
import AgentHUDCore

/// Settings → Experimental: what is still being tested. Agent HUD's own rows let agents arrange the HUD over MCP, in
/// two rows: the switch, off by default, and the clients that have the server, with a menu that adds it to a client
/// or takes it out again and what to ask once it is in. A host's rows follow below.
struct ExperimentalPane: View {
    let settings: SettingsStore
    let theme: Theme
    var additions: [DesktopSettingsSection] = []
    @State private var setup: MCPClientSetup

    init(settings: SettingsStore, theme: Theme, additions: [DesktopSettingsSection] = [], setup: MCPClientSetup? = nil) {
        self.settings = settings
        self.theme = theme
        self.additions = additions
        _setup = State(initialValue: setup ?? MCPClientSetup())
    }

    var body: some View {
        let toolsOn = settings.settings.hudTools
        VStack(alignment: .leading, spacing: 28) {
            SettingsSection(title: L10n.text("Agent 工具", "Agent tools"), theme: theme) {
                SettingRow(label: L10n.text("允许 Agent 调整 HUD", "Let agents arrange the HUD"),
                           subtitle: L10n.text("Agent 可以帮你重排面板、改 HUD 样式。改动先是草稿，你在面板里点“保存”才生效。",
                                               "Agents can rearrange the panel and restyle the HUD. A change stays a draft until you press Save in the panel.")) {
                    Toggle(L10n.text("允许 Agent 调整 HUD", "Let agents arrange the HUD"), isOn: settings.binding(\.hudTools))
                        .labelsHidden().toggleStyle(.switch).controlSize(.small)
                        .accessibilityIdentifier("experimental-hud-tools")
                }
                SettingsDivider(theme: theme)
                clientsRow(toolsOn: toolsOn)
            }
            ForEach(additions) { $0.content() }
        }
        .onAppear { setup.refresh() }
        // A command copied and run in a terminal shows here when the person comes back.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in setup.refresh() }
    }

    private func clientsRow(toolsOn: Bool) -> some View {
        let controls = setup.controls(toolsOn: toolsOn)
        return HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(setup.title)
                    .foregroundStyle(toolsOn || !setup.added.isEmpty ? theme.text : theme.secondary)
                    .accessibilityIdentifier("experimental-agent-tools-title")
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(setup.detail(toolsOn: toolsOn))
                        .foregroundStyle(theme.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("experimental-agent-tools-status")
                    // The command that failed, for a person who would rather run it in a terminal.
                    if let commandLine = setup.failedCommandLine {
                        Button(L10n.text("拷贝命令", "Copy command")) { setup.copyFailedCommand() }
                            .buttonStyle(.plain)
                            .foregroundStyle(theme.accentText)
                            .underline(theme.isSeeThrough)
                            .help(commandLine)
                            .accessibilityIdentifier("experimental-agent-tools-copy")
                    }
                }
                .font(.ui(11))
            }
            Spacer(minLength: 0)
            Menu(MCPClientSetup.menuTitle) {
                ForEach(setup.clients, id: \.self) { client in
                    Toggle(client.name, isOn: Binding(get: { setup.added.contains(client) }, set: { _ in pick(client) }))
                        .disabled(!setup.canPick(client, toolsOn: toolsOn))
                }
                if !setup.clients.isEmpty { Divider() }
                Button(MCPClientSetup.copySettingsTitle) { Task { await setup.perform(.copySettings) } }
                    .disabled(!controls.copySettings)
            }
            .fixedSize()
            .disabled(!controls.menu)
            .accessibilityIdentifier("experimental-agent-tools-client")
        }
        .frame(minHeight: 30)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
    }

    /// Adds the server to a client without it, or takes it out of one that has it.
    private func pick(_ client: MCPClient) {
        guard let action = setup.action(for: client) else { return }
        Task { await setup.perform(action) }
    }
}
