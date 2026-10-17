import AppKit
import AgentHUDCore
import Observation

/// What the clients row on Settings → Experimental offers: which clients on this Mac have the MCP server, in one line,
/// and a menu with a check mark for each client that takes a local MCP server through its own command line, which
/// adds the server where it is unchecked and removes it where it is checked, then the server's settings to copy for
/// any other client. Whether a client has the server is read from the client's own configuration.
///
/// A client's command runs only when a person picks it in the menu, never on its own. The executable added is the
/// running application's, and one that runs from a disk image or App Translocation is not offered, as the hooks are
/// not, since the client would lose it when the application is moved.
@MainActor
@Observable
final class MCPClientSetup {
    enum Action: Equatable {
        case add(MCPClient)
        case remove(MCPClient)
        case copySettings

        var client: MCPClient? {
            switch self {
            case .add(let client), .remove(let client): return client
            case .copySettings: return nil
            }
        }
    }

    /// What happened last, until another action starts.
    enum Note: Equatable {
        case failed(Action, String)
        case copiedCommand
        case copiedSettings
    }

    /// What a person can use, with the switch on or off: the menu, its unchecked clients, its checked ones, and the
    /// settings to copy.
    struct Controls: Equatable {
        var menu: Bool
        var add: Bool
        var remove: Bool
        var copySettings: Bool
    }

    /// Everything the row reaches outside itself, so a test can stand in for each.
    struct Environment {
        /// The running application's executable.
        var executable: URL?
        /// The client's command on this Mac, nil when it has none.
        var locate: (MCPClient) -> URL?
        /// Whether the client's configuration has the server.
        var hasServer: (MCPClient) -> Bool
        /// Runs a client's command and says how it went.
        var run: @Sendable (URL, [String]) async -> MCPCommandResult
        /// Puts text on the pasteboard.
        var copy: (String) -> Void

        static var live: Environment {
            Environment(executable: Bundle.main.executableURL,
                        locate: { $0.executable() },
                        hasServer: { $0.hasServer() },
                        run: { await MCPClientCommand.run($0, $1) },
                        copy: { text in
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(text, forType: .string)
                        })
        }
    }

    private let environment: Environment
    private(set) var clients: [MCPClient] = []
    private(set) var added: Set<MCPClient> = []
    private(set) var running: Action?
    private(set) var note: Note?
    private var executables: [MCPClient: URL] = [:]

    init(environment: Environment = .live) {
        self.environment = environment
    }

    /// Looks again for the clients and reads their configurations, as when a person comes back from running the copied
    /// command in a terminal: a change there takes the place of the note about the copy.
    func refresh() {
        guard running == nil else { return }
        executables = [:]
        for client in MCPClient.allCases {
            if let url = environment.locate(client) { executables[client] = url }
        }
        clients = MCPClient.allCases.filter { executables[$0] != nil }
        let before = added
        added = Set(clients.filter(environment.hasServer))
        if added != before, note == .copiedCommand { note = nil }
    }

    var executablePath: String? { environment.executable?.path }

    /// Whether the application runs from a place it only stays in until it is moved, or from nowhere known.
    var isTransient: Bool { executablePath.map(HookCommand.isTransient) ?? true }

    /// What picking the client in the menu does: Remove where it has the server, else Add, which waits until the
    /// application is in a place it stays in.
    func action(for client: MCPClient) -> Action? {
        if added.contains(client) { return .remove(client) }
        return isTransient ? nil : .add(client)
    }

    /// While the switch is off everything is dimmed except a checked client, so the server can still be taken out.
    func controls(toolsOn: Bool) -> Controls {
        let idle = running == nil
        let movable = !isTransient
        return Controls(menu: idle && (toolsOn || !added.isEmpty),
                        add: idle && toolsOn && movable,
                        remove: idle,
                        copySettings: idle && toolsOn && movable)
    }

    /// Whether the client's item in the menu can be picked now.
    func canPick(_ client: MCPClient, toolsOn: Bool) -> Bool {
        let controls = controls(toolsOn: toolsOn)
        return added.contains(client) ? controls.remove : controls.add
    }

    /// The command that failed last, as a person would type it, so they can run it in a terminal instead.
    var failedCommandLine: String? {
        guard case .failed(let action, _)? = note, let client = action.client, let arguments = arguments(for: action)
        else { return nil }
        return client.commandLine(arguments)
    }

    private func arguments(for action: Action) -> [String]? {
        switch action {
        case .add(let client): return executablePath.map(client.addArguments(executable:))
        case .remove(let client): return client.removeArguments
        case .copySettings: return nil
        }
    }

    /// Does what the menu item says. A client's command reports back in the row, which reads the client's
    /// configuration again afterwards rather than trusting the exit status alone.
    func perform(_ action: Action) async {
        guard running == nil else { return }
        switch action {
        case .copySettings:
            guard !isTransient, let executablePath else { return }
            note = nil
            environment.copy(MCPClient.settingsJSON(executable: executablePath))
            note = .copiedSettings
        case .add(let client), .remove(let client):
            guard self.action(for: client) == action, let executable = executables[client],
                  let arguments = arguments(for: action) else { return }
            note = nil
            running = action
            let result = await environment.run(executable, arguments)
            running = nil
            added = Set(clients.filter(environment.hasServer))
            if !result.succeeded {
                note = .failed(action, result.failureLine)
            } else if added.contains(client) != (action == .add(client)) {
                note = .failed(action, L10n.text("\(client.name) 的设置里没有相应变化", "\(client.name)'s settings did not change"))
            }
        }
    }

    func copyFailedCommand() {
        guard running == nil, let commandLine = failedCommandLine else { return }
        environment.copy(commandLine)
        note = .copiedCommand
    }

    // MARK: Words

    static var menuTitle: String { L10n.text("接入客户端", "Add to client") }
    static var copySettingsTitle: String { L10n.text("拷贝配置（其他客户端）", "Copy settings for another client") }

    /// The clients that have the server, in the menu's order.
    private var addedClients: [MCPClient] { clients.filter(added.contains) }

    /// The row's label: which clients have the server.
    var title: String {
        let names = addedClients.map(\.name)
        guard !names.isEmpty else { return L10n.text("还没有接入客户端", "Not added to a client yet") }
        return L10n.text("已接入 \(names.joined(separator: "、"))", "Added to \(Self.joined(names))")
    }

    /// One line under it on where things stand: a command running, what happened last, or what to do next.
    func detail(toolsOn: Bool) -> String {
        switch running {
        case .add(let client)?: return L10n.text("正在接入 \(client.name)…", "Adding to \(client.name)…")
        case .remove(let client)?: return L10n.text("正在从 \(client.name) 移除…", "Removing from \(client.name)…")
        case .copySettings?, nil: break
        }
        switch note {
        case .failed(.remove(let client), let line)?:
            return L10n.text("没能从 \(client.name) 移除：\(line)", "Couldn't remove from \(client.name): \(line)")
        case .failed(let action, let line)?:
            let name = action.client?.name ?? ""
            return L10n.text("没能接入 \(name)：\(line)", "Couldn't add to \(name): \(line)")
        case .copiedCommand?: return L10n.text("已拷贝命令，粘贴到终端运行。", "Command copied. Paste it into Terminal.")
        case .copiedSettings?:
            return L10n.text("已拷贝配置，粘贴到客户端的 MCP 设置里。", "Settings copied. Paste them into your client's MCP settings.")
        case nil: break
        }
        if isTransient { return L10n.text("请先把 Agent HUD 移到“应用程序”文件夹。", "Move Agent HUD to Applications first.") }
        let names = addedClients
        if !names.isEmpty {
            guard toolsOn else { return L10n.text("打开上面的开关，Agent 才能用。", "Turn on the switch above so agents can use it.") }
            if names.count == 1 {
                return L10n.text("在 \(names[0].name) 里新开一个会话，说“把 HUD 改成……”。",
                                 "Start a new \(names[0].name) session and ask it to change the HUD.")
            }
            return L10n.text("新开一个会话，说“把 HUD 改成……”。", "Start a new session and ask it to change the HUD.")
        }
        guard toolsOn else {
            return L10n.text("先打开上面的开关，再选一个客户端接入。", "Turn on the switch above, then pick a client to add it to.")
        }
        if clients.isEmpty {
            let supported = MCPClient.allCases.map(\.name)
            return L10n.text("这台 Mac 上没有 \(supported.joined(separator: " 或 "))，可以拷贝配置给其他客户端。",
                             "No \(supported.joined(separator: " or ")) on this Mac. Copy the settings for another client.")
        }
        return L10n.text("从右边的菜单选一个客户端接入。", "Pick a client from the menu to add it.")
    }

    /// "A", "A and B", "A, B and C".
    private static func joined(_ names: [String]) -> String {
        guard names.count > 1 else { return names.first ?? "" }
        return names.dropLast().joined(separator: ", ") + " and " + names[names.count - 1]
    }
}
