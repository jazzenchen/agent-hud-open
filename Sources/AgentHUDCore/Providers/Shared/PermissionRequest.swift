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
    /// What the call asks, when it is a question put to the user rather than a tool waiting to run. A question is
    /// answered, not allowed: the answers travel back inside the call's own input, which is kept for that.
    public let questions: [PermissionQuestion]
    let questionInput: JSONValue?
    public let at: Date

    public var isQuestion: Bool { !questions.isEmpty }
    /// A plan Claude Code wants approved before it starts changing anything. The HUD points to Claude Code for it
    /// rather than answering: a plan is read and approved there, with the choices only Claude Code offers.
    public var isPlan: Bool { toolName.map(Self.kind) == Self.planTool }
    static let planTool = "ExitPlanMode"

    /// The folder the session runs in, which is how a user tells two sessions apart.
    public var project: String? {
        guard let cwd, !cwd.isEmpty else { return nil }
        let name = (cwd as NSString).lastPathComponent
        return name.isEmpty ? nil : name
    }

    public init(id: String, source: PermissionHooks.Source, vendor: String? = nil, sessionID: String, toolName: String?,
                summary: String, detail: String?, cwd: String?, path: String? = nil,
                removed: String? = nil, added: String? = nil, suggestions: [JSONValue] = [],
                questions: [PermissionQuestion] = [], questionInput: JSONValue? = nil, at: Date) {
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
        self.questions = questions
        self.questionInput = questionInput
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
        case "AskUserQuestion": return "ASK"
        case "ExitPlanMode": return "PLAN"
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
        case "AskUserQuestion": return "questionmark.bubble.fill"
        case "ExitPlanMode": return "list.bullet.clipboard.fill"
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
    /// and is refused rather than guessed at; so is a call that asks the user something the HUD cannot answer, and a
    /// question it cannot read in full.
    static func parse(_ data: Data, source: PermissionHooks.Source, id: String, now: Date) throws -> PermissionRequest? {
        guard data.count <= 1024 * 1024 else { throw ProviderFailure.limit }
        let payload = try ProviderJSON.read(data)
        guard let session = payload["session_id"].stringValue, !session.isEmpty else { return nil }
        let tool = payload["tool_name"].stringValue
        guard !source.unanswerableTools.contains(tool ?? "") else { return nil }
        let input = payload["tool_input"]
        let kind = tool.map(kind)
        let questions = kind == PermissionQuestion.tool ? PermissionQuestion.read(input["questions"]) : []
        guard kind != PermissionQuestion.tool || !questions.isEmpty else { return nil }
        let file = fileTools.contains(kind ?? "") ? trimmed(input["file_path"]) : nil
        // A multi-edit is recognized by its first change, the same way a single edit is.
        let change = kind == "MultiEdit" ? input["edits"].arrayValue?.first ?? .null : input
        return PermissionRequest(
            id: id, source: source, sessionID: session, toolName: tool,
            summary: questions.first?.question ?? summary(tool: kind, input: input), detail: detail(tool: kind, input: input),
            cwd: payload["cwd"].stringValue, path: file,
            removed: lines(change["old_string"]), added: lines(change["new_string"] ?? change["content"]),
            suggestions: payload["permission_suggestions"].arrayValue ?? [],
            questions: questions, questionInput: questions.isEmpty ? nil : input, at: now
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
        case planTool:
            // A plan opens with its title; the heading marks are not part of it.
            let title = input["plan"].stringValue?.split(separator: "\n").lazy
                .map { $0.drop { $0 == "#" || $0 == " " }.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty }
            return title.map { String($0.prefix(detailLength)) } ?? L10n.text("计划", "Plan")
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
        case planTool: value = trimmed(input["plan"])
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

/// One question a client asks its user, with the answers it offers. The user can always answer in their own words
/// instead, the way the client's own dialog lets them.
public struct PermissionQuestion: Equatable, Sendable {
    public struct Option: Equatable, Sendable {
        public let label: String
        public let description: String?

        public init(label: String, description: String? = nil) {
            self.label = label
            self.description = description
        }
    }

    /// The question in full. It is also what its answer is filed under, so it is kept exactly as the client wrote it.
    public let question: String
    /// A word or two naming the question.
    public let header: String?
    public let options: [Option]
    public let multiSelect: Bool

    public init(question: String, header: String? = nil, options: [Option], multiSelect: Bool = false) {
        self.question = question
        self.header = header
        self.options = options
        self.multiSelect = multiSelect
    }

    /// The tool Claude Code asks its questions with.
    static let tool = "AskUserQuestion"

    /// Reads the questions a call asks. Answers go back as a set, so a list with one question the HUD cannot read is
    /// left whole to the client's own dialog rather than answered in part.
    static func read(_ value: JSONValue) -> [PermissionQuestion] {
        guard let list = value.arrayValue, !list.isEmpty else { return [] }
        let questions = list.compactMap { item -> PermissionQuestion? in
            guard let text = item["question"].stringValue,
                  !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
            let options = (item["options"].arrayValue ?? []).compactMap { option -> Option? in
                guard let label = option["label"].stringValue, !label.isEmpty else { return nil }
                return Option(label: label, description: option["description"].stringValue.flatMap { $0.isEmpty ? nil : $0 })
            }
            guard !options.isEmpty else { return nil }
            return PermissionQuestion(question: text, header: item["header"].stringValue.flatMap { $0.isEmpty ? nil : $0 },
                                      options: options, multiSelect: item["multiSelect"].boolValue ?? false)
        }
        return questions.count == list.count ? questions : []
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
    /// Answer a question: each answer filed under the question it answers — an offered option's label, several
    /// joined with a comma, or the user's own words. A question skipped has no entry.
    case answer([String: String])
    /// Say nothing: the request leaves the HUD and the client carries on with its own dialog, as though the HUD had
    /// never been asked.
    case leave

    var behavior: String {
        if case .deny = self { return "deny" }
        return "allow"
    }

    public func response(for source: PermissionHooks.Source) -> Data {
        var decision: [String: JSONValue] = ["behavior": .string(behavior)]
        switch self {
        case .allowAlways(let update):
            // Codex rejects updatedPermissions. Leave unsupported decisions to the client's own approval flow.
            guard source.supportsPermissionUpdates else { return Self.noDecision }
            decision["updatedPermissions"] = .array([update])
        case .answer, .leave:
            // Answers travel inside the question they answer; without it there is nothing to send.
            return Self.noDecision
        case .allow, .deny:
            break
        }
        return Self.encode(decision)
    }

    /// The answer for this request. A question's answers go back as the call's own input with the answers filled
    /// in, which is how Claude Code hears what its user chose; a question left out is one the user skipped, and with
    /// none answered Claude Code hears that its questions went unanswered.
    public func response(for request: PermissionRequest) -> Data {
        guard case .answer(let answers) = self else { return response(for: request.source) }
        guard var input = request.questionInput?.objectValue else { return Self.noDecision }
        input["answers"] = .object(answers.mapValues { .string($0) })
        return Self.encode(["behavior": .string("allow"), "updatedInput": .object(input)])
    }

    private static func encode(_ decision: [String: JSONValue]) -> Data {
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
