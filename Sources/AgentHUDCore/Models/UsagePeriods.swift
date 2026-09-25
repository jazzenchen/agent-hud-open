import Foundation

/// Every model's tokens over the periods the statistics compare, ending at the report's time: today from local midnight,
/// and the last seven and thirty days.
public struct UsagePeriods: Hashable, Codable, Sendable {
    public enum Period: String, CaseIterable, Codable, CodingKeyRepresentable, Sendable {
        case today, days7, days30

        public var label: String {
            switch self {
            case .today: L10n.text("今天", "Today")
            case .days7: L10n.text("近 7 天", "Last 7 days")
            case .days30: L10n.text("近 30 天", "Last 30 days")
            }
        }

        public func start(endingAt now: Date, calendar: Calendar = .current) -> Date {
            switch self {
            case .today: calendar.startOfDay(for: now)
            case .days7: now.addingTimeInterval(-7 * 86400)
            case .days30: now.addingTimeInterval(-30 * 86400)
            }
        }
    }

    /// Each period's tokens by model id.
    public var tokens: [Period: [String: TokenKinds]]
    /// The part of `tokens` counted in DeepSeek's peak hours, for the models it bills at peak rates.
    public var peak: [Period: [String: TokenKinds]]

    public init(tokens: [Period: [String: TokenKinds]] = [:], peak: [Period: [String: TokenKinds]] = [:]) {
        self.tokens = tokens
        self.peak = peak
    }

    private enum CodingKeys: String, CodingKey { case tokens, peak }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(tokens: try c.decode([Period: [String: TokenKinds]].self, forKey: .tokens),
                  peak: try c.decodeIfPresent([Period: [String: TokenKinds]].self, forKey: .peak) ?? [:])
    }

    /// Adds 15-minute buckets to the periods they fall in; like the charts, a bucket counts whole when the period starts
    /// inside it.
    public mutating func add(_ buckets: [UsageBucket], endingAt now: Date, calendar: Calendar = .current) {
        for period in Period.allCases {
            let start = period.start(endingAt: now, calendar: calendar)
            for bucket in buckets where bucket.start.addingTimeInterval(UsageBucket.duration) > start && bucket.start < now {
                tokens[period, default: [:]][bucket.agentId, default: TokenKinds()] += bucket.kinds
                if ModelCatalog.model(for: bucket.agentId)?.peakHours == true, ModelCatalog.isPeak(bucket.start) {
                    peak[period, default: [:]][bucket.agentId, default: TokenKinds()] += bucket.kinds
                }
            }
        }
    }
}
