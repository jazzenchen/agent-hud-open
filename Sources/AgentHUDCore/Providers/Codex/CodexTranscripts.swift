import AgentHUDSupport
import Foundation

/// Incremental summary of Codex rollout JSONL, shared by Desktop and CLI.
/// It retains counters and lifecycle events, never conversation bodies or tool output.
public struct CodexTranscript: Codable, Sendable {
    public struct Usage: Codable, Sendable {
        public let timestamp: Date
        public let model: String
        public let input: Int
        public let output: Int
        public let cachedInput: Int

        public var event: TranscriptSession.UsageEvent {
            .init(timestamp: timestamp, agentId: "codex-model:\(model)", tokensIn: input, tokensOut: output, cacheReadTokens: cachedInput)
        }
    }

    public private(set) var id: String?
    public private(set) var cwd: String?
    public private(set) var client = "Codex"
    public private(set) var isSubagent = false
    public private(set) var isInternal = false
    public private(set) var startedAt: Date?
    public private(set) var lastActivityAt: Date?
    public private(set) var task: String?
    public private(set) var model = "Unknown"
    public private(set) var usage: [Usage] = []
    private struct Turn: Codable, Sendable {
        let id: String?
        let startedAt: Date?
        var state: SessionTurn.State
        var observedAt: Date
    }
    private var turns: [Turn]?
    public private(set) var completions: [SessionCompletion]?
    private var totalInput = 0
    private var totalCached = 0
    private var totalOutput = 0
    private var hasTotals = false

    public init() {}

    public mutating func ingest(_ line: Data) {
        // Rollouts can contain multi-MB tool outputs. Inspect the envelope before decoding.
        guard Self.envelopes.contains(where: { line.range(of: $0) != nil }) else { return }
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              let type = object["type"] as? String,
              let payload = object["payload"] as? [String: Any],
              let timestamp = (object["timestamp"] as? String).flatMap(ISO8601Fast.parse) else { return }
        if type == "session_meta" {
            id = payload["id"] as? String
            cwd = payload["cwd"] as? String
            startedAt = (payload["timestamp"] as? String).flatMap(ISO8601Fast.parse) ?? timestamp
            let source = payload["source"] as? String
            let origin = payload["originator"] as? String
            isSubagent = (payload["source"] as? [String: Any])?["subagent"] != nil
            if let subagent = (payload["source"] as? [String: Any])?["subagent"] as? [String: Any] {
                isInternal = subagent["other"] as? String == "guardian"
            }
            // CLI launched from Desktop can inherit its originator; the rollout's source is authoritative.
            if source == "cli" { client = "CLI" }
            else if source == "exec" { client = "CLI · exec" }
            else if origin == "Codex Desktop" { client = "Desktop" }
            else if source == "vscode" { client = "IDE" }
            return
        }
        if type == "turn_context" {
            if let value = payload["model"] as? String { model = value }
            return
        }
        guard type == "event_msg", let kind = payload["type"] as? String else { return }
        // Forks copy earlier history. Read its cumulative baseline but do not count it again.
        let inherited = timestamp < (startedAt ?? .distantPast)
        if kind == "token_count" {
            guard let info = payload["info"] as? [String: Any],
                  let totals = info["total_token_usage"] as? [String: Any],
                  let input = totals["input_tokens"] as? Int,
                  let output = totals["output_tokens"] as? Int else { return }
            let cached = totals["cached_input_tokens"] as? Int ?? 0
            let last = info["last_token_usage"] as? [String: Any]
            let reset = input < totalInput || output < totalOutput || cached < totalCached
            let inputDelta = hasTotals && !reset ? input - totalInput : (last?["input_tokens"] as? Int ?? input)
            let cachedDelta = hasTotals && !reset ? cached - totalCached : (last?["cached_input_tokens"] as? Int ?? cached)
            let outputDelta = hasTotals && !reset ? output - totalOutput : (last?["output_tokens"] as? Int ?? output)
            totalInput = input; totalCached = cached; totalOutput = output; hasTotals = true
            guard !inherited, inputDelta > 0 || outputDelta > 0 else { return }
            // Cached input is already included in input_tokens; reasoning is already in output_tokens.
            usage.append(Usage(timestamp: timestamp, model: model, input: max(0, inputDelta - cachedDelta), output: max(0, outputDelta),
                               cachedInput: max(0, cachedDelta)))
            lastActivityAt = timestamp
        } else if !inherited {
            switch kind {
            case "task_started":
                lastActivityAt = max(lastActivityAt ?? timestamp, timestamp)
                let turnID = payload["turn_id"] as? String
                if !(turns ?? []).contains(where: { $0.id == turnID && (turnID != nil || ($0.state == .running && $0.startedAt == timestamp)) }),
                   timestamp >= (turns?.last?.startedAt ?? .distantPast) {
                    turns = Array(((turns ?? []) + [Turn(id: turnID, startedAt: timestamp, state: .running, observedAt: timestamp)]).suffix(32))
                }
            case "task_complete":
                lastActivityAt = max(lastActivityAt ?? timestamp, timestamp)
                let finished = finishTurn(payload["turn_id"] as? String, state: .completed, at: timestamp)
                if let id, !isSubagent, !isInternal {
                    let completion = SessionCompletion(sessionID: id, vendor: "Codex",
                        turnID: payload["turn_id"] as? String ?? finished?.id ?? String(RecordCoding.milliseconds(timestamp)),
                        task: task ?? cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Codex",
                        model: model, startedAt: finished?.startedAt, completedAt: timestamp)
                    if completions?.contains(where: { $0.id == completion.id }) != true {
                        completions = (completions ?? []) + [completion]
                    }
                }
            case "turn_aborted":
                lastActivityAt = max(lastActivityAt ?? timestamp, timestamp)
                _ = finishTurn(payload["turn_id"] as? String, state: .ended, at: timestamp)
            case "user_message":
                if task == nil, let text = payload["message"] as? String {
                    task = ClaudeTranscriptParser.title(from: text)
                }
            default: break
            }
        }
        // A source event can refresh an explicitly started turn, but never start or resurrect one.
        if !inherited, let index = turns?.indices.last, turns?[index].state == .running,
           timestamp > turns![index].observedAt, kind != "task_complete", kind != "turn_aborted" {
            turns?[index].observedAt = timestamp
        }
    }

    public func isLive(now: Date, modifiedAt: Date, freshness: TimeInterval = 120) -> Bool {
        guard turns?.last.map({ $0.state == .running }) != false, !isInternal else { return false }
        return now.timeIntervalSince(modifiedAt) < freshness && lastActivityAt != nil
    }

    public var sessionTurns: [SessionTurn] {
        guard let id, !isSubagent, !isInternal else { return [] }
        return (turns ?? []).compactMap { turn in
            guard let turnID = turn.id, !turnID.isEmpty else { return nil }
            return SessionTurn(provider: "codex", sessionID: id, turnID: turnID, state: turn.state,
                startedAtMs: turn.startedAt.map(RecordCoding.milliseconds), observedAtMs: RecordCoding.milliseconds(turn.observedAt))
        }
    }

    private mutating func finishTurn(_ id: String?, state: SessionTurn.State, at date: Date) -> Turn? {
        if let index = turns?.lastIndex(where: { $0.id == id }) {
            guard date >= turns![index].observedAt else { return nil }
            // A terminal observation is final. Repeated lines do not change its timestamp or outcome.
            guard turns![index].state == .running else { return turns![index] }
            turns?[index].state = state; turns?[index].observedAt = date
            return turns![index]
        }
        // A terminal-only log still identifies its turn, but cannot provide a start time.
        guard turns?.last?.state != .running else { return nil }
        let value = Turn(id: id, startedAt: nil, state: state, observedAt: date)
        turns = Array(((turns ?? []) + [value]).suffix(32))
        return value
    }

    private static let envelopes = ["\"session_meta\"", "\"turn_context\"", "\"event_msg\""].map { Data($0.utf8) }
}

/// Cooperative, persisted tail reader. A partially written final line is retried on the next poll.
public actor CodexTranscriptStore {
    private struct Entry: Codable {
        var modifiedAt: Date
        var size: Int
        var offset: UInt64
        var transcript: CodexTranscript
    }

    public struct Session: Sendable {
        public let transcript: CodexTranscript
        public let modifiedAt: Date
        public let path: String
        public let title: String?
    }

    public struct Result: Sendable {
        public let sessions: [Session]
        public let indexing: IndexProgress?
    }

    private let roots: [URL]
    private let cacheURL: URL?
    private let indexURL: URL?
    private var entries: [String: Entry] = [:]
    private var dirty = false
    private var lastSavedAt = Date.distantPast
    private let cacheSaveInterval: TimeInterval

    public init(roots: [URL], cacheURL: URL? = nil, indexURL: URL? = nil, cacheSaveInterval: TimeInterval = 30) {
        self.roots = roots
        self.cacheURL = cacheURL
        self.indexURL = indexURL
        self.cacheSaveInterval = cacheSaveInterval
        if let cacheURL, let data = try? Data(contentsOf: cacheURL) {
            entries = (try? JSONDecoder().decode([String: Entry].self, from: data)) ?? [:]
        }
    }

    public static func standard(directory: URL = CodexLocator.dataDirectory) -> CodexTranscriptStore {
        CodexTranscriptStore(roots: [directory.appendingPathComponent("sessions"), directory.appendingPathComponent("archived_sessions")],
                             cacheURL: AppSupport.directory.appendingPathComponent("codex-transcripts-v5.json"),
                             indexURL: directory.appendingPathComponent("session_index.jsonl"))
    }

    public func index(since cutoff: Date, timeBudget: TimeInterval = 1.5) -> Result {
        let deadline = Date().addingTimeInterval(timeBudget)
        let keys: Set<URLResourceKey> = [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey]
        var candidates: [(url: URL, modified: Date, size: Int)] = []
        for root in roots {
            guard let files = FileManager.default.enumerator(at: root, includingPropertiesForKeys: Array(keys), options: [.skipsHiddenFiles]) else { continue }
            for case let url as URL in files {
                guard url.pathExtension == "jsonl", url.lastPathComponent.hasPrefix("rollout-"),
                      let values = try? url.resourceValues(forKeys: keys), values.isRegularFile == true,
                      let modified = values.contentModificationDate, modified >= cutoff, let size = values.fileSize else { continue }
                candidates.append((url, modified, size))
            }
        }
        candidates.sort { $0.modified > $1.modified }
        let present = Set(candidates.map { $0.url.path })
        dirty = dirty || entries.keys.contains { !present.contains($0) }
        entries = entries.filter { present.contains($0.key) }
        var pending = 0
        for candidate in candidates {
            let old = entries[candidate.url.path]
            if let old, old.size == candidate.size, old.offset == candidate.size, old.modifiedAt == candidate.modified { continue }
            if Date() >= deadline { pending += 1; continue }
            guard let handle = try? FileHandle(forReadingFrom: candidate.url) else { continue }
            defer { try? handle.close() }
            var entry = old ?? Entry(modifiedAt: candidate.modified, size: candidate.size, offset: 0, transcript: CodexTranscript())
            if candidate.size < entry.offset || (old?.size == candidate.size && old?.modifiedAt != candidate.modified) {
                entry.offset = 0; entry.transcript = CodexTranscript()
            }
            try? handle.seek(toOffset: entry.offset)
            var carry = Data()
            while Date() < deadline, let chunk = try? handle.read(upToCount: 1 << 20), !chunk.isEmpty {
                carry.append(chunk)
                var start = carry.startIndex
                while let newline = carry[start...].firstIndex(of: 0x0A) {
                    entry.transcript.ingest(Data(carry[start..<newline]))
                    entry.offset += UInt64(newline - start + 1)
                    start = newline + 1
                }
                carry = Data(carry[start...])
            }
            entry.modifiedAt = candidate.modified; entry.size = candidate.size
            entries[candidate.url.path] = entry; dirty = true
            // An incomplete final line is not an indexing backlog.
            if entry.offset + UInt64(carry.count) < candidate.size { pending += 1 }
        }
        if dirty, let cacheURL, Date().timeIntervalSince(lastSavedAt) >= cacheSaveInterval,
           let data = try? JSONEncoder().encode(entries) {
            do {
                try FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
                try data.write(to: cacheURL, options: .atomic)
                dirty = false
                lastSavedAt = Date()
            } catch { /* Retry on the next poll; transcripts remain the source of truth. */ }
        }
        var titles: [String: String] = [:]
        if let indexURL, let text = try? String(contentsOf: indexURL, encoding: .utf8) {
            for line in text.split(separator: "\n") {
                if let item = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                   let id = item["id"] as? String, let title = item["thread_name"] as? String { titles[id] = title }
            }
        }
        // A rollout can move into archived_sessions. One session id contributes usage exactly once.
        var sessions: [String: Session] = [:]
        for candidate in candidates {
            guard let entry = entries[candidate.url.path], let id = entry.transcript.id,
                  sessions[id] == nil else { continue }
            sessions[id] = Session(transcript: entry.transcript, modifiedAt: candidate.modified, path: candidate.url.path, title: titles[id])
        }
        return Result(sessions: Array(sessions.values), indexing: pending > 0 ? IndexProgress(done: candidates.count - pending, total: candidates.count) : nil)
    }
}
