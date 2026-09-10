import Foundation

/// Summary of Harness v0 session logs. Only metadata and token counters survive indexing.
public struct DeepSeekTranscript: Codable, Sendable {
    public struct Usage: Codable, Sendable {
        public let timestamp: Date
        public let requestedAt: Date
        public let provider: String
        public let model: String
        public let input: Int
        public let cachedInput: Int
        public let output: Int

        public var event: TranscriptSession.UsageEvent {
            .init(timestamp: timestamp, agentId: "deepseek-model:\(model)", tokensIn: input, tokensOut: output, cacheReadTokens: cachedInput)
        }
    }

    public private(set) var id: String?
    public private(set) var cwd: String?
    public private(set) var startedAt: Date?
    public private(set) var lastActivityAt: Date?
    public private(set) var title: String?
    public private(set) var model = "Unknown"
    private var provider = "Unknown"
    private var requestedAt: Date?
    public private(set) var isSubagent = false
    public private(set) var usage: [Usage] = []
    private var seedLength = 0
    private var turnActive = false
    private var turnStartedAt: Date?
    public private(set) var completions: [SessionCompletion]?
    private var lastAttempt: Attempt?

    private struct Attempt: Codable {
        let turn: Int
        let step: Int
        let index: Int
    }

    public init() {}

    public mutating func ingest(_ line: Data) throws {
        // Packed text/reasoning/tool-call deltas have no accounting or lifecycle data.
        guard let object = try JSONSerialization.jsonObject(with: line) as? [String: Any],
              let type = object["type"] as? String else { return }
        if type == "session" {
            guard object["version"] as? Int == 0 else {
                throw UsageProviderError(L10n.text("不支持此 Harness 会话格式", "Unsupported Harness session format"))
            }
            guard let sessionId = object["id"] as? String, let created = object["createdAt"] as? Double else {
                throw UsageProviderError(L10n.text("Harness 会话头无效", "Invalid Harness session header"))
            }
            id = sessionId; cwd = object["cwd"] as? String
            startedAt = Date(timeIntervalSince1970: created / 1000)
            isSubagent = object["origin"] as? String == "subagent" || (object["delegationDepth"] as? Int ?? 0) > 0
            seedLength = object["seedLength"] as? Int ?? 0
            return
        }
        guard id != nil, let seq = object["seq"] as? Int,
              let milliseconds = object["time"] as? Double, let data = object["data"] as? [String: Any] else { return }
        // Inherited requests establish the model but never contribute the parent's tokens or liveness.
        if type == "request/header", let config = (data["header"] as? [String: Any])?["config"] as? [String: Any],
           let value = config["model"] as? String {
            model = value
            provider = config["provider"] as? String ?? "Unknown"
        }
        if type == "request/context", let value = data["model"] as? String {
            model = value
            provider = data["provider"] as? String ?? "Unknown"
        }
        guard seq >= seedLength else { return }
        let timestamp = Date(timeIntervalSince1970: milliseconds / 1000)
        // A rename or seed marker must not make a finished task look active again.
        if type != "session/title" && type != "session/end-seed" { lastActivityAt = timestamp }
        switch type {
        case "turn/start": turnActive = true; turnStartedAt = timestamp
        case "step/start": requestedAt = timestamp
        case "turn/end":
            turnActive = false
            if !isSubagent, (data["reason"] as? [String: Any])?["kind"] as? String == "completed", let id {
                let completion = SessionCompletion(sessionID: "deepseek:\(id)", vendor: "DeepSeek", turnID: String(seq),
                    task: title ?? cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "DeepSeek Harness",
                    model: model, startedAt: turnStartedAt, completedAt: timestamp)
                if completions?.contains(where: { $0.id == completion.id }) != true {
                    completions = (completions ?? []) + [completion]
                }
            }
            turnStartedAt = nil
        case "session/end-seed": turnActive = false; turnStartedAt = nil
        case "session/title":
            if let text = data["title"] as? String { title = ClaudeTranscriptParser.title(from: text) }
        case "user/message":
            if title == nil, (data["source"] as? [String: Any])?["kind"] as? String == "user",
               let content = data["content"] as? [[String: Any]],
               let text = content.first(where: { $0["type"] as? String == "text" })?["text"] as? String {
                title = ClaudeTranscriptParser.title(from: text)
            }
        case "llm/retry-started":
            if lastAttempt?.turn == data["turn"] as? Int && lastAttempt?.step == data["step"] as? Int { lastAttempt = nil }
            requestedAt = timestamp
        case "assistant/chunk", "assistant/message":
            let counters: [String: Any]?
            if type == "assistant/chunk" {
                let chunk = data["chunk"] as? [String: Any]
                counters = chunk?["type"] as? String == "usage" ? chunk?["usage"] as? [String: Any] : nil
            } else { counters = data["usage"] as? [String: Any] }
            guard let counters, let input = counters["inputTokens"] as? Int, input >= 0,
                  let output = counters["outputTokens"] as? Int, output >= 0,
                  let turn = data["turn"] as? Int, let step = data["step"] as? Int else { return }
            // Harness already excludes cache hits from inputTokens. Match HUD's fresh-input convention.
            // Reasoning is included in outputTokens; chunks and their final message report one attempt.
            let sample = Usage(timestamp: timestamp, requestedAt: requestedAt ?? timestamp, provider: provider, model: model,
                               input: input + max(0, counters["cacheWriteTokens"] as? Int ?? 0),
                               cachedInput: max(0, counters["cacheReadTokens"] as? Int ?? 0), output: output)
            if let previous = lastAttempt, previous.turn == turn, previous.step == step {
                usage[previous.index] = sample
            } else {
                lastAttempt = Attempt(turn: turn, step: step, index: usage.count)
                usage.append(sample)
            }
        default: break
        }
    }

    public func isLive(now: Date, modifiedAt: Date, freshness: TimeInterval = 120,
                       processStarts: [Date] = []) -> Bool {
        guard turnActive, lastActivityAt != nil else { return false }
        // Questions and long tools can leave an open turn quiet. A newer process cannot own an older turn.
        if let turnStartedAt, processStarts.contains(where: { $0 <= turnStartedAt }) { return true }
        return now.timeIntervalSince(modifiedAt) < freshness
    }
}
