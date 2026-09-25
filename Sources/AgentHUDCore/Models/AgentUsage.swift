import Foundation

/// What one agent spent in an interval, at the vendors' API list prices: by model and by session, with the part of the
/// price that read context back from the cache and what sending context again after the cache lapsed cost.
public struct AgentUsage: Hashable, Sendable, Identifiable {
    public struct Model: Hashable, Sendable {
        public let agentId: String
        public let tokens: TokenKinds
        public let cost: ModelCatalog.ListCost?
    }

    public struct Session: Hashable, Sendable {
        public let id: String
        public let tokens: TokenKinds
        public let cost: ModelCatalog.ListCost?
    }

    /// The vendor, as consumers and Settings name it.
    public let vendor: String
    public var id: String { vendor }
    public let tokens: TokenKinds
    public let cost: ModelCatalog.ListCost?
    /// The share of the price spent reading context back from the cache, 0…1; nil when no model has a price.
    public let cacheReadShare: Double?
    /// What its sessions paid, in US dollars, to send context again after the cache lapsed or the model changed; nil when
    /// none did.
    public let recachedCost: Decimal?
    /// The most tokens of the counted kinds first.
    public let models: [Model]
    /// Sessions by their tokens of the counted kinds in the interval, the most first.
    public let sessions: [Session]

    /// Every agent with tokens in `interval`, the most tokens of `dimensions` first, as agents, models and sessions are
    /// ranked throughout; the international list price breaks ties. Buckets give the models, and each session's breakdown
    /// its part of the interval. Prices follow each model's platform.
    public static func build(usage: [UsageBucket], consumers: [AgentDescriptor], sessions: [LiveSession],
                             breakdowns: [String: SessionUsage], vendor sessionVendor: (LiveSession) -> String?,
                             interval: DateInterval, dimensions: TokenDimensions,
                             region: (String) -> ModelCatalog.Region) -> [AgentUsage] {
        let vendors = Dictionary(consumers.map { ($0.id, $0.vendor) }, uniquingKeysWith: { first, _ in first })
        var tokens: [String: [String: TokenKinds]] = [:], peak: [String: [String: TokenKinds]] = [:]
        for bucket in usage where bucket.overlaps(interval) && !bucket.kinds.isEmpty {
            guard let vendor = vendors[bucket.agentId] else { continue }
            tokens[vendor, default: [:]][bucket.agentId, default: TokenKinds()] += bucket.kinds
            if ModelCatalog.model(for: bucket.agentId)?.peakHours == true, ModelCatalog.isPeak(bucket.start) {
                peak[vendor, default: [:]][bucket.agentId, default: TokenKinds()] += bucket.kinds
            }
        }
        var spent: [String: [(session: Session, rank: Rank)]] = [:], recached: [String: Decimal] = [:]
        for session in sessions {
            guard let vendor = sessionVendor(session), let breakdown = breakdowns[session.id] else { continue }
            var byModel: [String: TokenKinds] = [:]
            for period in breakdown.periods where interval.contains(period.start) { byModel[period.agentId, default: TokenKinds()] += period.tokens.kinds }
            if !byModel.isEmpty {
                let entry = Session(id: session.id, tokens: byModel.values.reduce(TokenKinds(), +), cost: ModelCatalog.cost(of: byModel, region: region))
                spent[vendor, default: []].append((entry, Rank(byModel, dimensions)))
            }
            for turn in breakdown.turns where interval.contains(turn.start) {
                guard let sent = turn.recachedTokens else { continue }
                let kinds = turn.tokens.cacheWriteTokens > 0 ? TokenKinds(cacheWrite: sent) : TokenKinds(input: sent)
                if let cost = ModelCatalog.cost(agentId: session.agentId, summed: kinds)?.amount { recached[vendor, default: 0] += cost }
            }
        }
        return tokens.map { vendor, models in
            var price: Decimal = 0, reading: Decimal = 0
            for (agentId, kinds) in models {
                guard let all = ModelCatalog.cost(agentId: agentId, summed: kinds)?.amount,
                      let read = ModelCatalog.cost(agentId: agentId, summed: TokenKinds(cacheRead: kinds.cacheRead))?.amount else { continue }
                price += all; reading += read
            }
            let ranked = models.sorted { Rank([$0.key: $0.value], dimensions) > Rank([$1.key: $1.value], dimensions) }.map { agentId, kinds in
                Model(agentId: agentId, tokens: kinds,
                      cost: ModelCatalog.cost(of: [agentId: kinds], peak: peak[vendor]?.filter { $0.key == agentId } ?? [:], region: region))
            }
            return AgentUsage(vendor: vendor, tokens: models.values.reduce(TokenKinds(), +),
                              cost: ModelCatalog.cost(of: models, peak: peak[vendor] ?? [:], region: region),
                              cacheReadShare: price > 0 ? NSDecimalNumber(decimal: reading / price).doubleValue : nil,
                              recachedCost: recached[vendor],
                              models: ranked, sessions: (spent[vendor] ?? []).sorted { $0.rank > $1.rank }.map(\.session))
        }
        .sorted { (Rank($0.models, dimensions), $1.vendor) > (Rank($1.models, dimensions), $0.vendor) }
    }

    /// Tokens of the counted kinds, then the international list price.
    private struct Rank: Comparable {
        let tokens: Int, price: Decimal

        init(_ models: [String: TokenKinds], _ dimensions: TokenDimensions) {
            tokens = models.values.reduce(0) { $0 + dimensions.count($1) }
            price = models.reduce(0) { $0 + (ModelCatalog.cost(agentId: $1.key, summed: $1.value)?.amount ?? 0) }
        }

        init(_ models: [Model], _ dimensions: TokenDimensions) {
            self.init(Dictionary(models.map { ($0.agentId, $0.tokens) }, uniquingKeysWith: +), dimensions)
        }

        static func < (lhs: Rank, rhs: Rank) -> Bool { (lhs.tokens, lhs.price) < (rhs.tokens, rhs.price) }
    }
}
