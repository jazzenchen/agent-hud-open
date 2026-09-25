import Foundation

/// Where one session's tokens went, as the usage ledger recorded them: by model, by 15-minute period, by turn, and the
/// part its sub-agents spent. It covers what this Mac read of the session within the ledger's retention.
public struct SessionUsage: Hashable, Codable, Sendable {
    public struct Tokens: Hashable, Codable, Sendable {
        public var tokensIn: Int
        public var tokensOut: Int
        public var cacheReadTokens: Int
        /// The part of `tokensIn` written to the prompt cache.
        public var cacheWriteTokens: Int
        /// The part of `tokensOut` spent reasoning.
        public var reasoningTokens: Int

        public init(tokensIn: Int = 0, tokensOut: Int = 0, cacheReadTokens: Int = 0, cacheWriteTokens: Int = 0, reasoningTokens: Int = 0) {
            self.tokensIn = tokensIn; self.tokensOut = tokensOut; self.cacheReadTokens = cacheReadTokens
            self.cacheWriteTokens = cacheWriteTokens; self.reasoningTokens = reasoningTokens
        }

        private enum CodingKeys: String, CodingKey { case tokensIn, tokensOut, cacheReadTokens, cacheWriteTokens, reasoningTokens }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            self.init(tokensIn: try c.decode(Int.self, forKey: .tokensIn), tokensOut: try c.decode(Int.self, forKey: .tokensOut),
                      cacheReadTokens: try c.decode(Int.self, forKey: .cacheReadTokens),
                      cacheWriteTokens: try c.decodeIfPresent(Int.self, forKey: .cacheWriteTokens) ?? 0,
                      reasoningTokens: try c.decodeIfPresent(Int.self, forKey: .reasoningTokens) ?? 0)
        }

        public static func + (lhs: Self, rhs: Self) -> Self {
            Tokens(tokensIn: lhs.tokensIn + rhs.tokensIn, tokensOut: lhs.tokensOut + rhs.tokensOut,
                   cacheReadTokens: lhs.cacheReadTokens + rhs.cacheReadTokens,
                   cacheWriteTokens: lhs.cacheWriteTokens + rhs.cacheWriteTokens, reasoningTokens: lhs.reasoningTokens + rhs.reasoningTokens)
        }

        public static func += (lhs: inout Self, rhs: Self) { lhs = lhs + rhs }

        public var isEmpty: Bool { tokensIn == 0 && tokensOut == 0 && cacheReadTokens == 0 }
        public var kinds: TokenKinds {
            TokenKinds(tokensIn: tokensIn, tokensOut: tokensOut, cacheRead: cacheReadTokens, cacheWrite: cacheWriteTokens, reasoning: reasoningTokens)
        }
        public func count(_ dimensions: TokenDimensions) -> Int { dimensions.count(kinds) }
    }

    public struct Model: Hashable, Codable, Sendable {
        public let agentId: String
        public let tokens: Tokens
        public init(agentId: String, tokens: Tokens) { self.agentId = agentId; self.tokens = tokens }
    }

    /// One model's tokens in one 15-minute period.
    public struct Period: Hashable, Codable, Sendable {
        public let start: Date
        public let agentId: String
        public let tokens: Tokens
        public init(start: Date, agentId: String, tokens: Tokens) { self.start = start; self.agentId = agentId; self.tokens = tokens }

        public var bucket: UsageBucket {
            UsageBucket(start: start, agentId: agentId, tokensIn: tokens.tokensIn, tokensOut: tokens.tokensOut, cacheReadTokens: tokens.cacheReadTokens,
                        cacheWriteTokens: tokens.cacheWriteTokens, reasoningTokens: tokens.reasoningTokens)
        }
    }

    /// One prompt and the model calls it led to, sub-agents' included, until the next prompt.
    public struct Turn: Hashable, Codable, Sendable {
        /// The prompt's time; for calls logged before any prompt, the first of them.
        public let start: Date
        /// The last call.
        public let end: Date
        public let tokens: Tokens
        public let calls: Int
        /// The prompt of the turn's last call in the session's own log: the context the turn ended with.
        public let contextTokens: Int?
        /// The client compacted the conversation during the turn.
        public let compacted: Bool
        /// What sub-agents spent in the turn; already part of `tokens`. Nil when none ran.
        public let subagents: Tokens?
        /// The largest prompt of the turn's calls in the session's own log, when it was above the context the turn ended with.
        public let peakContextTokens: Int?
        /// Prompt tokens the session's own log sent again because the cache no longer held them: its cache lapsed or the
        /// model changed. Nil when no call did.
        public let recachedTokens: Int?
        /// What the turn's calls would cost at the vendors' API list prices, in the unit of the session's `listCost`; nil
        /// when a call's model has no list price.
        public let listCost: Decimal?

        public init(start: Date, end: Date, tokens: Tokens, calls: Int, contextTokens: Int? = nil, compacted: Bool = false,
                    subagents: Tokens? = nil, peakContextTokens: Int? = nil, recachedTokens: Int? = nil, listCost: Decimal? = nil) {
            self.start = start; self.end = end; self.tokens = tokens; self.calls = calls
            self.contextTokens = contextTokens; self.compacted = compacted
            self.subagents = subagents; self.peakContextTokens = peakContextTokens; self.recachedTokens = recachedTokens; self.listCost = listCost
        }
    }

    /// The newest turns kept in a breakdown; `turnCount` still counts the older ones.
    public static let turnLimit = 200

    /// Every model the session and its sub-agents called, the largest first.
    public let models: [Model]
    /// Each model's 15-minute periods with tokens in them, oldest first.
    public let periods: [Period]
    /// What sub-agents spent; already part of `models` and `periods`. Nil when the session started none.
    public let subagents: Tokens?
    /// Model calls recorded.
    public let calls: Int
    /// The prompt of the session's latest model call: fresh input, cache writes and cache reads. Only logs that record
    /// every call say it; other sources leave it nil.
    public let contextTokens: Int?
    /// Estimated price by currency, for sessions of a priced account; a currency is missing when any call had no price in it.
    public let costs: [String: Decimal]?
    /// The newest turns with calls, oldest first; empty for a log that does not mark its prompts.
    public let turns: [Turn]
    /// Turns with calls, including those older than `turns` keeps.
    public let turnCount: Int
    /// The context window of the session's latest call.
    public let contextWindow: Int?
    /// What the calls would have cost at the vendors' API list prices, in US dollars; nil when a call's model has no list price.
    public let listCost: Decimal?

    public init(models: [Model], periods: [Period], subagents: Tokens? = nil, calls: Int, contextTokens: Int? = nil,
                costs: [String: Decimal]? = nil, turns: [Turn] = [], turnCount: Int? = nil, contextWindow: Int? = nil, listCost: Decimal? = nil) {
        self.models = models; self.periods = periods; self.subagents = subagents
        self.calls = calls; self.contextTokens = contextTokens; self.costs = costs
        self.turns = turns; self.turnCount = turnCount ?? turns.count; self.contextWindow = contextWindow; self.listCost = listCost
    }

    private enum CodingKeys: String, CodingKey {
        case models, periods, subagents, calls, contextTokens, costs, turns, turnCount, contextWindow, listCost
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let turns = try c.decodeIfPresent([Turn].self, forKey: .turns) ?? []
        self.init(models: try c.decode([Model].self, forKey: .models), periods: try c.decode([Period].self, forKey: .periods),
                  subagents: try c.decodeIfPresent(Tokens.self, forKey: .subagents), calls: try c.decode(Int.self, forKey: .calls),
                  contextTokens: try c.decodeIfPresent(Int.self, forKey: .contextTokens),
                  costs: try c.decodeIfPresent([String: Decimal].self, forKey: .costs), turns: turns,
                  turnCount: try c.decodeIfPresent(Int.self, forKey: .turnCount), contextWindow: try c.decodeIfPresent(Int.self, forKey: .contextWindow),
                  listCost: try c.decodeIfPresent(Decimal.self, forKey: .listCost))
    }

    public var total: Tokens { models.reduce(Tokens()) { $0 + $1.tokens } }

    /// The share of prompt tokens served from the cache; nil before any prompt was sent.
    public var cacheHitRate: Double? {
        let total = total, prompt = total.tokensIn + total.cacheReadTokens
        return prompt > 0 ? Double(total.cacheReadTokens) / Double(prompt) : nil
    }

    /// How full the context of the latest call was, 0…1; nil without both sizes.
    public var contextFill: Double? {
        guard let contextTokens, let contextWindow, contextWindow > 0 else { return nil }
        return Double(contextTokens) / Double(contextWindow)
    }
}

/// Which ledger contributions hold one session: its own log or id, and a key prefix under which its sub-agents' logs lie.
public struct SessionUsageRequest: Hashable, Sendable {
    public let sessionID: String
    public let keys: [String]
    public let subagentPrefix: String?
    /// Sub-agents' logs named one by one, for clients that keep them apart from the session's own log.
    public let subagentKeys: [String]
    /// The key of a log that records every model call, one event each, so its latest event tells the context size.
    public let callLog: String?

    public init(sessionID: String, keys: [String], subagentPrefix: String? = nil, subagentKeys: [String] = [], callLog: String? = nil) {
        self.sessionID = sessionID; self.keys = keys; self.subagentPrefix = subagentPrefix; self.subagentKeys = subagentKeys
        self.callLog = callLog
    }

    /// Whether any of `keys` is one of the session's logs or lies where its sub-agents' logs do.
    func touches(_ keys: Set<String>) -> Bool {
        guard !keys.isEmpty else { return false }
        if self.keys.contains(where: keys.contains) || subagentKeys.contains(where: keys.contains) { return true }
        return subagentPrefix.map { prefix in keys.contains { $0.hasPrefix(prefix) } } ?? false
    }

    /// Logs read line by line contribute under their path, sources read whole under the session id. A log `x.jsonl`
    /// keeps its sub-agents' logs in the directory `x/`.
    public init(_ session: LiveSession) {
        let named = session.subagentTranscripts ?? []
        guard let path = session.transcriptPath else {
            self.init(sessionID: session.id, keys: [session.id], subagentKeys: named)
            return
        }
        let directory = path.hasSuffix(".jsonl") ? String(path.dropLast(".jsonl".count)) + "/" : nil
        self.init(sessionID: session.id, keys: [path, session.id], subagentPrefix: directory, subagentKeys: named, callLog: path)
    }
}

/// Folds one session's calls, in time order, into its breakdown. Calls go to the turn of the latest prompt at or
/// before them, sub-agents' calls included; calls before the first prompt form a turn of their own. A call in the
/// session's own log that reads back less than half of the previous call's prompt from the cache and sends at least
/// half of it again re-cached that prompt: the cache had lapsed or the model changed. A compaction shrinks the prompt
/// instead, so it re-caches nothing.
struct SessionUsageBuilder {
    struct Call {
        let timestamp: Date
        let agentId: String
        let tokens: SessionUsage.Tokens
        /// The session's own log rather than a sub-agent's.
        let own: Bool
        /// A log that records every call, so its prompt is the session's context.
        let callLog: Bool
        let contextWindow: Int?
    }

    private let prompts: [Date]
    private let compactions: [Date]
    private var models: [String: SessionUsage.Tokens] = [:]
    private var periods: [Date: [String: SessionUsage.Tokens]] = [:]
    private var subagents: SessionUsage.Tokens?
    private var calls = 0
    private var context: Int?
    private var window: Int?
    private var latestAgent: String?
    private var listCost: Decimal? = 0
    /// The prompt of the latest call in the session's own log.
    private var previousPrompt: Int?
    private struct TurnSums {
        var start: Date, end: Date
        var tokens = SessionUsage.Tokens(), calls = 0
        var subagents: SessionUsage.Tokens?
        var context: Int?, peak = 0, recached = 0
        var listCost: Decimal? = 0
    }
    private var turns: [Int: TurnSums] = [:]
    private var promptIndex = -1

    /// - prompts, compactions: the session's own marks.
    init(prompts: [Date], compactions: [Date]) {
        self.prompts = prompts.sorted()
        self.compactions = compactions
    }

    var isEmpty: Bool { calls == 0 }
    var latestModel: String? { latestAgent }

    /// Calls must arrive oldest first.
    mutating func add(_ call: Call) {
        let tokens = call.tokens
        calls += 1
        models[call.agentId, default: .init()] += tokens
        let period = Date(timeIntervalSince1970: (call.timestamp.timeIntervalSince1970 / UsageBucket.duration).rounded(.down) * UsageBucket.duration)
        periods[period, default: [:]][call.agentId, default: .init()] += tokens
        if !call.own { subagents = (subagents ?? .init()) + tokens }
        let prompt = tokens.tokensIn + tokens.cacheReadTokens
        var recached = 0
        if call.callLog {
            if let previous = previousPrompt, tokens.cacheReadTokens * 2 < previous, tokens.tokensIn * 2 >= previous { recached = tokens.tokensIn }
            previousPrompt = prompt
            context = prompt
        }
        if call.own {
            latestAgent = call.agentId
            if let reported = call.contextWindow { window = reported }
        }
        let cost = ModelCatalog.cost(agentId: call.agentId, kinds: tokens.kinds)?.amount
        listCost = listCost.flatMap { sum in cost.map { sum + $0 } }
        guard !prompts.isEmpty else { return }
        while promptIndex + 1 < prompts.count, prompts[promptIndex + 1] <= call.timestamp { promptIndex += 1 }
        var turn = turns[promptIndex] ?? TurnSums(start: promptIndex >= 0 ? prompts[promptIndex] : call.timestamp, end: call.timestamp)
        turn.end = max(turn.end, call.timestamp)
        turn.tokens += tokens
        turn.calls += 1
        if !call.own { turn.subagents = (turn.subagents ?? .init()) + tokens }
        if call.callLog {
            turn.context = prompt
            turn.peak = max(turn.peak, prompt)
        }
        turn.recached += recached
        turn.listCost = turn.listCost.flatMap { sum in cost.map { sum + $0 } }
        turns[promptIndex] = turn
    }

    /// - window: the context window of the latest model given what the client reported, or nil.
    func build(costs: [String: Decimal]?, window resolve: (_ agentId: String, _ reported: Int?) -> Int?) -> SessionUsage {
        let compacted = Set(compactions.map { date in (prompts.lastIndex { $0 <= date } ?? -1) })
        let all = turns.keys.sorted().map { index in
            let turn = turns[index]!
            return SessionUsage.Turn(start: turn.start, end: turn.end, tokens: turn.tokens, calls: turn.calls, contextTokens: turn.context,
                                     compacted: compacted.contains(index), subagents: turn.subagents,
                                     peakContextTokens: turn.context.flatMap { turn.peak > $0 ? turn.peak : nil },
                                     recachedTokens: turn.recached > 0 ? turn.recached : nil, listCost: turn.listCost)
        }
        return SessionUsage(
            models: models.map { SessionUsage.Model(agentId: $0.key, tokens: $0.value) }.sorted {
                let left = $0.tokens.tokensIn + $0.tokens.tokensOut, right = $1.tokens.tokensIn + $1.tokens.tokensOut
                return left == right ? $0.agentId < $1.agentId : left > right
            },
            periods: periods.keys.sorted().flatMap { start in
                periods[start]!.keys.sorted().map { SessionUsage.Period(start: start, agentId: $0, tokens: periods[start]![$0]!) }
            },
            subagents: subagents, calls: calls, contextTokens: context, costs: costs,
            turns: Array(all.suffix(SessionUsage.turnLimit)), turnCount: all.count,
            contextWindow: latestAgent.flatMap { resolve($0, window) }, listCost: listCost)
    }
}
