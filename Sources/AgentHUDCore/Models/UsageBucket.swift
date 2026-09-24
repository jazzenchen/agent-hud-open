import Foundation

/// Token totals of one consumer in one 15-minute period, after overlapping logs were resolved.
/// Every time zone offset is a whole number of these periods, so local quarter hours, hours and days sum them exactly.
public struct UsageBucket: Hashable, Codable, Sendable {
    public static let duration: TimeInterval = 900

    public let start: Date
    public let agentId: String
    public let tokensIn: Int
    public let tokensOut: Int
    public let cacheReadTokens: Int
    /// The part of `tokensIn` written to the prompt cache.
    public let cacheWriteTokens: Int
    /// The part of `tokensOut` spent reasoning.
    public let reasoningTokens: Int
    /// The provider account of usage imported from that account, the same on every machine signed into it; nil for local logs.
    public let account: String?

    public init(start: Date, agentId: String, tokensIn: Int, tokensOut: Int, cacheReadTokens: Int = 0, cacheWriteTokens: Int = 0,
                reasoningTokens: Int = 0, account: String? = nil) {
        self.start = start
        self.agentId = agentId
        self.tokensIn = tokensIn
        self.tokensOut = tokensOut
        self.cacheReadTokens = cacheReadTokens
        self.cacheWriteTokens = cacheWriteTokens
        self.reasoningTokens = reasoningTokens
        self.account = account
    }

    private enum CodingKeys: String, CodingKey {
        case start, agentId, tokensIn, tokensOut, cacheReadTokens, cacheWriteTokens, reasoningTokens, account
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(start: try c.decode(Date.self, forKey: .start), agentId: try c.decode(String.self, forKey: .agentId),
                  tokensIn: try c.decode(Int.self, forKey: .tokensIn), tokensOut: try c.decode(Int.self, forKey: .tokensOut),
                  cacheReadTokens: try c.decodeIfPresent(Int.self, forKey: .cacheReadTokens) ?? 0,
                  cacheWriteTokens: try c.decodeIfPresent(Int.self, forKey: .cacheWriteTokens) ?? 0,
                  reasoningTokens: try c.decodeIfPresent(Int.self, forKey: .reasoningTokens) ?? 0,
                  account: try c.decodeIfPresent(String.self, forKey: .account))
    }

    public var end: Date { start.addingTimeInterval(Self.duration) }
    public var total: Int { tokensIn + tokensOut }
    public var kinds: TokenKinds {
        TokenKinds(tokensIn: tokensIn, tokensOut: tokensOut, cacheRead: cacheReadTokens, cacheWrite: cacheWriteTokens, reasoning: reasoningTokens)
    }

    /// Periods are counted when they overlap the range, so a range edge inside a period includes that whole period.
    public func overlaps(_ interval: DateInterval) -> Bool { end > interval.start && start < interval.end }
}
