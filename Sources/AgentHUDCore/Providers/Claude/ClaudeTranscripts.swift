import AgentHUDSupport
import Foundation

/// One line of a Claude Code transcript (`~/.claude/projects/**/*.jsonl`).
public struct TranscriptEvent: Hashable, Sendable {
    public enum Role: String, Sendable {
        case user, assistant, other
    }

    public let timestamp: Date
    public let role: Role
    public let model: String?
    public let inputTokens: Int
    public let cacheCreationTokens: Int
    public let cacheReadTokens: Int
    public let outputTokens: Int
    /// First text of a user message (used for the session title); nil for other lines.
    public let text: String?
    public let sessionId: String?
    public let cwd: String?
    /// API message id; Claude Code writes one line per content block, so usage repeats under the same id.
    public let messageId: String?
    public let requestId: String?
    /// `message.stop_reason` of an assistant line ("end_turn", "tool_use", …); nil for other lines and for a null reason.
    public let stopReason: String?
    /// Sub-agent traffic that older Claude Code builds wrote into the parent transcript.
    public let isSidechain: Bool
    /// A user line that starts a turn: typed or queued input, not a tool result, command output or injected context.
    public let isPrompt: Bool
    /// Surface that wrote the line, from the `entrypoint` current builds stamp on every message ("cli",
    /// "claude-desktop", "claude-vscode", "sdk-ts"); nil for older builds.
    public let entrypoint: String?

    public init(
        timestamp: Date, role: Role, model: String?, inputTokens: Int, cacheCreationTokens: Int, cacheReadTokens: Int,
        outputTokens: Int, text: String?, sessionId: String?, cwd: String?, messageId: String? = nil, requestId: String? = nil,
        stopReason: String? = nil, isSidechain: Bool = false, isPrompt: Bool = false, entrypoint: String? = nil
    ) {
        self.timestamp = timestamp
        self.role = role
        self.model = model
        self.inputTokens = inputTokens
        self.cacheCreationTokens = cacheCreationTokens
        self.cacheReadTokens = cacheReadTokens
        self.outputTokens = outputTokens
        self.text = text
        self.sessionId = sessionId
        self.cwd = cwd
        self.messageId = messageId
        self.requestId = requestId
        self.stopReason = stopReason
        self.isSidechain = isSidechain
        self.isPrompt = isPrompt
        self.entrypoint = entrypoint
    }

    /// Key used to count each API response once.
    public var usageKey: String? {
        if let messageId { return "m:" + messageId }
        if let requestId { return "r:" + requestId }
        return nil
    }

    /// Fresh input (prompt + cache writes); cache reads are excluded because they are billed differently.
    public var tokensIn: Int { inputTokens + cacheCreationTokens }
    public var hasUsage: Bool { role == .assistant && (tokensIn + cacheReadTokens + outputTokens) > 0 }
}

public enum ClaudeTranscriptParser {
    public static func parseLine(_ line: String) -> TranscriptEvent? {
        guard line.hasPrefix("{"), let data = line.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let timestamp = (object["timestamp"] as? String).flatMap(DateParsing.iso8601)
        else { return nil }
        let message = object["message"] as? [String: Any]
        let roleName = (message?["role"] as? String) ?? (object["type"] as? String)
        let role: TranscriptEvent.Role
        switch roleName {
        case "user": role = .user
        case "assistant": role = .assistant
        default: role = .other
        }
        let usage = message?["usage"] as? [String: Any]
        func count(_ key: String) -> Int {
            if let value = usage?[key] as? Int { return value }
            if let value = usage?[key] as? Double { return Int(value) }
            return 0
        }
        var text: String?
        var isToolResult = false
        if role == .user, let content = message?["content"] {
            if let string = content as? String {
                text = string
            } else if let blocks = content as? [[String: Any]] {
                text = blocks.first { ($0["type"] as? String) == "text" }?["text"] as? String
                isToolResult = blocks.contains { ($0["type"] as? String) == "tool_result" }
            }
        }
        let isMeta = object["isMeta"] as? Bool ?? false
        return TranscriptEvent(
            timestamp: timestamp,
            role: role,
            model: message?["model"] as? String,
            inputTokens: count("input_tokens"),
            cacheCreationTokens: count("cache_creation_input_tokens"),
            cacheReadTokens: count("cache_read_input_tokens"),
            outputTokens: count("output_tokens"),
            text: text,
            sessionId: object["sessionId"] as? String,
            cwd: object["cwd"] as? String,
            messageId: message?["id"] as? String,
            requestId: object["requestId"] as? String,
            stopReason: role == .assistant ? message?["stop_reason"] as? String : nil,
            isSidechain: object["isSidechain"] as? Bool ?? false,
            isPrompt: role == .user && !isMeta && !isToolResult,
            entrypoint: object["entrypoint"] as? String
        )
    }

    public static func parse(_ text: String) -> [TranscriptEvent] {
        text.split(separator: "\n", omittingEmptySubsequences: true).compactMap { parseLine(String($0)) }
    }

    /// Session title: first real user prompt, first line, trimmed to 60 characters. Skips slash-command/meta lines.
    public static func title(from text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !trimmed.hasPrefix("<"), !trimmed.hasPrefix("[Request interrupted") else { return nil }
        let firstLine = trimmed.split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: true).first.map(String.init) ?? trimmed
        let collapsed = firstLine.trimmingCharacters(in: .whitespaces)
        guard !collapsed.isEmpty else { return nil }
        return collapsed.count > 60 ? String(collapsed.prefix(59)) + "…" : collapsed
    }
}

public enum ClaudeModelMapper {
    /// Maps an API model id to a family row id ("claude-opus" / "claude-sonnet" / "claude-fable" / "claude-haiku").
    public static func agentId(for model: String?) -> String? {
        guard let model, !model.isEmpty, model != "<synthetic>" else { return nil }
        return "claude-model:\(model)"
    }
}

/// Which Claude Code surface drives a session, from the transcript's `entrypoint` (`CLAUDE_CODE_ENTRYPOINT`).
/// The label is stored as the session's client, so synced peers and the iOS app show it without knowing the ids.
public enum ClaudeEntrypoint {
    /// Builds before the field existed, and ids this build does not know.
    public static let defaultLabel = "Claude Code"

    public static func clientLabel(_ entrypoint: String?) -> String {
        switch entrypoint {
        case "cli": return "Claude Code CLI"
        case "claude-desktop": return "Claude Code Desktop"
        case "claude-vscode": return "Claude Code IDE extension"
        case let sdk? where sdk.hasPrefix("sdk-"): return "Claude Agent SDK"
        default: return defaultLabel
        }
    }
}

/// Compact summary of one transcript file; grows incrementally as the file is appended to.
public struct TranscriptSession: Hashable, Sendable, Identifiable {
    public struct UsageEvent: Hashable, Codable, Sendable {
        public let timestamp: Date
        public let agentId: String
        public let tokensIn: Int
        public let tokensOut: Int
        public let cacheReadTokens: Int
        /// Provider-owned identity, used when an account-wide event is observed on multiple Macs.
        public let eventID: String?
        /// Overlapping log formats can report the same conversation at different granularity.
        public struct Origin: Hashable, Codable, Sendable {
            public let group: String
            public let priority: Int
            public init(group: String, priority: Int) { self.group = group; self.priority = priority }
        }
        public let origin: Origin?
        public let attribution: UsageAttribution?

        public init(timestamp: Date, agentId: String, tokensIn: Int, tokensOut: Int, cacheReadTokens: Int = 0, eventID: String? = nil, origin: Origin? = nil, attribution: UsageAttribution? = nil) {
            self.timestamp = timestamp
            self.agentId = agentId
            self.tokensIn = tokensIn
            self.tokensOut = tokensOut
            self.cacheReadTokens = cacheReadTokens
            self.eventID = eventID
            self.origin = origin
            self.attribution = attribution
        }

        private enum CodingKeys: String, CodingKey { case timestamp, agentId, tokensIn, tokensOut, cacheReadTokens, eventID, origin, attribution }
        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            timestamp = try c.decode(Date.self, forKey: .timestamp)
            agentId = try c.decode(String.self, forKey: .agentId)
            tokensIn = try c.decode(Int.self, forKey: .tokensIn)
            tokensOut = try c.decode(Int.self, forKey: .tokensOut)
            cacheReadTokens = try c.decodeIfPresent(Int.self, forKey: .cacheReadTokens) ?? 0
            eventID = try c.decodeIfPresent(String.self, forKey: .eventID)
            origin = try c.decodeIfPresent(Origin.self, forKey: .origin)
            attribution = try c.decodeIfPresent(UsageAttribution.self, forKey: .attribution)
        }

        public var total: Int { tokensIn + tokensOut }
    }

    public let id: String
    public let path: String
    public let cwd: String?
    public let isSubagent: Bool
    public let startedAt: Date
    public let lastActivityAt: Date
    public let task: String?
    /// Usage events with exact model identities; missing model names remain explicitly unknown.
    public let usage: [UsageEvent]
    public let dominantAgentId: String
    /// Raw model ids seen with their last timestamp, for model discovery.
    public let modelsSeen: [String: Date]
    /// `entrypoint` of the newest line: a transcript resumed from another surface keeps its id but changes client.
    public let entrypoint: String?
    /// Recent explicit turn ends of a main session, newest last; sub-agent transcripts never report any.
    public var completions: [SessionCompletion] = []
    /// True after a prompt or a working assistant, false after `end_turn` or an interruption, nil when never observed.
    public var turn: SessionTurn? = nil
    public var turnInProgress: Bool? { turn.map { $0.state == .running } }

    public var tokensIn: Int { usage.reduce(0) { $0 + $1.tokensIn } }
    public var tokensOut: Int { usage.reduce(0) { $0 + $1.tokensOut } }
    public var cacheReadTokens: Int { usage.reduce(0) { $0 + $1.cacheReadTokens } }

    /// Running means a turn is in progress and the log is still being written. A session waiting for input
    /// stops right away; a crashed or stalled process stops once the log has been quiet for `threshold`.
    public func isLive(now: Date, threshold: TimeInterval) -> Bool {
        turnInProgress != false && now.timeIntervalSince(lastActivityAt) < threshold
    }

    /// Tokens consumed at or after `since`.
    public func tokens(since: Date) -> Int {
        usage.reduce(0) { $0 + ($1.timestamp >= since ? $1.total : 0) }
    }
}

/// Mutable builder that ingests events (possibly in several batches) and produces a `TranscriptSession`.
/// Codable so the transcript store can persist it between runs.
public struct TranscriptAccumulator: Hashable, Sendable, Codable {
    private struct RawUsage: Hashable, Sendable, Codable {
        let timestamp: Date
        // A required field invalidates old family-only caches, which must be reindexed from the logs.
        let consumerId: String
        let tokensIn: Int
        let tokensOut: Int
        let cacheReadTokens: Int
    }

    public let path: String
    public let isSubagent: Bool
    private var sessionId: String?
    private var cwd: String?
    private var startedAt: Date?
    private var lastActivityAt: Date?
    private var task: String?
    private var rawUsage: [RawUsage] = []
    private var outputByAgent: [String: Int] = [:]
    private var seenUsageKeys: Set<String> = []
    private var modelsSeen: [String: Date] = [:]
    private var entrypoint: String?
    private struct Turn: Hashable, Sendable, Codable {
        let startedAt: Date?
        var state: SessionTurn.State
        var observedAt: Date
    }
    private var currentTurn: Turn?
    private var completions: [SessionCompletion]?

    /// A response that neither requests a tool nor was cut off ends the turn and returns control to the user.
    /// Interruptions leave no such line; API errors are synthetic messages without one.
    static let completedStopReasons: Set<String> = ["end_turn", "stop_sequence"]
    /// Enough for every turn a poll can observe; the notification tracker ignores older ones anyway.
    static let retainedCompletions = 32

    public init(path: String, isSubagent: Bool) {
        self.path = path
        self.isSubagent = isSubagent
    }

    public mutating func ingest(_ events: [TranscriptEvent]) {
        for event in events {
            if sessionId == nil { sessionId = event.sessionId }
            if cwd == nil { cwd = event.cwd }
            if let entrypoint = event.entrypoint { self.entrypoint = entrypoint }
            startedAt = min(startedAt ?? event.timestamp, event.timestamp)
            lastActivityAt = max(lastActivityAt ?? event.timestamp, event.timestamp)
            if task == nil, event.role == .user, let text = event.text {
                task = ClaudeTranscriptParser.title(from: text)
            }
            if !isSubagent, !event.isSidechain {
                if event.isPrompt {
                    // Claude Code records an interruption as a user line; that turn is over without a completion.
                    let interrupted = event.text?.hasPrefix("[Request interrupted") == true
                    if event.timestamp >= (currentTurn?.observedAt ?? .distantPast) {
                        currentTurn = Turn(startedAt: interrupted ? currentTurn?.startedAt : event.timestamp,
                            state: interrupted ? .ended : .running, observedAt: event.timestamp)
                    }
                } else if event.role == .assistant, event.model != "<synthetic>" {
                    if let reason = event.stopReason, Self.completedStopReasons.contains(reason) {
                        let completionID = RecordCoding.hash(["Claude", sessionId ?? URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent,
                            event.messageId ?? String(RecordCoding.milliseconds(event.timestamp))])
                        let duplicate = completions?.contains { $0.id == completionID } == true
                        recordCompletion(event)
                        if !duplicate, event.timestamp >= (currentTurn?.observedAt ?? .distantPast) {
                            currentTurn = Turn(startedAt: currentTurn?.startedAt, state: .completed, observedAt: event.timestamp)
                        }
                    } else if currentTurn == nil {
                        // A partial legacy transcript can show work, but cannot invent a prompt start time.
                        currentTurn = Turn(startedAt: nil, state: .running, observedAt: event.timestamp)
                    }
                }
                if currentTurn?.state == .running, event.timestamp > currentTurn!.observedAt {
                    currentTurn?.observedAt = event.timestamp
                }
            }
            guard event.hasUsage else { continue }
            if let key = event.usageKey {
                // Same API response written as several lines: count its usage once.
                guard seenUsageKeys.insert(key).inserted else { continue }
            }
            let mapped = ClaudeModelMapper.agentId(for: event.model) ?? "claude-model:Unknown"
            rawUsage.append(RawUsage(timestamp: event.timestamp, consumerId: mapped, tokensIn: event.tokensIn, tokensOut: event.outputTokens,
                                     cacheReadTokens: event.cacheReadTokens))
            outputByAgent[mapped, default: 0] += event.outputTokens
            let model = String(mapped.dropFirst("claude-model:".count))
            if (modelsSeen[model] ?? .distantPast) < event.timestamp {
                modelsSeen[model] = event.timestamp
            }
        }
    }

    public var isEmpty: Bool { startedAt == nil }

    private mutating func recordCompletion(_ event: TranscriptEvent) {
        let fileName = URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
        let completion = SessionCompletion(
            sessionID: sessionId ?? fileName, vendor: "Claude",
            turnID: event.messageId ?? String(RecordCoding.milliseconds(event.timestamp)),
            task: task ?? cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Claude Code",
            model: event.model.map { ClaudeModelInfo.parse($0)?.displayName ?? $0 } ?? "Claude",
            startedAt: currentTurn?.state == .running ? currentTurn?.startedAt : nil, completedAt: event.timestamp
        )
        // Claude Code writes one line per content block, and every line carries the message's stop reason.
        if let index = completions?.firstIndex(where: { $0.id == completion.id }), let previous = completions?[index] {
            // Thinking and text blocks share an ID but can have timestamps tens of seconds apart.
            // Preserve the original prompt even when later blocks arrive in a subsequent indexing batch.
            if completion.completedAt > previous.completedAt {
                completions?[index] = SessionCompletion(
                    sessionID: completion.sessionID, vendor: completion.vendor,
                    turnID: event.messageId ?? String(RecordCoding.milliseconds(event.timestamp)),
                    task: previous.task, model: completion.model,
                    startedAt: previous.startedAt, completedAt: completion.completedAt
                )
            }
            return
        }
        completions = Array(((completions ?? []) + [completion]).suffix(Self.retainedCompletions))
    }

    /// Drops the dedupe set and old completions once a file can no longer receive appended lines (Claude Code
    /// writes the duplicate lines of one response back to back, so an old file never needs them again). Keeps
    /// memory and the on-disk cache small for thousands of finished sessions.
    public mutating func compactIfFinished(now: Date, idleFor interval: TimeInterval = 86400) {
        guard let lastActivityAt, now.timeIntervalSince(lastActivityAt) > interval,
              !seenUsageKeys.isEmpty || completions != nil else { return }
        seenUsageKeys.removeAll()
        completions = nil
    }

    public func build() -> TranscriptSession? {
        guard let startedAt, let lastActivityAt else { return nil }
        let dominant = outputByAgent.max { $0.value < $1.value }?.key ?? "claude-model:Unknown"
        let usage = rawUsage.map { raw in
            TranscriptSession.UsageEvent(timestamp: raw.timestamp, agentId: raw.consumerId, tokensIn: raw.tokensIn, tokensOut: raw.tokensOut,
                                         cacheReadTokens: raw.cacheReadTokens)
        }
        let fileName = URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
        return TranscriptSession(
            id: sessionId ?? fileName,
            path: path,
            cwd: cwd,
            isSubagent: isSubagent,
            startedAt: startedAt,
            lastActivityAt: lastActivityAt,
            task: task,
            usage: usage,
            dominantAgentId: dominant,
            modelsSeen: modelsSeen,
            entrypoint: entrypoint,
            completions: completions ?? [],
            turn: currentTurn.map { turn in
                SessionTurn(provider: "claude", sessionID: sessionId ?? fileName,
                    turnID: String(RecordCoding.milliseconds(turn.startedAt ?? turn.observedAt)), state: turn.state,
                    startedAtMs: turn.startedAt.map(RecordCoding.milliseconds), observedAtMs: RecordCoding.milliseconds(turn.observedAt))
            }
        )
    }
}
