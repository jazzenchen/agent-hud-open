import AgentHUDSupport
import Foundation

/// Incremental summary of Codex rollout JSONL, shared by Desktop and CLI.
/// It persists counters and lifecycle events; answer previews are kept only while the app runs.
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
        /// Where the line that reported it begins in the rollout, when it was read from a known place.
        public var offset: Int? = nil

        public var event: UsageEvent {
            .init(timestamp: timestamp, agentId: "codex-model:\(model)", tokensIn: input, tokensOut: output, cacheReadTokens: cachedInput,
                  cacheWriteTokens: cacheWrite, reasoningTokens: reasoning)
        }
    }

    public private(set) var id: String?
    public private(set) var cwd: String?
    public private(set) var client = "Codex"
    /// The rollout's authoritative client distinguishes Desktop from a CLI that inherited its originator.
    public var navigationTarget: SessionNavigationTarget? {
        guard client == "Desktop", let id, !id.isEmpty else { return nil }
        return .codexThread(id: id)
    }
    public private(set) var isSubagent = false
    public private(set) var isInternal = false
    /// The thread that started this rollout's sub-agent: a spawned agent names it in its source, a guardian beside it.
    public private(set) var parentThreadID: String?
    public private(set) var agentName: String?
    private(set) var agentPath: String?
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
        /// An explicitly final answer, kept apart from the agent's progress messages.
        var finalMessage: String?
        /// Why the turn ended, when the rollout said.
        var outcome: TurnOutcome?
        enum CodingKeys: String, CodingKey { case id, startedAt, state, observedAt, outcome }
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
    /// Newer forks restamp copied history and mark its last ordinal in their first metadata.
    private var inheritedThroughOrdinal: Int?
    /// Turn starts and compactions read since the store last took them.
    private var marks: [UsageLedger.Mark] = []
    /// What the running turn is busy with: the calls still waiting for their output and the kind of work they do.
    public private(set) var activity: SessionActivity?

    public init() {}

    /// - offset: where the line begins in its rollout, which keys the usage it reports.
    public mutating func ingest(_ line: Data, at offset: Int? = nil) {
        // A call the model made and its output are read from their first bytes alone: an output can run to megabytes.
        let head = line.prefix(1024)
        if head.range(of: Self.responseItem) != nil {
            noteCall(line, head: head)
            return
        }
        // Rollouts can contain multi-MB tool outputs. Inspect the envelope before decoding.
        guard Self.envelopes.contains(where: { line.range(of: $0) != nil }) else { return }
        // A completed item counts as a prompt or an answer. A tool's call and its output, most of a long rollout's bytes
        // with its screenshots and page dumps, only date the turn, which their first bytes do without decoding them.
        if head.range(of: Self.itemCompleted) != nil, head.range(of: Self.itemType) != nil,
           head.range(of: Self.userItem) == nil, head.range(of: Self.agentItem) == nil {
            if let timestamp = Self.timestamp(in: head), timestamp >= (startedAt ?? .distantPast),
               let index = turns?.indices.last, turns?[index].state == .running, timestamp > turns![index].observedAt {
                turns?[index].observedAt = timestamp
                // A file changed or an image viewed is recorded only once done, which says what the turn does all the same.
                if head.range(of: Self.fileChangeItem) != nil { activity?.did(.edit, at: timestamp) }
                else if head.range(of: Self.imageViewItem) != nil { activity?.did(.read, at: timestamp) }
            }
            return
        }
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              let type = object["type"] as? String,
              let payload = object["payload"] as? [String: Any],
              let timestamp = (object["timestamp"] as? String).flatMap(ISO8601Fast.parse) else { return }
        if type == "session_meta" {
            // A fork starts with its own metadata, then can replay the parent's history and metadata.
            // The first valid identity owns this file and the inherited-history cutoff.
            guard id == nil, let sessionID = payload["id"] as? String, !sessionID.isEmpty else { return }
            id = sessionID
            inheritedThroughOrdinal = payload["subagent_history_start_ordinal"] as? Int
            cwd = payload["cwd"] as? String
            startedAt = (payload["timestamp"] as? String).flatMap(ISO8601Fast.parse) ?? timestamp
            let source = payload["source"] as? String
            let origin = payload["originator"] as? String
            isSubagent = (payload["source"] as? [String: Any])?["subagent"] != nil
            if let subagent = (payload["source"] as? [String: Any])?["subagent"] as? [String: Any] {
                let spawn = subagent["thread_spawn"] as? [String: Any]
                isInternal = subagent["other"] as? String == "guardian"
                parentThreadID = payload["parent_thread_id"] as? String
                    ?? spawn?["parent_thread_id"] as? String
                agentPath = (spawn?["agent_path"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                agentName = [spawn?["agent_nickname"] as? String, payload["agent_nickname"] as? String,
                             agentPath.map { URL(fileURLWithPath: $0).lastPathComponent }].compactMap { $0 }.first { !$0.isEmpty }
            }
            // CLI launched from Desktop can inherit its originator; the rollout's source is authoritative. Otherwise the
            // originator names the client, and one the vendor catalog does not name is shown as written.
            if source == "cli" { client = "CLI" }
            else if source == "exec" { client = "CLI · exec" }
            else if let origin, !origin.isEmpty {
                if let named = VendorCatalog.client(origin, vendor: "Codex") { client = named }
                else { client = origin; VendorCatalog.noteUnnamed(origin, kind: "Codex client") }
            }
            else if source == "vscode" { client = "IDE" }
            return
        }
        if type == "turn_context" {
            if let value = payload["model"] as? String { model = value }
            // Who reviews the turn is asked when it wants an approval; the record that says so passes here as the turn begins.
            if let turn = payload["turn_id"] as? String, !turn.isEmpty {
                CodexTurnReviewers.shared.note(turn: turn, reviewer: payload["approvals_reviewer"] as? String, at: timestamp)
            }
            return
        }
        // Forks copy earlier history. Read its cumulative baseline but do not count it again.
        let inherited = timestamp < (startedAt ?? .distantPast)
            || inheritedThroughOrdinal.map { boundary in (object["ordinal"] as? Int).map { $0 <= boundary } ?? false } == true
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
            sample.offset = offset
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
                    var started = activity ?? SessionActivity()
                    started.startTurn()
                    started.line = .prompt
                    activity = started
                }
            case "task_complete":
                lastActivityAt = max(lastActivityAt ?? timestamp, timestamp)
                // Codex also closes a turn that failed (a usage limit, for one) with `task_complete`, and records the
                // failure as a structured `error`. That turn ended without an answer: like an abort, it is no completion.
                let failed = Self.carriesError(payload["error"])
                let finished = finishTurn(payload["turn_id"] as? String, state: failed ? .ended : .completed,
                                          outcome: failed ? Self.outcome(ofError: payload["error"]) : .completed, at: timestamp)
                if !failed, let id, !isSubagent, !isInternal {
                    let answer = (payload["last_agent_message"] as? String).flatMap {
                        $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0
                    } ?? finished?.finalMessage
                    let completion = SessionCompletion(sessionID: id, vendor: "Codex",
                        turnID: payload["turn_id"] as? String ?? finished?.id ?? String(RecordCoding.milliseconds(timestamp)),
                        task: task ?? cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Codex",
                        model: model, startedAt: finished?.startedAt, completedAt: timestamp,
                        message: answer)
                    if completions?.contains(where: { $0.id == completion.id }) != true {
                        completions = (completions ?? []) + [completion]
                    }
                }
            case "turn_aborted":
                lastActivityAt = max(lastActivityAt ?? timestamp, timestamp)
                // Only the user's own stop says why; any other abort ends the turn without a reason.
                _ = finishTurn(payload["turn_id"] as? String, state: .ended,
                               outcome: payload["reason"] as? String == "interrupted" ? .interrupted : nil, at: timestamp)
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
                if payload["phase"] as? String == "final_answer" || payload["channel"] as? String == "final",
                   let text = payload["message"] as? String {
                    recordFinalMessage(text, turnID: payload["turn_id"] as? String, at: timestamp)
                }
            case "item_completed":
                // Current rollouts record a prompt only as a completed UserMessage item of text and image parts.
                guard let item = payload["item"] as? [String: Any], let content = item["content"] as? [[String: Any]] else { break }
                if task == nil, item["type"] as? String == "UserMessage" {
                    task = content.lazy.filter { $0["type"] as? String == "text" }.compactMap { ($0["text"] as? String).flatMap(SessionTitle.from) }.first
                } else if item["type"] as? String == "AgentMessage", item["phase"] as? String == "final_answer" {
                    let text = content.filter { $0["type"] as? String == "Text" }.compactMap { $0["text"] as? String }.joined(separator: "\n\n")
                    recordFinalMessage(text, turnID: payload["turn_id"] as? String, at: timestamp)
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

    /// Hands the usage read since the last call to the ledger, keyed by its position in the rollout.
    mutating func drainUsage() -> [UsageLedger.Event] {
        defer {
            recordedUsage += usage.count
            usage = []
        }
        return usage.enumerated().map { index, sample in
            // A line's place in the rollout keys it whichever page is read first; a sample without one keeps its count.
            UsageLedger.Event(key: sample.offset.map { "o\($0)" } ?? "u\(recordedUsage + index)", timestamp: sample.timestamp,
                              agentId: "codex-model:\(sample.model)",
                              tokensIn: sample.input, tokensOut: sample.output, cacheReadTokens: sample.cachedInput,
                              cacheWriteTokens: sample.cacheWrite, reasoningTokens: sample.reasoning, contextWindow: sample.contextWindow)
        }
    }

    /// Takes what a rollout's first lines say about its session: its metadata, which also marks the history a fork
    /// copied, and its first prompt as the title, counting none of their usage. A page of history read later carries these
    /// before its own lines.
    mutating func readHead(_ lines: Data) {
        for line in lines.split(separator: 0x0A) {
            let head = line.prefix(512)
            if head.range(of: Self.metadataMarker) != nil
                || (task == nil && (head.range(of: Self.userMessageMarker) != nil
                    || (head.range(of: Self.itemCompletedMarker) != nil && line.range(of: Self.userItemMarker) != nil))) {
                ingest(line)
            }
        }
    }
    private static let metadataMarker = Data("\"session_meta\"".utf8)
    private static let userMessageMarker = Data("\"user_message\"".utf8)
    private static let itemCompletedMarker = Data("\"item_completed\"".utf8)
    private static let userItemMarker = Data("\"UserMessage\"".utf8)

    /// Takes the running totals a `token_count` record carries, counting nothing: a page read on its own starts from the
    /// totals before it, as a read from the start would, so a record that only repeats them counts nothing here either.
    mutating func seed(totalsFrom line: Data) {
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              let info = (object["payload"] as? [String: Any])?["info"] as? [String: Any],
              let totals = info["total_token_usage"] as? [String: Any],
              let input = totals["input_tokens"] as? Int, let output = totals["output_tokens"] as? Int else { return }
        totalInput = input
        totalOutput = output
        totalCached = totals["cached_input_tokens"] as? Int ?? 0
        totalWrite = totals["cache_write_input_tokens"] as? Int ?? 0
        totalReasoning = totals["reasoning_output_tokens"] as? Int ?? 0
        hasTotals = true
    }

    /// Takes in what an older page of the same rollout read: its counts and models add up, its first prompt names the
    /// session, and its turns and completions come before this summary's own; the newest model and running totals stay.
    mutating func merge(older: CodexTranscript) {
        id = id ?? older.id
        cwd = cwd ?? older.cwd
        startedAt = [startedAt, older.startedAt].compactMap { $0 }.min()
        lastActivityAt = [lastActivityAt, older.lastActivityAt].compactMap { $0 }.max()
        task = older.task ?? task
        if model == "Unknown" { model = older.model }
        inputTokens += older.inputTokens
        outputTokens += older.outputTokens
        cachedInputTokens += older.cachedInputTokens
        models.formUnion(older.models)
        let ids = Set((turns ?? []).compactMap(\.id))
        let earlier = (older.turns ?? []).filter { $0.id == nil || !ids.contains($0.id!) }
        if !earlier.isEmpty || turns != nil { turns = Array((earlier + (turns ?? [])).suffix(32)) }
        if let done = older.completions {
            let known = Set((completions ?? []).map(\.id))
            completions = done.filter { !known.contains($0.id) } + (completions ?? [])
        }
    }

    /// Hands over the turn starts and compactions read since the last call.
    mutating func drainMarks() -> [UsageLedger.Mark] {
        defer { marks = [] }
        return marks
    }

    /// Whether this rollout has its session in flight at `now`, by the rollout rule of `SessionPhase.read(_:rule:at:)`.
    /// Quiet counts from the rollout's newest event, so `modifiedAt` is not read; nor are `freshness` and
    /// `abandonedAfter`, since the limits are `SessionPhase.Limits`.
    public func isLive(now: Date, modifiedAt: Date, freshness: TimeInterval = SessionPhase.Limits.quiet,
                       abandonedAfter: TimeInterval = UsageRefresh.abandonedTurnTimeout) -> Bool {
        SessionPhase.read(evidence, rule: .rollout, at: now).inFlight
    }

    /// What this rollout says about its session: its newest turn, one without an id included, and its newest event. A
    /// guardian's rollout, and one without any activity, recorded nothing that counts.
    var evidence: SessionPhase.SourceEvidence {
        SessionPhase.SourceEvidence(turn: turns?.last.map { turn in
            SessionTurn(provider: "codex", sessionID: id ?? "", turnID: turn.id ?? "", state: turn.state,
                        startedAtMs: turn.startedAt.map(RecordCoding.milliseconds),
                        observedAtMs: RecordCoding.milliseconds(turn.observedAt), message: turn.message, outcome: turn.outcome)
        }, lastWriteAt: isInternal ? nil : lastEventAt)
    }

    /// The rollout's newest event: the latest of its counted activity and its newest turn's last event.
    var lastEventAt: Date? { lastActivityAt.map { max($0, turns?.last?.observedAt ?? $0) } }

    public var sessionTurns: [SessionTurn] {
        guard let id, !isInternal else { return [] }
        return (turns ?? []).compactMap { turn in
            guard let turnID = turn.id, !turnID.isEmpty else { return nil }
            return SessionTurn(provider: "codex", sessionID: id, turnID: turnID, state: turn.state,
                startedAtMs: turn.startedAt.map(RecordCoding.milliseconds), observedAtMs: RecordCoding.milliseconds(turn.observedAt),
                message: turn.message, outcome: turn.outcome)
        }
    }

    private mutating func recordFinalMessage(_ text: String, turnID: String?, at date: Date) {
        let index = turnID.flatMap { id in turns?.lastIndex(where: { $0.id == id }) } ?? (turnID == nil ? turns?.indices.last : nil)
        guard let index, turns?[index].state == .running,
              date >= (turns?[index].startedAt ?? .distantPast) else { return }
        if let paragraph = SessionCompletion.lastParagraph(text) { turns?[index].finalMessage = paragraph }
    }

    /// Whether an event's `error` field reports a failure: present, not JSON null and not an empty string.
    private static func carriesError(_ value: Any?) -> Bool {
        guard let value, !(value is NSNull) else { return false }
        return (value as? String)?.isEmpty != true
    }

    /// How a turn's structured error ended it, by the kind Codex names in `codex_error_info`: a usage limit, which lifts
    /// when its window resets, or any other failure, such as an overloaded server.
    static func outcome(ofError value: Any?) -> TurnOutcome {
        let info = (value as? [String: Any])?["codex_error_info"]
        let kind = info as? String ?? (info as? [String: Any])?.keys.first
        return kind == "usage_limit_exceeded" ? .limited(resetsAt: nil) : .failed
    }

    private mutating func finishTurn(_ id: String?, state: SessionTurn.State, outcome: TurnOutcome?, at date: Date) -> Turn? {
        if let index = turns?.lastIndex(where: { $0.id == id }) {
            guard date >= turns![index].observedAt else { return nil }
            // A terminal observation is final. Repeated lines do not change its timestamp or outcome.
            guard turns![index].state == .running else { return turns![index] }
            turns?[index].state = state; turns?[index].observedAt = date; turns?[index].outcome = outcome
            if index == turns!.indices.last { activity?.endTurn() }
            return turns![index]
        }
        // A terminal-only log still identifies its turn, but cannot provide a start time.
        guard turns?.last?.state != .running else { return nil }
        var value = Turn(id: id, startedAt: nil, state: state, observedAt: date)
        value.outcome = outcome
        turns = Array(((turns ?? []) + [value]).suffix(32))
        return value
    }

    /// A call the model made, or its output, while a turn runs: an open call says what the turn does until its output
    /// comes back. Only the line's first bytes are read, but for a function call's id, which follows its arguments.
    private mutating func noteCall(_ line: Data, head: Data) {
        guard let timestamp = Self.timestamp(in: head), timestamp >= (startedAt ?? .distantPast),
              let index = turns?.indices.last, turns?[index].state == .running,
              let type = Self.string(after: Self.payloadType, in: head) else { return }
        // A local shell call has no name: it is the shell.
        let isCall = type == "function_call" || type == "custom_tool_call" || type == "local_shell_call"
        guard isCall || type == "function_call_output" || type == "custom_tool_call_output",
              let id = Self.string(after: Self.callID, in: line) else { return }
        if timestamp > turns![index].observedAt { turns?[index].observedAt = timestamp }
        var activity = self.activity ?? SessionActivity()
        if isCall {
            let name = type == "local_shell_call" ? "shell" : Self.string(after: Self.callName, in: head)
            activity.open(id, work: name.flatMap { Self.work(ofCall: $0, namespace: Self.string(after: Self.callNamespace, in: head)) },
                          at: timestamp)
            activity.line = .call
        } else {
            activity.close(id, at: timestamp)
            activity.line = .result
        }
        self.activity = activity
    }

    /// Ends one read of the rollout (`SessionActivity.settle()`).
    mutating func settleActivity() { activity?.settle() }

    /// The tools Codex starts, messages and waits on its sub-agents with.
    static let collaborationCalls: Set<String> = ["spawn_agent", "wait_agent", "send_message", "followup_task", "list_agents", "wait",
                                                  "interrupt_agent"]

    /// What a Codex call does, by its tool's name: its shell, its other commands and MCP tools, its sub-agents and its
    /// questions; the tools of older rollouts that edit or view files too. Nil for any other tool.
    static func work(ofCall name: String, namespace: String?) -> SessionActivity.Work? {
        switch name {
        case "exec", "shell", "shell_command", "exec_command", "write_stdin": return .shell
        case "js", "js_reset": return .command
        case "apply_patch": return .edit
        case "view_image": return .read
        case "request_user_input_async": return .question
        case _ where collaborationCalls.contains(name): return .agents
        case _ where name.hasPrefix("mcp__") || namespace?.hasPrefix("mcp__") == true: return .command
        default: return nil
        }
    }

    /// The JSON string that follows `key` in `data`, without unescaping: ids and names never carry escapes.
    private static func string(after key: Data, in data: Data) -> String? {
        guard let start = data.range(of: key)?.upperBound,
              let end = data[start...].firstIndex(of: UInt8(ascii: "\"")) else { return nil }
        return String(decoding: data[start..<end], as: UTF8.self)
    }

    private static let responseItem = Data(#""type":"response_item""#.utf8)
    /// A response item's type is the first thing Codex writes of its payload.
    private static let payloadType = Data(#""payload":{"type":""#.utf8)
    private static let callID = Data(#""call_id":""#.utf8)
    private static let callName = Data(#""name":""#.utf8)
    private static let callNamespace = Data(#""namespace":""#.utf8)
    private static let fileChangeItem = Data(#""item":{"type":"FileChange""#.utf8)
    private static let imageViewItem = Data(#""item":{"type":"ImageView""#.utf8)
    private static let itemCompleted = Data(#""type":"item_completed""#.utf8)
    /// The item's type is the first thing Codex writes of it; a line written otherwise is decoded whole.
    private static let itemType = Data(#""item":{"type":""#.utf8)
    private static let userItem = Data(#""item":{"type":"UserMessage""#.utf8)
    private static let agentItem = Data(#""item":{"type":"AgentMessage""#.utf8)
    private static let timestampKey = Data(#""timestamp":""#.utf8)

    /// The line's own time, written first.
    private static func timestamp(in head: Data) -> Date? {
        guard let key = head.range(of: timestampKey),
              let end = head[key.upperBound...].firstIndex(of: UInt8(ascii: "\"")) else { return nil }
        return ISO8601Fast.parse(String(decoding: head[key.upperBound..<end], as: UTF8.self))
    }

    private static let envelopes = ["\"session_meta\"", "\"turn_context\"", "\"event_msg\"", "\"type\":\"compacted\""].map { Data($0.utf8) }
}

/// Cooperative tail reader over the usage ledger. A partially written final line is retried on the next poll.
/// Each rollout is one ledger contribution; when a session's rollout exists in several places, only its newest copy counts.
/// A rollout Codex compressed stays the same contribution under its `.jsonl.zst` name.
public actor CodexTranscriptStore {
    public struct Session: Sendable {
        public let transcript: CodexTranscript
        public let modifiedAt: Date
        public let path: String
        public let title: String?
        /// Whether Codex archived the thread: its rollout, the copy that counts, lies in `archived_sessions`. Its usage
        /// counts as any other's; it is left out of the session list.
        public var archived = false
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
        logs = TailLogStore(roots: roots, ledger: ledger, watchesChanges: watchesChanges, accepts: CodexRollouts.accepts)
    }

    public static func standard(directory: URL = CodexLocator.dataDirectory, ledger: UsageLedger = .inMemory()) -> CodexTranscriptStore {
        CodexTranscriptStore(roots: [directory.appendingPathComponent("sessions"), directory.appendingPathComponent(archiveFolder)],
                             indexURL: directory.appendingPathComponent("session_index.jsonl"), ledger: ledger, watchesChanges: true)
    }

    /// The folder, beside `sessions`, Codex moves the rollouts of the threads it archives into, flat, sub-agents' with
    /// their parent's; unarchiving moves them back.
    static let archiveFolder = "archived_sessions"

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
            sessions[id] = Session(transcript: entry.summary, modifiedAt: log.file.modified, path: log.path, title: titles[id],
                                   archived: URL(fileURLWithPath: log.path).deletingLastPathComponent().lastPathComponent == Self.archiveFolder)
        }
        return Result(sessions: Array(sessions.values),
                      indexing: pass.pending > 0 ? IndexProgress(done: pass.logs.count - pass.pending, total: pass.logs.count) : nil)
    }

    /// Thread names, read again only when the index file changed.
    private func readTitles() -> [String: String] {
        guard var indexURL else { return [:] }
        indexURL.removeAllCachedResourceValues()
        guard let values = try? indexURL.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey]) else { return [:] }
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
    /// 5: keep the first valid metadata, so copied parent history cannot replace a spawned agent's identity. 6: read from
    /// the newest turn first and back through the history; usage is keyed by its place in the rollout.
    static let version = 6

    static func summary(for url: URL) -> CodexTranscript { CodexTranscript() }

    static func ingest(_ lines: Data, at offset: Int, into transcript: inout CodexTranscript) -> [UsageLedger.Event] {
        var position = offset
        for line in lines.split(separator: 0x0A, omittingEmptySubsequences: false) {
            defer { position += line.count + 1 }
            if !line.isEmpty { transcript.ingest(line, at: position) }
        }
        transcript.settleActivity()
        return transcript.drainUsage()
    }

    /// A turn starts where Codex starts a task, whether the user's prompt or a phone's started it.
    static let startsTurn: LogPager.StartsTurn? = { prefix, _ in prefix.range(of: taskStarted) != nil }
    private static let taskStarted = Data(#""type":"task_started""#.utf8)

    static func head(_ lines: Data, into transcript: inout CodexTranscript) { transcript.readHead(lines) }

    static func merge(older: CodexTranscript, into transcript: inout CodexTranscript) { transcript.merge(older: older) }

    /// Codex counts a call as the change of its running totals, which `token_count` records carry.
    static let carriesState: (@Sendable (Data) -> Bool)? = { prefix in prefix.range(of: tokenCount) != nil }
    private static let tokenCount = Data(#""type":"token_count""#.utf8)

    static func seed(_ line: Data, into transcript: inout CodexTranscript) { transcript.seed(totalsFrom: line) }

    static func drainMarks(_ transcript: inout CodexTranscript) -> [UsageLedger.Mark] { transcript.drainMarks() }

    static func group(_ transcript: CodexTranscript) -> String? { transcript.id ?? "" }

    /// Codex compresses a rollout it has not written for a week into `<rollout>.jsonl.zst`, and turns it back into the
    /// plain rollout before it writes to that session again.
    static let compressedSuffix: String? = ".zst"

    /// A rollout, plain or compressed.
    static func accepts(_ url: URL) -> Bool {
        let name = url.lastPathComponent
        return name.hasPrefix("rollout-") && (name.hasSuffix(".jsonl") || name.hasSuffix(".jsonl.zst"))
    }
}
