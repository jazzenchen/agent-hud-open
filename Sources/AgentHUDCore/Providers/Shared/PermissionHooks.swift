import AgentHUDSupport
import Foundation

/// The hook a client runs when it is about to ask its user whether a tool may run.
///
/// The notification hook only says a session needs its user; this one is answered. The client waits on the hook's own
/// output and acts on what it says, so a request can be approved from the HUD instead of the terminal. Saying nothing
/// is always available and always safe: the client then behaves exactly as it would with no hook installed.
public enum PermissionHooks {
    /// Clients with the PermissionRequest hook contract: it runs only when the client is about to ask, and the client
    /// reads Claude Code's allow/deny answer. Codex CLI and Desktop share one hooks file, WorkBuddy runs CodeBuddy
    /// Code's engine, ZCode's desktop app and terminal share one engine and one configuration file, and Qwen Code
    /// reads the answer unchanged; only Claude Code and the Qoder builds apply a permission-rule update sent back.
    public enum Source: String, CaseIterable, Sendable {
        case claude
        case codex
        case qoder
        case qoderCN
        case qoderWork
        case codebuddy
        case workbuddy
        case zcode
        case qwen

        public var vendor: String {
            switch self {
            case .claude: return "Claude"
            case .codex: return "Codex"
            case .qoder: return "Qoder"
            case .qoderCN: return "Qoder CN"
            case .qoderWork: return "QoderWork"
            case .codebuddy: return "CodeBuddy"
            case .workbuddy: return "WorkBuddy"
            case .zcode: return "ZCode"
            case .qwen: return "Qwen"
            }
        }

        var event: String { "PermissionRequest" }
        /// A rule is echoed back only where the client both offers one and applies it. CodeBuddy Code offers
        /// suggestions but never applies one sent back, ZCode applies a rule but never offers one, and Codex and
        /// Qwen Code do neither.
        var supportsPermissionUpdates: Bool {
            switch self {
            case .claude, .qoder, .qoderCN, .qoderWork: return true
            case .codex, .codebuddy, .workbuddy, .zcode, .qwen: return false
            }
        }
        /// Matched against the tool name; empty is every tool. ZCode rejects an empty matcher and runs a group without
        /// one for every tool.
        var matcher: String? { self == .zcode ? nil : "" }
        /// Claude Code's layout keeps the event lists at `hooks.<Event>`; ZCode nests them at `hooks.events.<Event>`.
        var nestsEvents: Bool { self == .zcode }
        /// Calls that ask the user something other than permission. ZCode routes its question and its plan approval
        /// through this event, and an answer without the user's reply fails the question or approves an unread plan;
        /// Qwen Code ignores an allow for both. The HUD leaves them to the client's own dialog.
        var unanswerableTools: Set<String> {
            switch self {
            case .zcode: return ["AskUserQuestion", "ExitPlanMode"]
            case .qwen: return ["ask_user_question", "exit_plan_mode"]
            default: return []
            }
        }
        /// How long the client waits for an answer. A request stays on the HUD until it is answered or the client
        /// withdraws it, so this only has to outlast a user who walked away. A client that cancels the hook first
        /// closes the connection, which takes the request off the HUD.
        /// Qwen Code reads a value of 1000 or more as milliseconds on every version, so its day is written that way.
        var timeout: Int { self == .qwen ? 86_400_000 : 86_400 }

        var directory: String {
            switch self {
            case .claude: return ".claude"
            case .codex: return ".codex"
            case .qoder: return ".qoder"
            case .qoderCN: return ".qoder-cn"
            case .qoderWork: return ".qoderwork"
            case .codebuddy: return ".codebuddy"
            case .workbuddy: return ".workbuddy"
            case .zcode: return ".zcode/cli"
            case .qwen: return ".qwen"
            }
        }

        func home(_ base: URL) -> URL {
            if case .codex = self { return CodexLocator.dataDirectory(home: base) }
            if case .codebuddy = self { return CodeBuddySessions.home(base) }
            if case .qwen = self { return QwenSessions.home(base) }
            // Claude Code's configuration directory moves with CLAUDE_CONFIG_DIR; the forks have no such variable.
            if case .claude = self, let configured = ClaudeSubscription.configDirectory { return configured }
            return base.appendingPathComponent(directory, isDirectory: true)
        }

        func configuration(home base: URL) -> URL {
            let name: String
            switch self {
            case .codex: name = "hooks.json"
            case .zcode: name = "config.json"
            default: name = "settings.json"
            }
            return home(base).appendingPathComponent(name)
        }

        /// Whether the client is here at all. A machine without it keeps its home untouched.
        public func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                fileManager: FileManager = .default) -> Bool {
            switch self {
            case .claude:
                return ClaudeEngineLocator.find(home: home, fileManager: fileManager) != nil
                    || fileManager.fileExists(atPath: home.appendingPathComponent(".claude/projects").path)
            case .codex, .qoder, .qoderCN, .qoderWork, .zcode, .qwen:
                return fileManager.fileExists(atPath: self.home(home).path)
            // The session folder is what the usage provider reads; a settings folder alone can be the IDE extension's.
            case .codebuddy, .workbuddy:
                return fileManager.fileExists(atPath: self.home(home).appendingPathComponent("projects").path)
            }
        }
    }

    // MARK: Installation

    static func configuration(_ source: Source, home: URL) throws -> [String: ProviderJSON] {
        let url = source.configuration(home: home)
        guard FileManager.default.fileExists(atPath: url.path) else { return [:] }
        let data = try Data(contentsOf: url)
        guard data.count <= 4 * 1024 * 1024 else { throw ProviderFailure.limit }
        guard !data.isEmpty else { return [:] }
        guard let object = try ProviderJSON.read(data).objectValue else { throw ProviderFailure.format }
        return object
    }

    static func ownsCommand(_ command: String?, source: Source) -> Bool {
        command?.hasSuffix(" --permission-hook " + source.rawValue) == true
    }

    public static func isActive(_ source: Source, home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        guard let object = try? configuration(source, home: home) else { return false }
        return !commands(in: object, source: source).isEmpty
    }

    static func commands(in configuration: [String: ProviderJSON], source: Source) -> [String] {
        let hooks = ProviderJSON.object(configuration)["hooks"]
        let events = source.nestsEvents ? hooks["events"] : hooks
        return (events[source.event].arrayValue ?? []).flatMap { $0["hooks"].arrayValue ?? [] }
            .compactMap { $0["command"].stringValue }.filter { ownsCommand($0, source: source) }
    }

    /// Adds or removes Agent HUD's handler, leaving every other hook in the file alone. An unrecognized layout throws
    /// rather than being rewritten.
    public static func configure(_ source: Source, enabled: Bool, executable: URL,
                                 home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                 replacingExisting: Bool = false) throws {
        // Taking a handler out never leaves behind a file the client did not have.
        guard enabled || FileManager.default.fileExists(atPath: source.configuration(home: home).path) else { return }
        let object = try configuration(source, home: home)
        let quoted = "'" + executable.path.replacingOccurrences(of: "'", with: "'\\''") + "'"
        let command = quoted + " --permission-hook " + source.rawValue
        if !replacingExisting && commands(in: object, source: source).contains(where: { $0 != command }) {
            throw UsageProviderError(L10n.text("批准回调由另一安装管理，请手动重新安装以切换",
                                               "The permission hook belongs to another installation; reinstall it explicitly to switch"))
        }
        let updated = try updating(object, source: source, command: enabled ? command : nil)
        guard updated != object else { return }
        let url = source.configuration(home: home)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        // A configuration file can hold server credentials; the rewrite keeps whatever access the client gave it.
        let permissions = try? FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions]
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(ProviderJSON.object(updated)).write(to: url, options: .atomic)
        if let permissions { try FileManager.default.setAttributes([.posixPermissions: permissions], ofItemAtPath: url.path) }
    }

    static func updating(_ configuration: [String: ProviderJSON], source: Source, command: String?) throws -> [String: ProviderJSON] {
        var object = configuration
        guard object["hooks"] == nil || object["hooks"]?.objectValue != nil else { throw ProviderFailure.format }
        var hooks = object["hooks"]?.objectValue ?? [:]
        if source.nestsEvents {
            // ZCode drops its whole configuration over a malformed hooks section, so only its known shapes are edited.
            guard hooks["events"] == nil || hooks["events"]?.objectValue != nil,
                  hooks["enabled"] == nil || hooks["enabled"]?.boolValue != nil else { throw ProviderFailure.format }
        }
        var events = source.nestsEvents ? hooks["events"]?.objectValue ?? [:] : hooks
        guard events[source.event] == nil || events[source.event]?.arrayValue != nil else { throw ProviderFailure.format }
        var groups = (events[source.event]?.arrayValue ?? []).compactMap { group -> ProviderJSON? in
            guard var fields = group.objectValue, let handlers = fields["hooks"]?.arrayValue else { return group }
            let kept = handlers.filter { !ownsCommand($0["command"].stringValue, source: source) }
            if kept.count == handlers.count { return group }
            if kept.isEmpty { return nil }
            fields["hooks"] = .array(kept)
            return .object(fields)
        }
        if let command {
            var group: [String: ProviderJSON] = ["hooks": .array([.object(["type": .string("command"), "command": .string(command),
                                                                           "timeout": .integer(Int64(source.timeout))])])]
            if let matcher = source.matcher { group["matcher"] = .string(matcher) }
            groups.append(.object(group))
        }
        events[source.event] = groups.isEmpty ? nil : .array(groups)
        if source.nestsEvents {
            hooks["events"] = events.isEmpty ? nil : .object(events)
            // ZCode runs no hook at all until this is set. A user who switched hooks off keeps them off.
            if command != nil, hooks["enabled"] == nil { hooks["enabled"] = .bool(true) }
        } else {
            hooks = events
        }
        object["hooks"] = hooks.isEmpty && configuration["hooks"] == nil ? nil : .object(hooks)
        return object
    }
}
