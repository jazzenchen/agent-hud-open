import Foundation

/// A provider-normalized token observation, shared by every client.
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
