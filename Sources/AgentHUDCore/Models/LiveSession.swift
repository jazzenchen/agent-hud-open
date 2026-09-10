import Foundation

/// A coding-agent session read from local logs (running or recently finished).
public struct LiveSession: Hashable, Codable, Sendable, Identifiable {
    public let id: String
    public let agentId: String
    public let task: String
    public let terminal: String?
    public let startedAt: Date
    public let endedAt: Date?
    /// Share of the current quota window consumed by this session, in %.
    public let pctOfWindow: Double?
    public let tokensIn: Int
    public let tokensOut: Int
    public let cacheReadTokens: Int
    public let client: String?
    public let transcriptPath: String?
    public let accountWide: Bool

    public init(
        id: String,
        agentId: String,
        task: String,
        terminal: String?,
        startedAt: Date,
        endedAt: Date? = nil,
        pctOfWindow: Double?,
        tokensIn: Int,
        tokensOut: Int,
        client: String? = nil,
        transcriptPath: String? = nil,
        cacheReadTokens: Int = 0,
        accountWide: Bool = false
    ) {
        self.id = id
        self.agentId = agentId
        self.task = task
        self.terminal = terminal
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.pctOfWindow = pctOfWindow
        self.tokensIn = tokensIn
        self.tokensOut = tokensOut
        self.cacheReadTokens = cacheReadTokens
        self.client = client
        self.transcriptPath = transcriptPath
        self.accountWide = accountWide
    }

    public var isLive: Bool { endedAt == nil }

    private enum CodingKeys: String, CodingKey {
        case id, agentId, task, terminal, startedAt, endedAt, pctOfWindow, tokensIn, tokensOut, client, transcriptPath, cacheReadTokens, accountWide
    }
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(id: try c.decode(String.self, forKey: .id), agentId: try c.decode(String.self, forKey: .agentId),
            task: try c.decode(String.self, forKey: .task), terminal: try c.decodeIfPresent(String.self, forKey: .terminal),
            startedAt: try c.decode(Date.self, forKey: .startedAt), endedAt: try c.decodeIfPresent(Date.self, forKey: .endedAt),
            pctOfWindow: try c.decodeIfPresent(Double.self, forKey: .pctOfWindow), tokensIn: try c.decode(Int.self, forKey: .tokensIn),
            tokensOut: try c.decode(Int.self, forKey: .tokensOut), client: try c.decodeIfPresent(String.self, forKey: .client),
            transcriptPath: try c.decodeIfPresent(String.self, forKey: .transcriptPath),
            cacheReadTokens: try c.decodeIfPresent(Int.self, forKey: .cacheReadTokens) ?? 0,
            accountWide: try c.decodeIfPresent(Bool.self, forKey: .accountWide) ?? false)
    }

    public func duration(now: Date) -> TimeInterval {
        max(0, (endedAt ?? now).timeIntervalSince(startedAt))
    }

    public var hasTokenCounts: Bool { tokensIn > 0 || tokensOut > 0 || cacheReadTokens > 0 }
}
