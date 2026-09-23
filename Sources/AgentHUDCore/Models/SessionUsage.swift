import Foundation

/// Where one session's tokens went, as the usage ledger recorded them: by model, by 15-minute period, and the part
/// its sub-agents spent. It covers what this Mac read of the session within the ledger's retention.
public struct SessionUsage: Hashable, Codable, Sendable {
    public struct Tokens: Hashable, Codable, Sendable {
        public var tokensIn: Int
        public var tokensOut: Int
        public var cacheReadTokens: Int

        public init(tokensIn: Int = 0, tokensOut: Int = 0, cacheReadTokens: Int = 0) {
            self.tokensIn = tokensIn; self.tokensOut = tokensOut; self.cacheReadTokens = cacheReadTokens
        }

        public static func + (lhs: Self, rhs: Self) -> Self {
            Tokens(tokensIn: lhs.tokensIn + rhs.tokensIn, tokensOut: lhs.tokensOut + rhs.tokensOut,
                   cacheReadTokens: lhs.cacheReadTokens + rhs.cacheReadTokens)
        }

        public static func += (lhs: inout Self, rhs: Self) { lhs = lhs + rhs }

        public var isEmpty: Bool { tokensIn == 0 && tokensOut == 0 && cacheReadTokens == 0 }
        public func count(_ dimensions: TokenDimensions) -> Int {
            dimensions.count(input: tokensIn, output: tokensOut, cache: cacheReadTokens)
        }
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
            UsageBucket(start: start, agentId: agentId, tokensIn: tokens.tokensIn, tokensOut: tokens.tokensOut, cacheReadTokens: tokens.cacheReadTokens)
        }
    }

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

    public init(models: [Model], periods: [Period], subagents: Tokens? = nil, calls: Int, contextTokens: Int? = nil,
                costs: [String: Decimal]? = nil) {
        self.models = models; self.periods = periods; self.subagents = subagents
        self.calls = calls; self.contextTokens = contextTokens; self.costs = costs
    }

    public var total: Tokens { models.reduce(Tokens()) { $0 + $1.tokens } }

    /// The share of prompt tokens served from the cache; nil before any prompt was sent.
    public var cacheHitRate: Double? {
        let total = total, prompt = total.tokensIn + total.cacheReadTokens
        return prompt > 0 ? Double(total.cacheReadTokens) / Double(prompt) : nil
    }
}

/// Which ledger contributions hold one session: its own log or id, and a key prefix under which its sub-agents' logs lie.
public struct SessionUsageRequest: Hashable, Sendable {
    public let sessionID: String
    public let keys: [String]
    public let subagentPrefix: String?
    /// The key of a log that records every model call, one event each, so its latest event tells the context size.
    public let callLog: String?

    public init(sessionID: String, keys: [String], subagentPrefix: String? = nil, callLog: String? = nil) {
        self.sessionID = sessionID; self.keys = keys; self.subagentPrefix = subagentPrefix; self.callLog = callLog
    }

    /// Logs read line by line contribute under their path, sources read whole under the session id. A log `x.jsonl`
    /// keeps its sub-agents' logs in the directory `x/`.
    public init(_ session: LiveSession) {
        guard let path = session.transcriptPath else {
            self.init(sessionID: session.id, keys: [session.id])
            return
        }
        let directory = path.hasSuffix(".jsonl") ? String(path.dropLast(".jsonl".count)) + "/" : nil
        self.init(sessionID: session.id, keys: [path, session.id], subagentPrefix: directory, callLog: path)
    }
}
