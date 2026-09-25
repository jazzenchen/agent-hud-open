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
        /// The part of `input` written to the cache and the part of `output` spent reasoning.
        public var cacheWrite = 0
        public var reasoning = 0
        /// The model's context window as the rollout states it.
        public var contextWindow: Int? = nil

        public var event: UsageEvent {
            .init(timestamp: timestamp, agentId: "codex-model:\(model)", tokensIn: input, tokensOut: output, cacheReadTokens: cachedInput,
                  cacheWriteTokens: cacheWrite, reasoningTokens: reasoning)
        }
    }

    public private(set) var id: String?
    public private(set) var cwd: String?
    public private(set) var client = "Codex"
    public private(set) var isSubagent = false
    public private(set) var isInternal = false
    /// The thread that started this rollout's sub-agent: a spawned agent names it in its source, a guardian beside it.
    public private(set) var parentThreadID: String?
    public private(set) var startedAt: Date?
    public private(set) var lastActivityAt: Date?
    public private(set) var task: String?
    public private(set) var model = "Unknown"
    /// Usage read since the store last recorded it in the ledger; the totals below cover the whole rollout.
    public private(set) var usage: [Usage] = []
    public private(set) var inputTokens = 0
    public private(set) var outputTokens = 0
    public private(set) var cachedInputTokens = 0
    /// Models that reported usage in this rollout.
    public private(set) var models: Set<String> = []
    /// Usage entries already recorded, so each keeps its position as its ledger key.
    private var recordedUsage = 0
    private struct Turn: Codable, Sendable {
        let id: String?
        let startedAt: Date?
        var state: SessionTurn.State
        var observedAt: Date
        /// What the agent said, kept only while the app runs: the ledger stores no conversation text.
        var message: String?
        enum CodingKeys: String, CodingKey { case id, startedAt, state, observedAt }
    }
    /// How much of an agent message is kept; readers truncate it further.
    static let messageLength = 2048
    private var turns: [Turn]?
    public private(set) var completions: [SessionCompletion]?
    private var totalInput = 0
    private var totalCached = 0
    private var totalOutput = 0
    private var totalWrite = 0
    private var totalReasoning = 0
    private var hasTotals = false
    /// Turn starts and compactions read since the store last took them.
    private var marks: [UsageLedger.Mark] = []

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
                parentThreadID = payload["parent_thread_id"] as? String
                    ?? (subagent["thread_spawn"] as? [String: Any])?["parent_thread_id"] as? String
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
        // Forks copy earlier history. Read its cumulative baseline but do not count it again.
        let inherited = timestamp < (startedAt ?? .distantPast)
        if type == "compacted" {
            if !inherited { marks.append(.init(.compaction, at: timestamp)) }
            return
        }
        guard type == "event_msg", let kind = payload["type"] as? String else { return }
        if kind == "token_count" {
            guard let info = payload["info"] as? [String: Any],
                  let totals = info["total_token_usage"] as? [String: Any],
                  let input = totals["input_tokens"] as? Int,
                  let output = totals["output_tokens"] as? Int else { return }
            let cached = totals["cached_input_tokens"] as? Int ?? 0
            let written = totals["cache_write_input_tokens"] as? Int ?? 0
            let reasoned = totals["reasoning_output_tokens"] as? Int ?? 0
            let last = info["last_token_usage"] as? [String: Any]
            let reset = input < totalInput || output < totalOutput || cached < totalCached
            let inputDelta = hasTotals && !reset ? input - totalInput : (last?["input_tokens"] as? Int ?? input)
            let cachedDelta = hasTotals && !reset ? cached - totalCached : (last?["cached_input_tokens"] as? Int ?? cached)
            let outputDelta = hasTotals && !reset ? output - totalOutput : (last?["output_tokens"] as? Int ?? output)
            let writeDelta = hasTotals && !reset ? written - totalWrite : (last?["cache_write_input_tokens"] as? Int ?? written)
            let reasoningDelta = hasTotals && !reset ? reasoned - totalReasoning : (last?["reasoning_output_tokens"] as? Int ?? reasoned)
            totalInput = input; totalCached = cached; totalOutput = output; totalWrite = written; totalReasoning = reasoned; hasTotals = true
            guard !inherited, inputDelta > 0 || outputDelta > 0 else { return }
            // Cached input and cache writes are part of input_tokens; reasoning is part of output_tokens.
            var sample = Usage(timestamp: timestamp, model: model, input: max(0, inputDelta - cachedDelta), output: max(0, outputDelta),
                               cachedInput: max(0, cachedDelta))
            sample.cacheWrite = max(0, writeDelta)
            sample.reasoning = max(0, reasoningDelta)
            sample.contextWindow = info["model_context_window"] as? Int
            usage.append(sample)
            inputTokens += sample.input
            outputTokens += sample.output
            cachedInputTokens += sample.cachedInput
            models.insert(model)
            lastActivityAt = timestamp
        } else if !inherited {
            switch kind {
            case "task_started":
                lastActivityAt = max(lastActivityAt ?? timestamp, timestamp)
                marks.append(.init(.prompt, at: timestamp))
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
                    task = SessionTitle.from(text)
                }
            case "agent_message":
                // The visible answer of the turn that is running, kept where the turn can carry it.
                if let text = payload["message"] as? String, let index = turns?.indices.last, turns?[index].state == .running {
                    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !trimmed.isEmpty { turns?[index].message = String(trimmed.prefix(Self.messageLength)) }
                }
            case "item_completed":
                // Current rollouts record a prompt only as a completed UserMessage item of text and image parts.
                guard task == nil, let item = payload["item"] as? [String: Any], item["type"] as? String == "UserMessage",
                      let content = item["content"] as? [[String: Any]] else { break }
                task = content.lazy.filter { $0["type"] as? String == "text" }.compactMap { ($0["text"] as? String).flatMap(SessionTitle.from) }.first
            default: break
            }
        }
        // A source event can refresh an explicitly started turn, but never start or resurrect one.
        if !inherited, let index = turns?.indices.last, turns?[index].state == .running,
           timestamp > turns![index].observedAt, kind != "task_complete", kind != "turn_aborted" {
            turns?[index].observedAt = timestamp
        }
    }

    /// Hands the usage read since the last call to the ledger, keyed by its position in the rollout.
    mutating func drainUsage() -> [UsageLedger.Event] {
        defer {
            recordedUsage += usage.count
            usage = []
        }
        return usage.enumerated().map { offset, sample in
            UsageLedger.Event(key: "u\(recordedUsage + offset)", timestamp: sample.timestamp, agentId: "codex-model:\(sample.model)",
                              tokensIn: sample.input, tokensOut: sample.output, cacheReadTokens: sample.cachedInput,
                              cacheWriteTokens: sample.cacheWrite, reasoningTokens: sample.reasoning, contextWindow: sample.contextWindow)
        }
    }

    /// Hands over the turn starts and compactions read since the last call.
    mutating func drainMarks() -> [UsageLedger.Mark] {
        defer { marks = [] }
        return marks
    }

    /// Running means the newest turn is still going. A quiet rollout does not end it: one tool call can take minutes
    /// without writing a line, and only silence long enough to mean the client is gone does. A rollout that never
    /// logged a turn falls back to how recently it was written.
    public func isLive(now: Date, modifiedAt: Date, freshness: TimeInterval = 120,
                       abandonedAfter: TimeInterval = UsageRefresh.abandonedTurnTimeout) -> Bool {
        guard !isInternal, lastActivityAt != nil else { return false }
        let quiet = now.timeIntervalSince(modifiedAt)
        guard let running = turns?.last.map({ $0.state == .running }) else { return quiet < freshness }
        return running && quiet < abandonedAfter
    }

    public var sessionTurns: [SessionTurn] {
        guard let id, !isSubagent, !isInternal else { return [] }
        return (turns ?? []).compactMap { turn in
            guard let turnID = turn.id, !turnID.isEmpty else { return nil }
            return SessionTurn(provider: "codex", sessionID: id, turnID: turnID, state: turn.state,
                startedAtMs: turn.startedAt.map(RecordCoding.milliseconds), observedAtMs: RecordCoding.milliseconds(turn.observedAt),
                message: turn.message)
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

    private static let envelopes = ["\"session_meta\"", "\"turn_context\"", "\"event_msg\"", "\"type\":\"compacted\""].map { Data($0.utf8) }
}

/// Cooperative tail reader over the usage ledger. A partially written final line is retried on the next poll.
/// Each rollout is one ledger contribution; when a session's rollout exists in several places, only its newest copy counts.
public actor CodexTranscriptStore {
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

    let roots: [URL]
    private let indexURL: URL?
    private let ledger: UsageLedger
    private let logs: TailLogStore<CodexRollouts>
    private var titles: (signature: String, values: [String: String]) = ("", [:])

    /// - watchesChanges: after the first listing, polls look only at rollouts a directory watch reports changed.
    public init(roots: [URL], indexURL: URL? = nil, ledger: UsageLedger = .inMemory(), watchesChanges: Bool = false) {
        self.roots = roots
        self.indexURL = indexURL
        self.ledger = ledger
        logs = TailLogStore(roots: roots, ledger: ledger, watchesChanges: watchesChanges) { $0.pathExtension == "jsonl" && $0.lastPathComponent.hasPrefix("rollout-") }
    }

    public static func standard(directory: URL = CodexLocator.dataDirectory, ledger: UsageLedger = .inMemory()) -> CodexTranscriptStore {
        CodexTranscriptStore(roots: [directory.appendingPathComponent("sessions"), directory.appendingPathComponent("archived_sessions")],
                             indexURL: directory.appendingPathComponent("session_index.jsonl"), ledger: ledger, watchesChanges: true)
    }

    public func fileChanges(_ paths: Set<String>?) { logs.noteChanges(paths) }

    /// Codex's 15-minute token totals from the period holding `since`.
    public func usage(since: Date) async -> [UsageBucket] {
        (try? await ledger.buckets(since: since, source: CodexRollouts.source)) ?? []
    }

    public func index(since cutoff: Date, timeBudget: TimeInterval = 1.5) async -> Result {
        let pass = await logs.index(since: cutoff, timeBudget: timeBudget)
        let titles = readTitles()
        // A rollout can move into archived_sessions. One session id contributes usage exactly once.
        var sessions: [String: Session] = [:]
        for log in pass.logs.sorted(by: { $0.file.modified > $1.file.modified }) {
            guard let entry = logs.entry(log.path), let id = entry.summary.id, sessions[id] == nil else { continue }
            sessions[id] = Session(transcript: entry.summary, modifiedAt: log.file.modified, path: log.path, title: titles[id])
        }
        return Result(sessions: Array(sessions.values),
                      indexing: pass.pending > 0 ? IndexProgress(done: pass.logs.count - pass.pending, total: pass.logs.count) : nil)
    }

    /// Thread names, read again only when the index file changed.
    private func readTitles() -> [String: String] {
        guard let indexURL, let values = try? indexURL.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey]) else { return [:] }
        let signature = "\(values.contentModificationDate?.timeIntervalSince1970 ?? 0):\(values.fileSize ?? 0)"
        guard signature != titles.signature else { return titles.values }
        var result: [String: String] = [:]
        if let text = try? String(contentsOf: indexURL, encoding: .utf8) {
            for line in text.split(separator: "\n") {
                if let item = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                   let id = item["id"] as? String, let title = item["thread_name"] as? String { result[id] = title }
            }
        }
        titles = (signature, result)
        return result
    }
}

/// Codex rollouts; an archived copy of a rollout counts once.
enum CodexRollouts: TailLog {
    static let source = "codex"
    static let summaryKey = "transcript"
    /// 3: cache writes, reasoning, context windows, turn starts and compactions. 4: sub-agents name the thread that
    /// started them.
    static let version = 4

    static func summary(for url: URL) -> CodexTranscript { CodexTranscript() }

    static func ingest(_ lines: Data, into transcript: inout CodexTranscript) -> [UsageLedger.Event] {
        for line in lines.split(separator: 0x0A) { transcript.ingest(line) }
        return transcript.drainUsage()
    }

    static func drainMarks(_ transcript: inout CodexTranscript) -> [UsageLedger.Mark] { transcript.drainMarks() }

    static func group(_ transcript: CodexTranscript) -> String? { transcript.id ?? "" }
}
