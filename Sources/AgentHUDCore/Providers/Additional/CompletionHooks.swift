import Foundation

/// A successful finished turn reported by a client's stop hook.
struct CompletionHookEvent: Equatable, Sendable {
    let session: String
    let turn: String
    var workspace: String? = nil
    var model: String? = nil
}

/// One client's hook configuration file and stop payload. `CompletionHooks` owns the callback, the local record and
/// the installation rules shared by every client.
protocol CompletionHookFormat {
    static func configuration(home: URL) -> URL
    /// The finished turn in a stop payload, or nil when the turn did not finish successfully. `now` is the callback time,
    /// for clients whose payload names no turn.
    static func completion(_ payload: ProviderJSON, now: Date) -> CompletionHookEvent?
    /// Commands of the Agent HUD handlers present in the configuration, enabled or not.
    static func commands(in configuration: [String: ProviderJSON]) -> [String]
    /// Whether an Agent HUD handler is present and active.
    static func isActive(in configuration: [String: ProviderJSON]) -> Bool
    /// The configuration with Agent HUD's handler set to `command`, or removed when `command` is nil.
    /// Every other entry is preserved; an unrecognized layout throws instead of being rewritten.
    static func updating(_ configuration: [String: ProviderJSON], command: String?) throws -> [String: ProviderJSON]
}

extension CompletionHookFormat {
    static func isActive(in configuration: [String: ProviderJSON]) -> Bool { !commands(in: configuration).isEmpty }
}

/// Explicit client stop callbacks. No credentials, prompts, or tool arguments are persisted.
public enum CompletionHooks {
    public enum Source: String, CaseIterable, Sendable {
        case antigravity, cursor, copilot, codebuddy, qwen
        var vendor: String { AdditionalSource(rawValue: rawValue)!.vendor }
        var format: any CompletionHookFormat.Type {
            switch self {
            case .antigravity: AntigravityHookFormat.self
            case .cursor: CursorHookFormat.self
            case .copilot: CopilotHookFormat.self
            case .codebuddy: CodeBuddyHookFormat.self
            case .qwen: QwenHookFormat.self
            }
        }
        func configuration(home: URL) -> URL { format.configuration(home: home) }
    }

    public static var directory: URL { AppSupport.directory.appendingPathComponent("turn-completions") }

    static func completion(source: Source, payload: ProviderJSON, now: Date) -> SessionCompletion? {
        guard let event = source.format.completion(payload, now: now), !event.session.isEmpty, !event.turn.isEmpty else { return nil }
        let title = event.workspace.map { URL(fileURLWithPath: $0).lastPathComponent }.flatMap { $0.isEmpty ? nil : $0 }
        return SessionCompletion(sessionID: "\(source.rawValue):\(event.session)", vendor: source.vendor, turnID: event.turn,
            task: title.map { "\(source.vendor) · \($0)" } ?? source.vendor,
            model: event.model ?? source.vendor, startedAt: nil, completedAt: now)
    }

    public static func record(source: Source, data: Data, now: Date = Date(), directory: URL = directory) throws {
        guard data.count <= 1024 * 1024 else { throw ProviderFailure.limit }
        guard let event = completion(source: source, payload: try ProviderJSON.read(data), now: now) else { return }
        let folder = directory.appendingPathComponent(source.rawValue)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let file = folder.appendingPathComponent(event.id + ".json")
        if !FileManager.default.fileExists(atPath: file.path) {
            try JSONEncoder().encode(event).write(to: file, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
        }
        // This is a short-lived local inbox for agent completion events.
        for file in try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.contentModificationDateKey]) where file.pathExtension == "json" {
            if let date = try file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
               date < now.addingTimeInterval(-30 * 86400) { try? FileManager.default.removeItem(at: file) }
        }
    }

    static func read(source: Source, since: Date, directory: URL = directory) throws -> [SessionCompletion] {
        let folder = directory.appendingPathComponent(source.rawValue)
        guard FileManager.default.fileExists(atPath: folder.path) else { return [] }
        return try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.contentModificationDateKey])
            .filter { $0.pathExtension == "json" }
            .compactMap { url in
                guard let date = try url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate, date >= since else { return nil }
                let data = try Data(contentsOf: url)
                guard data.count <= 64 * 1024 else { throw ProviderFailure.limit }
                let event = try JSONDecoder().decode(SessionCompletion.self, from: data)
                return event.vendor == source.vendor && event.completedAt >= since ? event : nil
            }
    }

    private static func configuration(_ source: Source, home: URL) throws -> [String: ProviderJSON] {
        let url = source.configuration(home: home)
        guard FileManager.default.fileExists(atPath: url.path) else { return [:] }
        guard let object = try ProviderFiles.json(url).objectValue else { throw ProviderFailure.format }
        return object
    }

    public static func isInstalled(_ source: Source, home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        guard let object = try? configuration(source, home: home) else { return false }
        return source.format.isActive(in: object)
    }

    /// The suffix that marks a handler command as Agent HUD's in formats without a dedicated entry.
    static func ownsCommand(_ command: String?, source: Source) -> Bool {
        command?.hasSuffix(" --completion-hook " + source.rawValue) == true
    }

    public static func configure(_ source: Source, enabled: Bool, executable: URL,
                                 home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                 replacingExisting: Bool = false) throws {
        let object = try configuration(source, home: home)
        let quoted = "'" + executable.path.replacingOccurrences(of: "'", with: "'\\''") + "'"
        let command = quoted + " --completion-hook " + source.rawValue
        if !replacingExisting && source.format.commands(in: object).contains(where: { $0 != command }) {
            throw UsageProviderError(L10n.text("完成回调由另一安装管理，请手动重新安装以切换", "Completion hook belongs to another installation; reinstall it explicitly to switch"))
        }
        let updated = try source.format.updating(object, command: enabled ? command : nil)
        guard updated != object else { return }
        let url = source.configuration(home: home)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(ProviderJSON.object(updated)).write(to: url, options: .atomic)
    }
}

/// `agent-hud` entry of `~/.gemini/config/hooks.json`.
enum AntigravityHookFormat: CompletionHookFormat {
    static func configuration(home: URL) -> URL { home.appendingPathComponent(".gemini/config/hooks.json") }

    static func completion(_ payload: ProviderJSON, now: Date) -> CompletionHookEvent? {
        guard payload["terminationReason"].stringValue == "model_stop", payload["fullyIdle"].boolValue == true,
              payload["error"].stringValue?.isEmpty != false, let execution = payload["executionNum"].countValue,
              let conversation = payload["conversationId"].stringValue else { return nil }
        return .init(session: conversation, turn: "execution-\(execution)",
                     workspace: payload["workspacePaths"].arrayValue?.first?.stringValue, model: payload["modelName"].stringValue)
    }

    static func commands(in configuration: [String: ProviderJSON]) -> [String] {
        (configuration["agent-hud"]?["Stop"].arrayValue ?? []).map { $0["command"].stringValue ?? "" }
    }

    static func isActive(in configuration: [String: ProviderJSON]) -> Bool {
        !commands(in: configuration).isEmpty && configuration["agent-hud"]?["enabled"].boolValue != false
    }

    static func updating(_ configuration: [String: ProviderJSON], command: String?) throws -> [String: ProviderJSON] {
        var object = configuration
        object["agent-hud"] = command.map { .object(["Stop": .array([.object([
            "type": .string("command"), "command": .string($0), "timeout": .integer(5)
        ])])]) }
        return object
    }
}

/// Handler appended to `hooks.stop` of a version-1 `~/.cursor/hooks.json`.
enum CursorHookFormat: CompletionHookFormat {
    static func configuration(home: URL) -> URL { home.appendingPathComponent(".cursor/hooks.json") }

    static func completion(_ payload: ProviderJSON, now: Date) -> CompletionHookEvent? {
        guard payload["hook_event_name"].stringValue == "stop", payload["status"].stringValue == "completed",
              let conversation = payload["conversation_id"].stringValue, let generation = payload["generation_id"].stringValue else { return nil }
        return .init(session: conversation, turn: generation, workspace: payload["workspace_roots"].arrayValue?.first?.stringValue,
                     model: payload["model_id"].stringValue ?? payload["model"].stringValue)
    }

    private static func owns(_ handler: ProviderJSON) -> Bool { CompletionHooks.ownsCommand(handler["command"].stringValue, source: .cursor) }

    static func commands(in configuration: [String: ProviderJSON]) -> [String] {
        (configuration["hooks"]?["stop"].arrayValue ?? []).filter(owns).map { $0["command"].stringValue ?? "" }
    }

    static func updating(_ configuration: [String: ProviderJSON], command: String?) throws -> [String: ProviderJSON] {
        var object = configuration
        guard object["version"] == nil || object["version"] == .integer(1),
              object["hooks"] == nil || object["hooks"]?.objectValue != nil else { throw ProviderFailure.format }
        var hooks = object["hooks"]?.objectValue ?? [:]
        guard hooks["stop"] == nil || hooks["stop"]?.arrayValue != nil else { throw ProviderFailure.format }
        var handlers = (hooks["stop"]?.arrayValue ?? []).filter { !owns($0) }
        if let command { handlers.append(.object(["command": .string(command), "timeout": .integer(5)])) }
        hooks["stop"] = handlers.isEmpty ? nil : .array(handlers)
        object["hooks"] = .object(hooks); object["version"] = .integer(1)
        return object
    }
}
