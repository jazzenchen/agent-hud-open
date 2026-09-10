import Foundation

/// One hourly bucket of history for one agent. Remaining % is recorded at the start and end of the hour so a window
/// reset (a jump back up to 100 %) is visible inside the bucket.
public struct HistorySample: Hashable, Codable, Sendable {
    public let agentId: String
    public let hourStart: Date
    public let remainingStart: Double
    public let remainingEnd: Double
    /// Input + output tokens consumed during the hour.
    public let tokens: Int

    public init(agentId: String, hourStart: Date, remainingStart: Double, remainingEnd: Double, tokens: Int) {
        self.agentId = agentId
        self.hourStart = hourStart
        self.remainingStart = remainingStart
        self.remainingEnd = remainingEnd
        self.tokens = tokens
    }
}

/// 7 × 24 model token counts, rows Monday → Sunday, columns hour 0 → 23.
public struct ActivityGrid: Hashable, Codable, Sendable {
    public let tokensByModel: [[[String: Int]]]

    public init(tokensByModel: [[[String: Int]]]) {
        self.tokensByModel = tokensByModel
    }

    public var tokens: [[Int]] {
        tokensByModel.map { $0.map { $0.values.reduce(0, +) } }
    }

    /// Colour intensity derives from the same counts shown on hover.
    public var rows: [[Double]] {
        let peak = max(1, tokens.flatMap { $0 }.max() ?? 0)
        return tokens.map { $0.map { Double($0) / Double(peak) } }
    }

    public static let empty = ActivityGrid(tokensByModel: Array(repeating: Array(repeating: [:], count: 24), count: 7))
}
