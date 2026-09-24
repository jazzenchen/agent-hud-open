import AgentHUDSupport
import Foundation
import SQLite3

// Transcript stores, response identities and bookkeeping-row exclusions follow Tokscale openclaw.rs (MIT).
enum OpenClawSessions: LocalSessionLayout {
    static let installPaths = [".openclaw/agents", ".clawdbot/agents"]
    /// Covers the report's week plus its current hour, so a cached parse still holds every event a later report needs.
    static let lookback: TimeInterval = 8 * 86400

    static func roots(home: URL, environment: [String: String]) -> [URL] {
        if let state = environment["OPENCLAW_STATE_DIR"]?.trimmingCharacters(in: .whitespaces), !state.isEmpty {
            let path = state == "~" ? home.path : state.hasPrefix("~/") ? home.path + state.dropFirst() : state
            return [URL(fileURLWithPath: path).appendingPathComponent("agents")]
        }
        let manager = FileManager.default, current = home.appendingPathComponent(".openclaw")
        // `.clawdbot` is used only while `.openclaw` is absent; named profiles are `.openclaw-<name>` beside it.
        let profiles = ((try? manager.contentsOfDirectory(atPath: home.path)) ?? []).filter {
            $0.range(of: #"^\.openclaw-[A-Za-z0-9][A-Za-z0-9_-]{0,63}$"#, options: .regularExpression) != nil
        }.sorted().map { home.appendingPathComponent($0) }
        return ([manager.fileExists(atPath: current.path) ? current : home.appendingPathComponent(".clawdbot")] + profiles)
            .map { $0.appendingPathComponent("agents") }
    }

    /// Only `agents/<id>/agent` and `agents/<id>/sessions` are visited; agent-owned Codex homes and other files are not.
    static func skips(_ url: URL) -> Bool {
        let parent = url.deletingLastPathComponent(), folders = ["agent", "sessions"]
        if parent.deletingLastPathComponent().lastPathComponent == "agents" { return !folders.contains(url.lastPathComponent) }
        return folders.contains(parent.lastPathComponent)
            && parent.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent == "agents" && !accepts(url)
    }

    static func accepts(_ url: URL) -> Bool {
        let name = url.lastPathComponent, folder = url.deletingLastPathComponent().lastPathComponent
        if folder == "agent" { return name == "openclaw-agent.sqlite" }
        // Plain reset and deleted archives keep usage reclaimed from SQLite; zstd archives are skipped.
        return folder == "sessions" && (name.hasSuffix(".jsonl")
            || [".jsonl.reset.", ".jsonl.deleted."].contains { name.contains($0) } && !name.hasSuffix(".zst") && !name.hasSuffix(".tmp"))
    }

    static func related(_ url: URL) -> [URL] { url.pathExtension == "sqlite" ? [URL(fileURLWithPath: url.path + "-wal")] : [] }

    static func read(_ url: URL) throws -> ProviderSessions { try read(url, since: Date().addingTimeInterval(-lookback)) }

    static func read(_ url: URL, since: Date) throws -> ProviderSessions {
        url.pathExtension == "sqlite" ? try database(url, since: since) : try transcript(url)
    }

    /// Database sessions own titles and turns; transcript files only add events under the same ids.
    static func merge(_ sessions: [ProviderSession]) -> [ProviderSession] {
        sessions.filter { $0.path?.hasSuffix(".sqlite") == true } + sessions.filter { $0.path?.hasSuffix(".sqlite") != true }
    }

    struct Usage {
        private var model: String?
        private var seen = Set<String>()

        /// Events carry their own time and model so the SQLite row and a JSONL copy of one event are identical.
        mutating func event(_ entry: ProviderJSON, session: String, ordinal: Int) throws -> ProviderEvent? {
            if entry["type"].stringValue == "model_change" { model = entry["modelId"].stringValue ?? model; return nil }
            let message = entry["message"], usage = message["usage"]
            guard entry["type"].stringValue == "message", message["role"].stringValue == "assistant", usage.objectValue != nil else { return nil }
            let provider = message["provider"].stringValue ?? "", name = message["model"].stringValue
            // OpenClaw's own delivery and gateway rows are not model output. Claude Code writes its own transcript for
            // claude-cli runs, and Codex-harness rows mirror only a turn's last response.
            if message["api"].stringValue == "openclaw-transcript" || provider == "openclaw" && ["delivery-mirror", "gateway-injected"].contains(name ?? "")
                || provider.lowercased() == "claude-cli" || message["idempotencyKey"].stringValue?.hasPrefix("codex-app-server:") == true { return nil }
            guard let at = ProviderDate.milliseconds(message["timestamp"]) ?? ProviderDate.iso(entry["timestamp"].stringValue) else { throw ProviderFailure.format }
            let input = try usage["input"].optionalCounter(), output = try usage["output"].optionalCounter()
            let read = try usage["cacheRead"].optionalCounter(), write = try usage["cacheWrite"].optionalCounter()
            guard try TokenCount.sum(input, output, read, write) > 0 else { return nil }
            let identity: String
            if let response = message["responseId"].stringValue, !response.isEmpty {
                identity = "response:" + RecordCoding.hash([provider, response])
            } else if let id = entry["id"].stringValue, !id.isEmpty {
                identity = "entry:" + RecordCoding.hash([id, String(RecordCoding.milliseconds(at))] + [input, output, read, write].map(String.init))
            } else { identity = "\(session):\(ordinal)" }
            guard seen.insert(identity).inserted else { return nil }
            // Input excludes cache reads and writes; reasoning is already inside output.
            return ProviderEvent(id: identity, model: name ?? model ?? "Unknown", timestamp: at,
                                 input: try TokenCount.sum(input, write), output: output, cacheRead: read, cacheWrite: write)
        }
    }

    static func transcript(_ url: URL) throws -> ProviderSessions {
        let name = url.lastPathComponent, raw = String(name[..<(name.range(of: ".jsonl")?.lowerBound ?? name.endIndex)])
        let agent = url.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent
        var session = ProviderSession(id: "openclaw:\(raw)", title: "OpenClaw · \(agent)", path: url.path, client: "OpenClaw")
        var usage = Usage()
        try ProviderFiles.lines(url) { entry, line in
            if entry["type"].stringValue == "session" {
                session.workspace = entry["cwd"].stringValue; session.startedAt = ProviderDate.iso(entry["timestamp"].stringValue)
            } else if let event = try usage.event(entry, session: session.id, ordinal: line) { session.events.append(event) }
        }
        session.lastActivity = session.events.map(\.timestamp).max()
        return ProviderSessions(sessions: [session])
    }

    static func database(_ url: URL, since: Date) throws -> ProviderSessions {
        let db = try ReadOnlySQLite(url)
        // Only session tables are queried. The same file's auth_profile_store holds credentials and is never read.
        for table in ["session_windows", "session_nodes", "transcript_events"] { try db.requireTable(table) }
        let agent = url.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent
        let cutoff = String(RecordCoding.milliseconds(since))
        var sessions: [(raw: String, session: ProviderSession, turn: SessionTurn?)] = []
        try db.rows("""
            SELECT w.session_id, COALESCE(w.display_name, n.display_name, n.label), w.created_at,
                   MAX(w.updated_at, COALESCE(w.transcript_updated_at, 0), COALESCE(w.ended_at, 0)), w.started_at, w.ended_at, w.status,
                   n.session_key, COALESCE(w.spawned_by, n.spawned_by), json_extract(n.entry_json, '$.lifecycleRunId'), json_extract(n.entry_json, '$.lastRunId'),
                   (SELECT CASE WHEN json_extract(e.event_json, '$.type') = 'session' THEN json_extract(e.event_json, '$.cwd') END
                    FROM transcript_events e WHERE e.session_id = w.session_id ORDER BY e.seq LIMIT 1)
            FROM session_windows w LEFT JOIN session_nodes n ON n.current_session_id = w.session_id
            WHERE MAX(w.updated_at, COALESCE(w.transcript_updated_at, 0), COALESCE(w.ended_at, 0)) >= CAST(? AS INTEGER)
            """, strings: [cutoff]) { row in
            guard let raw = ReadOnlySQLite.text(row, 0), let observed = milliseconds(row, 3) else { throw ProviderFailure.format }
            let id = "openclaw:\(raw)"
            let session = ProviderSession(id: id, title: ReadOnlySQLite.text(row, 1) ?? "OpenClaw · \(agent)", workspace: ReadOnlySQLite.text(row, 11),
                path: url.path, client: "OpenClaw", startedAt: milliseconds(row, 2).map(RecordCoding.date), lastActivity: RecordCoding.date(observed))
            // Gateway lifecycle status belongs to the session's current window; sub-agents never finish their parent's turn.
            var turn: SessionTurn?
            let started = milliseconds(row, 4), ended = milliseconds(row, 5)
            if let status = ReadOnlySQLite.text(row, 6), ReadOnlySQLite.text(row, 7) != nil, ReadOnlySQLite.text(row, 8) == nil,
               let runID = ReadOnlySQLite.text(row, status == "running" ? 9 : 10) ?? started.map({ "started:\($0)" }) {
                let state: SessionTurn.State = status == "running" ? .running : status == "done" ? .completed : .ended
                turn = SessionTurn(provider: "OpenClaw", sessionID: id, turnID: runID, state: state, startedAtMs: started,
                                   observedAtMs: state == .running ? observed : ended ?? observed)
            }
            sessions.append((raw, session, turn))
        }
        for index in sessions.indices {
            var usage = Usage()
            // Only whitelisted fields leave SQLite; prompts, tool bodies and content stay in the database.
            try db.rows("""
                SELECT e.seq, json_object('type', json_extract(e.event_json, '$.type'), 'id', json_extract(e.event_json, '$.id'),
                    'timestamp', json_extract(e.event_json, '$.timestamp'), 'modelId', json_extract(e.event_json, '$.modelId'),
                    'message', json_object('role', json_extract(e.event_json, '$.message.role'), 'api', json_extract(e.event_json, '$.message.api'),
                        'provider', json_extract(e.event_json, '$.message.provider'), 'model', json_extract(e.event_json, '$.message.model'),
                        'responseId', json_extract(e.event_json, '$.message.responseId'), 'timestamp', json_extract(e.event_json, '$.message.timestamp'),
                        'idempotencyKey', json_extract(e.event_json, '$.message.idempotencyKey'), 'usage', json_extract(e.event_json, '$.message.usage')))
                FROM transcript_events e
                WHERE e.session_id = ? AND (instr(e.event_json, '"model_change"') > 0 OR e.created_at >= CAST(? AS INTEGER) AND instr(e.event_json, '"usage"') > 0)
                ORDER BY e.seq
                """, strings: [sessions[index].raw, cutoff]) { row in
                guard let text = ReadOnlySQLite.text(row, 1) else { throw ProviderFailure.format }
                if let event = try usage.event(ProviderJSON.read(Data(text.utf8)), session: sessions[index].session.id, ordinal: Int(sqlite3_column_int64(row, 0))) {
                    sessions[index].session.events.append(event)
                }
            }
        }
        return ProviderSessions(sessions: sessions.map { item in
            var session = item.session
            guard let turn = item.turn else { return session }
            session.turns = [turn]
            if turn.state == .completed {
                session.completions = [SessionCompletion(sessionID: session.id, vendor: "OpenClaw", turnID: turn.turnID, task: session.title,
                    model: session.events.max { $0.timestamp < $1.timestamp }?.model ?? "Unknown",
                    startedAt: turn.startedAtMs.map(RecordCoding.date), completedAt: RecordCoding.date(turn.observedAtMs))]
            }
            return session
        })
    }

    private static func milliseconds(_ row: OpaquePointer, _ column: Int32) -> Int64? {
        sqlite3_column_type(row, column) == SQLITE_NULL ? nil : sqlite3_column_int64(row, column)
    }
}
