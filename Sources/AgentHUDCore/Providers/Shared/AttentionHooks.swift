import AgentHUDSupport
import Foundation

/// A client saying it needs the user. Claude Code's notification hook is installed for the types that mean exactly
/// that; which kind of attention it is still comes from the transcript, never from the wording of a message, and the
/// transcript is also what says the request has been answered.
public enum AttentionHooks {
    public enum Source: String, CaseIterable, Sendable {
        case claude
        var event: String { "Notification" }
        /// The notification types worth waking for. Claude Code filters on the type itself, so nothing here depends on
        /// the wording of a message, and a sign-in or quota notice never looks like a request for the user.
        var matcher: String { "permission_prompt|agent_needs_input" }
        func configuration(home: URL) -> URL { home.appendingPathComponent(".claude/settings.json") }

        /// Whether the client is here at all. A machine without it keeps its home untouched.
        func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser, fileManager: FileManager = .default) -> Bool {
            switch self {
            case .claude:
                return ClaudeEngineLocator.find(home: home, fileManager: fileManager) != nil
                    || fileManager.fileExists(atPath: home.appendingPathComponent(".claude/projects").path)
            }
        }
    }

    /// The last thing a client asked for in one session.
    public struct Event: Codable, Equatable, Sendable {
        public let sessionID: String
        /// What the client said it is waiting for; shown as the session's last message while it waits.
        public let message: String?
        public let at: Date

        public init(sessionID: String, message: String?, at: Date) {
            self.sessionID = sessionID; self.message = message; self.at = at
        }
    }

    public static var directory: URL { AppSupport.directory.appendingPathComponent("attention") }
    /// A request nobody answered is forgotten after this long.
    static let retention: TimeInterval = 86400
    static let messageLength = 2048

    static func event(_ payload: ProviderJSON, now: Date) -> Event? {
        guard let session = payload["session_id"].stringValue, !session.isEmpty else { return nil }
        let message = payload["message"].stringValue?.trimmingCharacters(in: .whitespacesAndNewlines)
        return Event(sessionID: session, message: message.flatMap { $0.isEmpty ? nil : String($0.prefix(messageLength)) }, at: now)
    }

    /// One file per session: only the latest request matters, and answering it is seen in the transcript, not here.
    public static func record(source: Source, data: Data, now: Date = Date(), directory: URL = directory) throws {
        guard data.count <= 1024 * 1024 else { throw ProviderFailure.limit }
        guard let event = event(try ProviderJSON.read(data), now: now) else { return }
        let folder = directory.appendingPathComponent(source.rawValue)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let name = RecordCoding.hash([event.sessionID]) + ".json"
        let file = folder.appendingPathComponent(name)
        try JSONEncoder().encode(event).write(to: file, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
        // Requests age by when they were made, which is inside them; a file's own timestamps say nothing about that.
        for file in try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
        where file.pathExtension == "json" && file != file.deletingLastPathComponent().appendingPathComponent(name) {
            let stored = (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode(Event.self, from: $0) }
            if stored.map({ $0.at <= now.addingTimeInterval(-retention) }) ?? true { try? FileManager.default.removeItem(at: file) }
        }
    }

    /// The requests still worth showing, by session.
    public static func read(source: Source, now: Date = Date(), directory: URL = directory) -> [String: Event] {
        let folder = directory.appendingPathComponent(source.rawValue)
        guard let files = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil) else { return [:] }
        var result: [String: Event] = [:]
        for file in files where file.pathExtension == "json" {
            guard let data = try? Data(contentsOf: file), data.count <= 1024 * 1024,
                  let event = try? JSONDecoder().decode(Event.self, from: data),
                  event.at > now.addingTimeInterval(-retention), event.at <= now.addingTimeInterval(60) else { continue }
            if let existing = result[event.sessionID], existing.at >= event.at { continue }
            result[event.sessionID] = event
        }
        return result
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
        command?.hasSuffix(" --attention-hook " + source.rawValue) == true
    }

    public static func isActive(_ source: Source, home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        guard let object = try? configuration(source, home: home) else { return false }
        return !commands(in: object, source: source).isEmpty
    }

    static func commands(in configuration: [String: ProviderJSON], source: Source) -> [String] {
        (configuration["hooks"]?[source.event].arrayValue ?? []).flatMap { $0["hooks"].arrayValue ?? [] }
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
        let command = quoted + " --attention-hook " + source.rawValue
        if !replacingExisting && commands(in: object, source: source).contains(where: { $0 != command }) {
            throw UsageProviderError(L10n.text("通知回调由另一安装管理，请手动重新安装以切换",
                                               "The notification hook belongs to another installation; reinstall it explicitly to switch"))
        }
        let updated = try updating(object, source: source, command: enabled ? command : nil)
        guard updated != object else { return }
        let url = source.configuration(home: home)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        // The inbox exists from the moment the hook does, so its changes can be watched before the first request.
        try? FileManager.default.createDirectory(at: directory.appendingPathComponent(source.rawValue),
                                                 withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(ProviderJSON.object(updated)).write(to: url, options: .atomic)
    }

    static func updating(_ configuration: [String: ProviderJSON], source: Source, command: String?) throws -> [String: ProviderJSON] {
        var object = configuration
        guard object["hooks"] == nil || object["hooks"]?.objectValue != nil else { throw ProviderFailure.format }
        var hooks = object["hooks"]?.objectValue ?? [:]
        guard hooks[source.event] == nil || hooks[source.event]?.arrayValue != nil else { throw ProviderFailure.format }
        var groups = (hooks[source.event]?.arrayValue ?? []).compactMap { group -> ProviderJSON? in
            guard var fields = group.objectValue, let handlers = fields["hooks"]?.arrayValue else { return group }
            let kept = handlers.filter { !ownsCommand($0["command"].stringValue, source: source) }
            if kept.count == handlers.count { return group }
            if kept.isEmpty { return nil }
            fields["hooks"] = .array(kept)
            return .object(fields)
        }
        if let command {
            groups.append(.object(["matcher": .string(source.matcher),
                                   "hooks": .array([.object(["type": .string("command"), "command": .string(command), "timeout": .integer(5)])])]))
        }
        hooks[source.event] = groups.isEmpty ? nil : .array(groups)
        object["hooks"] = hooks.isEmpty && configuration["hooks"] == nil ? nil : .object(hooks)
        return object
    }
}
