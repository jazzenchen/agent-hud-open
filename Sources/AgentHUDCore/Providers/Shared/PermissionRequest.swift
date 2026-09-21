import AgentHUDSupport
import Foundation

/// One tool call a client is about to ask its user about.
///
/// The client is waiting on the hook that carried this request, so the request lives only as long as that hook does:
/// answering it resumes the client, and the client withdrawing it takes it off the HUD. Nothing here is stored.
public struct PermissionRequest: Identifiable, Equatable, Sendable {
    public let id: String
    public let source: PermissionHooks.Source
    /// Whose request this is, as the HUD names clients elsewhere. It comes from the hook's own source today and
    /// from the channel itself once a client answers without one.
    public let vendor: String
    public let sessionID: String
    public let toolName: String?
    /// The one line worth reading first: what this call does to what.
    public let summary: String
    /// The full subject of the call — a command, a path, a URL — when it is longer than the summary.
    public let detail: String?
    public let cwd: String?
    /// The file this call is about, when it is about one; the summary names it, this locates it.
    public let path: String?
    /// The two sides of an edit, already trimmed to what is worth reading before approving it.
    public let removed: String?
    public let added: String?
    /// What the client itself offers to change if this is allowed — the same entries its own dialog builds its
    /// options from. They are echoed back untouched rather than assembled here: the client knows what rule fits its
    /// own request, and a rule invented from the outside is the kind that silently never matches.
    public let suggestions: [JSONValue]
    public let at: Date

    /// The folder the session runs in, which is how a user tells two sessions apart.
    public var project: String? {
        guard let cwd, !cwd.isEmpty else { return nil }
        let name = (cwd as NSString).lastPathComponent
        return name.isEmpty ? nil : name
    }

    public init(id: String, source: PermissionHooks.Source, vendor: String? = nil, sessionID: String, toolName: String?,
                summary: String, detail: String?, cwd: String?, path: String? = nil,
                removed: String? = nil, added: String? = nil, suggestions: [JSONValue] = [], at: Date) {
        self.id = id
        self.source = source
        self.vendor = vendor ?? source.vendor
        self.sessionID = sessionID
        self.toolName = toolName
        self.summary = summary
        self.detail = detail
        self.cwd = cwd
        self.path = path
        self.removed = removed
        self.added = added
        self.suggestions = suggestions
        self.at = at
    }

    /// What kind of call this is, in one word, so a list of them can be read down the left edge.
    public var badge: String {
        guard let toolName, !toolName.isEmpty else { return "TOOL" }
        if toolName.hasPrefix("mcp__") { return "MCP" }
        switch Self.kind(toolName) {
        case "Bash", "BashOutput", "KillShell": return "BASH"
        case "Edit", "MultiEdit", "Write", "NotebookEdit", "apply_patch": return "EDIT"
        case "Read": return "READ"
        case "Glob", "Grep": return "FIND"
        case "WebFetch", "WebSearch": return "WEB"
        case "Task", "Agent": return "AGENT"
        default: return String(toolName.prefix(6)).uppercased()
        }
    }

    /// The mark a call is recognized by before its name is read. Only what the client's own tools are; an unknown
    /// tool gets the neutral one rather than a guess.
    public var symbol: String {
        guard let toolName, !toolName.isEmpty else { return "questionmark.circle" }
        if toolName.hasPrefix("mcp__") { return "puzzlepiece.extension.fill" }
        switch Self.kind(toolName) {
        case "Bash", "BashOutput", "KillShell": return "terminal.fill"
        case "Edit", "MultiEdit", "NotebookEdit", "apply_patch": return "pencil"
        case "Write": return "square.and.pencil"
        case "Read": return "doc.text.fill"
        case "Glob", "Grep": return "magnifyingglass"
        case "WebFetch", "WebSearch": return "globe"
        case "Task", "Agent": return "sparkles"
        default: return "wrench.and.screwdriver.fill"
        }
    }

    /// Where the call lands: the file it touches, else the folder the session runs in. Written the way a shell
    /// prompt would, so it is recognized rather than read.
    public var context: String? {
        guard let raw = path ?? cwd, !raw.isEmpty else { return nil }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return raw.hasPrefix(home) ? "~" + raw.dropFirst(home.count) : raw
    }

    /// The offer worth a button of its own: stop asking about calls like this one. Only a rule is taken — an entry
    /// that changes the whole permission mode is a different decision than the one being made here, and one that
    /// quietly does nothing unless the session was started to allow it.
    public var alwaysAllow: JSONValue? {
        guard source.supportsPermissionUpdates else { return nil }
        return suggestions.first {
            $0["type"].stringValue == "addRules" && $0["behavior"].stringValue == "allow"
                && ($0["rules"].arrayValue?.isEmpty == false)
        }
    }

    static let detailLength = 2048
    static let fileTools: Set<String> = ["Read", "Write", "Edit", "MultiEdit", "NotebookEdit"]

    /// Qwen Code's names for the tools Claude Code has, whose inputs use the same keys; a call reads the same
    /// whichever client makes it.
    static let aliases = ["run_shell_command": "Bash", "edit": "Edit", "replace": "Edit", "write_file": "Write",
                          "read_file": "Read", "glob": "Glob", "grep_search": "Grep", "search_file_content": "Grep",
                          "web_fetch": "WebFetch", "web_search": "WebSearch", "agent": "Agent", "task": "Task"]
    static func kind(_ tool: String) -> String { aliases[tool] ?? tool }

    /// Reads a client's hook payload. A payload without a session cannot be shown next to the session it belongs to,
    /// and is refused rather than guessed at; so is a call that asks the user something other than permission.
    static func parse(_ data: Data, source: PermissionHooks.Source, id: String, now: Date) throws -> PermissionRequest? {
        guard data.count <= 1024 * 1024 else { throw ProviderFailure.limit }
        let payload = try ProviderJSON.read(data)
        guard let session = payload["session_id"].stringValue, !session.isEmpty else { return nil }
        let tool = payload["tool_name"].stringValue
        guard !source.unanswerableTools.contains(tool ?? "") else { return nil }
        let input = payload["tool_input"]
        let kind = tool.map(kind)
        let file = fileTools.contains(kind ?? "") ? trimmed(input["file_path"]) : nil
        // A multi-edit is recognized by its first change, the same way a single edit is.
        let change = kind == "MultiEdit" ? input["edits"].arrayValue?.first ?? .null : input
        return PermissionRequest(
            id: id, source: source, sessionID: session, toolName: tool,
            summary: summary(tool: kind, input: input), detail: detail(tool: kind, input: input),
            cwd: payload["cwd"].stringValue, path: file,
            removed: lines(change["old_string"]), added: lines(change["new_string"] ?? change["content"]),
            suggestions: payload["permission_suggestions"].arrayValue ?? [], at: now
        )
    }

    /// What the call is about, in the words the tool itself uses. Known hook tool names are matched; anything
    /// else falls back to the tool's name, which is all a request for an unknown tool can honestly say.
    static func summary(tool: String?, input: ProviderJSON) -> String {
        guard let tool, !tool.isEmpty else { return L10n.text("工具调用", "Tool call") }
        switch tool {
        case "Bash", "BashOutput", "KillShell":
            return trimmed(input["description"]) ?? trimmed(input["command"]) ?? tool
        case "apply_patch":
            return trimmed(input["description"]) ?? L10n.text("应用文件修改", "Apply file changes")
        case "Read", "Write", "Edit", "MultiEdit", "NotebookEdit":
            return trimmed(input["file_path"]).map { ($0 as NSString).lastPathComponent } ?? tool
        case "Glob", "Grep":
            return trimmed(input["pattern"]) ?? tool
        case "WebFetch":
            return trimmed(input["url"]) ?? tool
        case "WebSearch":
            return trimmed(input["query"]) ?? tool
        case "Task", "Agent":
            return trimmed(input["description"]) ?? tool
        default:
            // An MCP tool is named mcp__<server>__<tool>; the last two parts are what the user recognizes.
            guard tool.hasPrefix("mcp__") else { return tool }
            let parts = tool.dropFirst(5).components(separatedBy: "__")
            return parts.count >= 2 ? "\(parts[0]) · \(parts.dropFirst().joined(separator: "__"))" : tool
        }
    }

    /// The exact subject of the call, shown under the summary when it adds something the summary left out.
    static func detail(tool: String?, input: ProviderJSON) -> String? {
        let value: String?
        switch tool {
        case "Bash", "apply_patch": value = trimmed(input["command"])
        case "Read", "Write", "Edit", "MultiEdit", "NotebookEdit": value = trimmed(input["file_path"])
        case "WebFetch": value = trimmed(input["url"])
        default: value = nil
        }
        guard let value, value != summary(tool: tool, input: input) else { return nil }
        return String(value.prefix(detailLength))
    }

    /// One side of an edit, capped at what fits on a card: the point is to recognize the change, not to review it.
    static let diffLines = 6
    static func lines(_ value: ProviderJSON) -> String? {
        guard let text = value.stringValue, !text.isEmpty else { return nil }
        let kept = text.split(separator: "\n", omittingEmptySubsequences: false).prefix(diffLines)
        return kept.joined(separator: "\n")
    }

    private static func trimmed(_ value: ProviderJSON) -> String? {
        guard let text = value.stringValue?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else { return nil }
        return String(text.prefix(detailLength))
    }
}

/// What the user decided, in the shape the client reads it in.
///
/// The allow/deny answer is shared; permission-rule updates are available only on supporting clients.
/// Saying nothing is its own answer:
/// the client then does what it would have done without the hook, which is to ask in its own terminal.
public enum PermissionDecision: Sendable, Equatable {
    case allow
    case deny
    /// Allow, and apply what the client offered so it stops asking about calls like this one.
    case allowAlways(JSONValue)

    var behavior: String {
        if case .deny = self { return "deny" }
        return "allow"
    }

    public func response(for source: PermissionHooks.Source) -> Data {
        var decision: [String: JSONValue] = ["behavior": .string(behavior)]
        if case .allowAlways(let update) = self {
            // Codex rejects updatedPermissions. Leave unsupported decisions to the client's own approval flow.
            guard source.supportsPermissionUpdates else { return Self.noDecision }
            decision["updatedPermissions"] = .array([update])
        }
        let payload: [String: JSONValue] = [
            "hookSpecificOutput": .object([
                "hookEventName": .string("PermissionRequest"),
                "decision": .object(decision),
            ])
        ]
        return (try? RecordCoding.encoder().encode(JSONValue.object(payload))) ?? Data()
    }

    /// No opinion. An empty answer leaves the permission flow exactly as it was, which is what a HUD that is closed,
    /// busy or switched off must look like.
    public static let noDecision = Data()
}

public extension PermissionRequest {
    /// The requests the demo shows: four clients caught mid-task, the way a working morning actually looks — one
    /// edit worth reading, one build, one ticket and one file. No client is waiting behind them, so answering one
    /// only takes it off the HUD.
    static func demo(now: Date = Date()) -> [PermissionRequest] {
        [
            PermissionRequest(
                id: "demo-edit", source: .claude, sessionID: "demo-1", toolName: "Edit",
                summary: "Pricing.jsx", detail: nil, cwd: "~/Development/agent-hud-web",
                path: "~/Development/agent-hud-web/src/Pricing.jsx",
                removed: "  { name: 'Pro', price: 20 },", added: "  { name: 'Pro', price: 24 },",
                suggestions: [.object([
                    "type": .string("addRules"), "behavior": .string("allow"),
                    "destination": .string("localSettings"),
                    "rules": .array([.object(["toolName": .string("Edit"), "ruleContent": .string("src/**")])]),
                ])],
                at: now.addingTimeInterval(-38)),
            PermissionRequest(
                id: "demo-build", source: .codex, sessionID: "demo-2", toolName: "Bash",
                summary: L10n.text("打包鸿蒙版本", "Package the HarmonyOS build"), detail: "hvigorw assembleHap",
                cwd: "~/Development/agent-hud-harmony", at: now.addingTimeInterval(-124)),
            PermissionRequest(
                id: "demo-ticket", source: .claude, vendor: "Pi", sessionID: "demo-3",
                toolName: "mcp__linear__create_issue", summary: "linear · create_issue", detail: nil,
                cwd: "~/Development/agent-hud-ios", at: now.addingTimeInterval(-71)),
            PermissionRequest(
                id: "demo-read", source: .claude, sessionID: "demo-4", toolName: "Read",
                summary: ".env.production", detail: nil, cwd: "~/Development/agent-hud-web",
                path: "~/Development/agent-hud-web/.env.production", at: now.addingTimeInterval(-9)),
        ]
    }
}
