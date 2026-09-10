import Foundation

/// Explicit client stop callbacks. No credentials, prompts, or tool arguments are persisted.
public enum CompletionHooks {
    public enum Source: String, CaseIterable, Sendable {
        case antigravity, cursor
        var vendor: String { self == .antigravity ? "Antigravity" : "Cursor" }
        func configuration(home: URL) -> URL {
            home.appendingPathComponent(self == .antigravity ? ".gemini/config/hooks.json" : ".cursor/hooks.json")
        }
    }

    public static var directory: URL { AppSupport.directory.appendingPathComponent("turn-completions") }

    static func completion(source: Source, payload: ProviderJSON, now: Date) -> SessionCompletion? {
        let conversation: String?, turn: String?, workspace: String?, model: String?
        switch source {
        case .antigravity:
            guard payload["terminationReason"].stringValue == "model_stop", payload["fullyIdle"].boolValue == true,
                  payload["error"].stringValue?.isEmpty != false, let execution = payload["executionNum"].countValue else { return nil }
            conversation = payload["conversationId"].stringValue
            turn = "execution-\(execution)"
            workspace = payload["workspacePaths"].arrayValue?.first?.stringValue
            model = payload["modelName"].stringValue
        case .cursor:
            guard payload["hook_event_name"].stringValue == "stop", payload["status"].stringValue == "completed" else { return nil }
            conversation = payload["conversation_id"].stringValue
            turn = payload["generation_id"].stringValue
            workspace = payload["workspace_roots"].arrayValue?.first?.stringValue
            model = payload["model_id"].stringValue ?? payload["model"].stringValue
        }
        guard let conversation, !conversation.isEmpty, let turn, !turn.isEmpty else { return nil }
        let title = workspace.map { URL(fileURLWithPath: $0).lastPathComponent }.flatMap { $0.isEmpty ? nil : $0 }
        return SessionCompletion(sessionID: "\(source.rawValue):\(conversation)", vendor: source.vendor, turnID: turn,
            task: title.map { "\(source.vendor) · \($0)" } ?? source.vendor,
            model: model ?? source.vendor, startedAt: nil, completedAt: now)
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
        if source == .antigravity { return object["agent-hud"]?["Stop"].arrayValue?.isEmpty == false && object["agent-hud"]?["enabled"].boolValue != false }
        return object["hooks"]?["stop"].arrayValue?.contains(where: ownsCursorHandler) == true
    }

    private static func ownsCursorHandler(_ handler: ProviderJSON) -> Bool {
        handler["command"].stringValue?.hasSuffix(" --completion-hook cursor") == true
    }

    public static func configure(_ source: Source, enabled: Bool, executable: URL,
                                 home: URL = FileManager.default.homeDirectoryForCurrentUser) throws {
        var object = try configuration(source, home: home)
        let original = object
        let quoted = "'" + executable.path.replacingOccurrences(of: "'", with: "'\\''") + "'"
        let command = ProviderJSON.string(quoted + " --completion-hook " + source.rawValue)
        if source == .antigravity {
            object["agent-hud"] = enabled ? .object(["Stop": .array([.object([
                "type": .string("command"), "command": command, "timeout": .integer(5)
            ])])]) : nil
        } else {
            guard object["version"] == nil || object["version"] == .integer(1),
                  object["hooks"] == nil || object["hooks"]?.objectValue != nil else { throw ProviderFailure.format }
            var hooks = object["hooks"]?.objectValue ?? [:]
            guard hooks["stop"] == nil || hooks["stop"]?.arrayValue != nil else { throw ProviderFailure.format }
            var handlers = (hooks["stop"]?.arrayValue ?? []).filter { !ownsCursorHandler($0) }
            if enabled { handlers.append(.object(["command": command, "timeout": .integer(5)])) }
            hooks["stop"] = handlers.isEmpty ? nil : .array(handlers)
            object["hooks"] = .object(hooks); object["version"] = .integer(1)
        }
        guard object != original else { return }
        let url = source.configuration(home: home)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(ProviderJSON.object(object)).write(to: url, options: .atomic)
    }
}
