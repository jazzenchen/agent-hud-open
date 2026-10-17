import AgentHUDSupport
import Foundation

public enum OpenAgentSource: String, CaseIterable, Sendable {
    case opencode, kimi, glm, pi
    public var name: String {
        switch self { case .opencode: "OpenCode"; case .kimi: "Kimi"; case .glm: "GLM"; case .pi: "Pi" }
    }
    public var detail: String {
        self == .glm ? L10n.text("国内 / 国际 Coding Plan，按计费池去重", "China / Global Coding Plan, grouped by billing pool")
            : L10n.text("本地会话与用量；共享供应商额度只计一次", "Local sessions and usage; shared provider quota counted once")
    }
    public func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                            environment: [String: String] = ProcessInfo.processInfo.environment) -> Bool {
        // A plan shown in another client's group, such as one only WorkBuddy carries, does not make GLM detected.
        if self == .glm { return OpenAgentCredentials.discover(home: home, environment: environment).contains { $0.displayGroup == "GLM" } }
        return OpenAgentPaths(home: home, environment: environment).roots(for: self).contains { FileManager.default.fileExists(atPath: $0.path) }
    }
}

struct OpenAgentPaths: Sendable {
    let home: URL
    let environment: [String: String]
    var openCode: URL { URL(fileURLWithPath: ClientHome.variable("XDG_DATA_HOME", in: environment) ?? home.appendingPathComponent(".local/share").path).appendingPathComponent("opencode") }
    var openCodeTurns: URL { openCode.appendingPathComponent("agent-hud/turns") }
    var openCodeConfig: URL { URL(fileURLWithPath: ClientHome.variable("XDG_CONFIG_HOME", in: environment) ?? home.appendingPathComponent(".config").path).appendingPathComponent("opencode") }
    var pi: URL { URL(fileURLWithPath: ClientHome.variable("PI_CODING_AGENT_DIR", in: environment) ?? home.appendingPathComponent(".pi/agent").path) }
    var piTurns: URL { pi.appendingPathComponent("agent-hud/turns") }
    var kimi: URL { URL(fileURLWithPath: ClientHome.variable("KIMI_CODE_HOME", in: environment) ?? home.appendingPathComponent(".kimi-code").path) }
    func roots(for source: OpenAgentSource) -> [URL] {
        switch source {
        case .opencode: [openCode]
        case .pi: [pi.appendingPathComponent("sessions")]
        case .kimi: [kimi.appendingPathComponent("sessions"), home.appendingPathComponent(".kimi/sessions")]
        case .glm: []
        }
    }
}

struct OpenAgentSession: Sendable {
    var id: String
    var client: OpenAgentSource
    var title: String
    var titleSource = TitleSource.log
    var workspace: String?
    var path: String
    var events: [UsageEvent] = []
    var models: [String: String] = [:]
    var currentModel: (id: String, name: String, provider: String)?
    var start: Date?
    var end: Date?
    var turns: [SessionTurn] = []
    var completions: [SessionCompletion] = []
    /// A missing target in the latest observer reading clears an older terminal destination.
    var navigation: NavigationObservation?
    /// Whether the client archived the session; only OpenCode's database says so (`time_archived`). Its usage counts, and
    /// it is left out of the session list.
    var archived = false

    struct NavigationObservation: Sendable {
        let observedAt: Date
        let target: SessionNavigationTarget?
    }

    /// Which copy's title wins when copies of one session merge: a name or first prompt from the session's own log,
    /// then the name Pi's observer saw when a turn settled, then a name standing in for a missing title.
    enum TitleSource: Comparable, Sendable { case placeholder, observer, log }

    /// A consumer is the model as the log names it and, after `#`, the provider the calls went through: routes to one
    /// model stay apart, and the price catalog prices only the vendor's own.
    mutating func setModel(_ model: String, provider: String) {
        let id = "\(client.rawValue)-model:\(model)#\(provider)"
        models[id] = model
        currentModel = (id, model, provider)
    }

    /// - input, output: including `cacheWrite` and `reasoning`.
    mutating func add(id eventID: String, model: String, provider: String, at: Date, input: Int, output: Int,
                      cacheRead: Int, cacheWrite: Int = 0, reasoning: Int = 0) throws {
        _ = try TokenCount.sum(input, output, cacheRead)
        setModel(model, provider: provider)
        let consumer = currentModel!.id
        events.append(.init(timestamp: at, agentId: consumer, tokensIn: input, tokensOut: output,
            cacheReadTokens: cacheRead, cacheWriteTokens: cacheWrite, reasoningTokens: reasoning, eventID: eventID,
            attribution: .init(client: client.name, providerID: provider)))
        start = min(start ?? at, at); end = max(end ?? at, at)
    }
}

/// Provider licenses are listed in THIRD_PARTY_NOTICES.txt.
/// Only metadata, counters and session titles (a name, or the first line of the first prompt) leave these parsers;
/// prompts, tool bodies and credentials do not.
enum OpenAgentParser {
    static func jsonLines(_ data: Data, visit: (ProviderJSON, Int) throws -> Void) throws {
        guard data.count <= 64 * 1024 * 1024 else { throw ProviderFailure.limit }
        let lines = data.split(separator: 10, omittingEmptySubsequences: false)
        for (index, line) in lines.enumerated() where !line.isEmpty {
            try Task.checkCancellation()
            guard let value = try? ProviderJSON.read(Data(line)) else {
                // Writers append the last line concurrently. Interior corruption must be surfaced.
                if index == lines.count - 1 && data.last != 10 { continue }
                throw ProviderFailure.format
            }
            try visit(value, index)
        }
    }

    static func pi(_ data: Data, path: String) throws -> [OpenAgentSession] {
        var session: OpenAgentSession?
        // Pi's own session list: the name last given with `/name` (an empty one clears it), else the first message.
        var name: String?, prompt: String?
        try jsonLines(data) { line, index in
            let type = line["type"].stringValue
            if type == "session", let id = line["id"].stringValue {
                session = .init(id: "pi:\(id)", client: .pi, title: "Pi", workspace: line["cwd"].stringValue, path: path,
                                start: DateParsing.internet(line["timestamp"].stringValue))
                name = nil; prompt = nil
                return
            }
            guard session != nil else { return }
            if type == "session_info" { name = SessionTitle.named(line["name"].stringValue); return }
            if type == "model_change", let model = line["modelId"].stringValue, let provider = line["provider"].stringValue {
                session?.setModel(model, provider: provider)
                return
            }
            let message = line["message"]
            if prompt == nil, type == "message", message["role"].stringValue == "user" {
                let content = message["content"]
                let text = content.stringValue ?? content.arrayValue?.first { $0["type"].stringValue == "text" }?["text"].stringValue
                prompt = text.flatMap(SessionTitle.from)
                return
            }
            guard type == "message", message["role"].stringValue == "assistant", message["usage"].objectValue != nil else { return }
            guard let at = DateParsing.internet(line["timestamp"].stringValue) ?? ProviderDate.milliseconds(message["timestamp"]) else { throw ProviderFailure.format }
            let usage = message["usage"]
            let input = try usage["input"].optionalCounter(), output = try usage["output"].optionalCounter()
            let read = try usage["cacheRead"].optionalCounter(), write = try usage["cacheWrite"].optionalCounter()
            let model = message["model"].stringValue ?? "Unknown", provider = message["provider"].stringValue ?? "Unknown"
            // Pi entry IDs are short. Retain timestamp/provider/model to distinguish unrelated collisions,
            // while forks retaining the original entries collapse to the original request.
            let identity: String
            if let response = message["responseId"].stringValue, !response.isEmpty {
                identity = "pi:response:" + RecordCoding.hash([provider, response])
            } else if let entry = line["id"].stringValue {
                identity = "pi:entry:" + RecordCoding.hash([entry, String(RecordCoding.milliseconds(at)), provider, model])
            } else { identity = "\(session!.id):line:\(index)" }
            try session?.add(id: identity, model: model, provider: provider, at: at, input: try TokenCount.sum(input, write),
                         output: output, cacheRead: read, cacheWrite: write)
            // An assistant stop is not agent_settled; retries, tools and queued followups can still run.
        }
        if let title = name ?? prompt { session?.title = title } else { session?.titleSource = .placeholder }
        return session.map { [$0] } ?? []
    }

    /// The session folder of a Kimi wire log (Kimi Code's `<session>/agents/<agent>/wire.jsonl`, or kimi-cli's
    /// `<session>/wire.jsonl`) and the agent that wrote it.
    static func kimiSession(_ wire: URL) -> (folder: URL, agent: String, modern: Bool) {
        let directory = wire.deletingLastPathComponent()
        guard directory.deletingLastPathComponent().lastPathComponent == "agents" else { return (directory, "main", false) }
        return (directory.deletingLastPathComponent().deletingLastPathComponent(), directory.lastPathComponent, true)
    }

    /// The title in the session's `state.json`: Kimi Code's first prompt until a generated title or a `/title` replaces
    /// it, or the `/title` kimi-cli kept as `custom_title`.
    static func kimiTitle(_ folder: URL) -> String? {
        let state = (try? ProviderFiles.json(folder.appendingPathComponent("state.json"))) ?? .null
        return (SessionTitle.named(state["title"].stringValue) ?? SessionTitle.named(state["custom_title"].stringValue))
            .flatMap { $0 == "New Session" ? nil : $0 }
    }

    static func kimi(_ data: Data, path: String) throws -> [OpenAgentSession] {
        let (folder, agent, modern) = kimiSession(URL(fileURLWithPath: path))
        // Sub-agents keep the client's name; the title belongs to the conversation the main agent holds.
        let title = agent == "main" ? kimiTitle(folder) : nil
        var session = OpenAgentSession(id: "kimi:\(folder.lastPathComponent):\(agent)", client: .kimi, title: title ?? "Kimi",
                                       titleSource: title == nil ? .placeholder : .log, path: path)
        var requestModel: String?, keyed: [String: Int] = [:]
        func concrete(_ name: String?) -> String? {
            guard let name, !name.isEmpty, !name.hasPrefix("__") else { return nil }; return name
        }
        try jsonLines(data) { line, index in
            let type = line["type"].stringValue
            if modern {
                if agent == "main", type == "context.append_loop_event",
                   let turnID = line["event"]["turnId"].stringValue, let at = ProviderDate.milliseconds(line["time"]) {
                    if let position = session.turns.firstIndex(where: { $0.turnID == turnID }) {
                        let previous = session.turns[position]
                        if previous.state == .running, RecordCoding.milliseconds(at) > previous.observedAtMs {
                            session.turns[position] = .init(provider: "Kimi", sessionID: session.id, turnID: turnID,
                                state: .running, startedAtMs: previous.startedAtMs, observedAtMs: RecordCoding.milliseconds(at))
                        }
                    } else if line["event"]["type"].stringValue == "step.begin" {
                        session.turns.append(.init(provider: "Kimi", sessionID: session.id, turnID: turnID,
                            state: .running, startedAtMs: RecordCoding.milliseconds(at), observedAtMs: RecordCoding.milliseconds(at)))
                    }
                    session.start = min(session.start ?? at, at); session.end = max(session.end ?? at, at)
                    return
                }
                if type == "llm.request" { requestModel = concrete(line["model"].stringValue); return }
                if type == "turn.ended", let turn = line["turnId"].countValue, let at = ProviderDate.milliseconds(line["time"]) {
                    let failed = line["error"] != .null
                    let success = line["reason"].stringValue == "completed" && !failed
                    let position = session.turns.firstIndex { $0.turnID == String(turn) }
                    let start = position.flatMap { session.turns[$0].startedAtMs }
                    if agent == "main" {
                        // An error the turn carries is a failure and a cancelled turn the user's stop; any other end that is
                        // not a completion has no reason.
                        let finished = SessionTurn(provider: "Kimi", sessionID: session.id, turnID: String(turn),
                            state: success ? .completed : .ended, startedAtMs: start, observedAtMs: RecordCoding.milliseconds(at),
                            outcome: success ? .completed : failed ? .failed : line["reason"].stringValue == "cancelled" ? .interrupted : nil)
                        if let position { session.turns[position] = finished } else { session.turns.append(finished) }
                    }
                    session.end = max(session.end ?? at, at)
                    // Subagent ends cannot complete the parent conversation.
                    if success && agent == "main" {
                        session.completions.append(.init(sessionID: session.id, vendor: "Kimi", turnID: String(turn),
                            task: session.title, model: requestModel ?? "Unknown", startedAt: start.map(RecordCoding.date), completedAt: at))
                    }
                    return
                }
                guard type == "usage.record", line["usageScope"].stringValue == "turn" else { return }
                guard let at = ProviderDate.milliseconds(line["time"]) else { throw ProviderFailure.format }
                let usage = line["usage"]
                let input = try usage["inputOther"].optionalCounter(), read = try usage["inputCacheRead"].optionalCounter()
                let write = try usage["inputCacheCreation"].optionalCounter(), output = try usage["output"].optionalCounter()
                try session.add(id: "\(session.id):usage:\(index)", model: concrete(line["model"].stringValue) ?? requestModel ?? "Unknown",
                    provider: "kimi-code", at: at, input: try TokenCount.sum(input, write), output: output, cacheRead: read, cacheWrite: write)
            } else {
                let message = line["message"], payload = message["payload"]
                guard message["type"].stringValue == "StatusUpdate", payload["token_usage"].objectValue != nil else { return }
                guard let seconds = line["timestamp"].numberValue, seconds > 0, seconds <= 253402300799 else { throw ProviderFailure.format }
                let at = Date(timeIntervalSince1970: seconds), usage = payload["token_usage"]
                let input = try usage["input_other"].optionalCounter(), read = try usage["input_cache_read"].optionalCounter()
                let write = try usage["input_cache_creation"].optionalCounter(), output = try usage["output"].optionalCounter()
                let id = "\(session.id):" + (payload["message_id"].stringValue ?? "line:\(index)")
                let old = keyed[id].map { session.events[$0] }
                try session.add(id: id, model: "Unknown", provider: "Unknown", at: old?.timestamp ?? at,
                    input: try TokenCount.sum(input, write), output: output, cacheRead: read, cacheWrite: write)
                if let position = keyed[id] {
                    let latest = session.events.removeLast()
                    if latest.total >= session.events[position].total { session.events[position] = latest }
                } else { keyed[id] = session.events.count - 1 }
            }
        }
        return [session]
    }

    /// OpenCode names a session `New session - <ISO time>` (`Child session - …` for a sub-agent's) until its title agent
    /// answers the first message, and keeps that name when the title call fails.
    static func openCodeTitle(_ title: String?) -> String? {
        guard let title = SessionTitle.named(title),
              title.range(of: #"^(New|Child) session - \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$"#, options: .regularExpression) == nil else { return nil }
        return title
    }

    static func openCodeMessage(_ value: ProviderJSON, id: String, sessionID: String, path: String,
                                title: String? = nil, workspace: String? = nil, assistant: Bool = false) throws -> OpenAgentSession? {
        guard value["role"].stringValue == "assistant" || (assistant && value["role"] == .null) else { return nil }
        guard value["tokens"].objectValue != nil else { return nil }
        guard let at = ProviderDate.milliseconds(value["time"]["created"]) else { throw ProviderFailure.format }
        let tokens = value["tokens"]
        let input = try tokens["input"].optionalCounter(), output = try tokens["output"].optionalCounter()
        let read = try tokens["cache"]["read"].optionalCounter(), write = try tokens["cache"]["write"].optionalCounter()
        let model = value["modelID"].stringValue ?? value["model"]["id"].stringValue ?? "Unknown"
        let provider = value["providerID"].stringValue ?? value["model"]["providerID"].stringValue ?? "Unknown"
        let workspace = workspace ?? value["path"]["root"].stringValue, named = openCodeTitle(title)
        var session = OpenAgentSession(id: "opencode:\(sessionID)", client: .opencode,
            title: named ?? workspace.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "OpenCode",
            titleSource: named == nil ? .placeholder : .log, workspace: workspace, path: path)
        try session.add(id: "opencode:\(id)", model: model, provider: provider, at: at,
                    input: try TokenCount.sum(input, write), output: try TokenCount.sum(output, tokens["reasoning"].optionalCounter()), cacheRead: read,
                    cacheWrite: write, reasoning: try tokens["reasoning"].optionalCounter())
        session.end = ProviderDate.milliseconds(value["time"]["completed"]) ?? at
        return session
    }

    /// Replies read from OpenCode's database per statement.
    static let openCodePage = 5000

    /// Replies are in `message`, and in `session_message` for sessions of OpenCode's newer kind, whose table also holds
    /// agent and model switches before it holds any reply. The two name one reply by different ids, so a session is read
    /// from `session_message` once that table has its replies, and from `message` until then. A reply's record can hold
    /// far more than what it is counted by, so SQLite hands over only those fields, a page of replies at a time.
    static func openCodeSQLite(_ url: URL, since: Date = .distantPast) throws -> [OpenAgentSession] {
        let db = try ReadOnlySQLite(url)
        var tables = Set<String>()
        try db.rows("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('session_message', 'message', 'session_v2', 'session')") { row in
            if let name = ReadOnlySQLite.text(row, 0) { tables.insert(name) }
        }
        for table in tables { try db.requireTable(table) }
        let newer = tables.contains("session_message")
        guard newer || tables.contains("message") else { throw ProviderFailure.format }
        var sessions: [OpenAgentSession] = []
        // Prefer SQLite records. Stable message IDs deduplicate JSON records.
        let fields = ["role", "time", "tokens", "modelID", "providerID", "model", "path"]
            .map { "'\($0)', json_extract(m.data, '$.\($0)')" }.joined(separator: ", ")
        func read(_ messages: String, titles table: String?, where filter: String) throws {
            var archivable = false
            if let table { try db.rows("SELECT name FROM pragma_table_info('\(table)')") { row in archivable = archivable || ReadOnlySQLite.text(row, 0) == "time_archived" } }
            let metadata = table == nil ? "NULL, NULL, NULL" : "s.title, s.directory, \(archivable ? "s.time_archived" : "NULL")"
            let join = table.map { "LEFT JOIN \($0) s ON s.id = m.session_id" } ?? ""
            var last: String?, count = 0
            repeat {
                count = 0
                try db.rows("SELECT m.id, m.session_id, json_object(\(fields)), \(metadata) FROM \(messages) m \(join) WHERE \(filter) AND json_extract(m.data, '$.time.created') >= CAST(? AS REAL)\(last == nil ? "" : " AND m.id < ?") ORDER BY m.id DESC LIMIT \(openCodePage)",
                            strings: [String(since.timeIntervalSince1970 * 1000)] + (last.map { [$0] } ?? [])) { row in
                    guard let id = ReadOnlySQLite.text(row, 0), let sid = ReadOnlySQLite.text(row, 1), let raw = ReadOnlySQLite.text(row, 2) else { throw ProviderFailure.format }
                    count += 1
                    last = id
                    if var item = try openCodeMessage(ProviderJSON.read(Data(raw.utf8)), id: id, sessionID: sid, path: url.path,
                        title: ReadOnlySQLite.text(row, 3), workspace: ReadOnlySQLite.text(row, 4), assistant: messages == "session_message") {
                        item.archived = ReadOnlySQLite.text(row, 5) != nil
                        sessions.append(item)
                    }
                }
            } while count == openCodePage
        }
        if newer {
            try read("session_message", titles: ["session_v2", "session"].first(where: tables.contains), where: "m.type = 'assistant'")
        }
        if tables.contains("message") {
            try read("message", titles: ["session", "session_v2"].first(where: tables.contains), where: "json_extract(m.data, '$.role') = 'assistant'"
                + (newer ? " AND m.session_id NOT IN (SELECT session_id FROM session_message WHERE type = 'assistant' AND session_id IS NOT NULL)" : ""))
        }
        return sessions
    }
}
