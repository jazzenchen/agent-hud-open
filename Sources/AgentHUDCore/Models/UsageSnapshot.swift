import Foundation

/// Latest quota reading for one agent.
public struct UsageSnapshot: Hashable, Codable, Sendable {
    public let agentId: String
    /// Remaining % of this quota window.
    public let remainingPct: Double
    /// Remaining % of the weekly (7d) window, when the source reports one.
    public let weeklyRemainingPct: Double?
    public let resetAt: Date?
    /// Full reset period reported by the source, in seconds.
    public let windowDuration: TimeInterval?
    public let weeklyResetAt: Date?
    public let updatedAt: Date

    public init(
        agentId: String,
        remainingPct: Double,
        weeklyRemainingPct: Double? = nil,
        resetAt: Date? = nil,
        windowDuration: TimeInterval? = nil,
        weeklyResetAt: Date? = nil,
        updatedAt: Date
    ) {
        self.agentId = agentId
        self.remainingPct = remainingPct
        self.weeklyRemainingPct = weeklyRemainingPct
        self.resetAt = resetAt
        self.windowDuration = windowDuration
        self.weeklyResetAt = weeklyResetAt
        self.updatedAt = updatedAt
    }

    public var cycle: QuotaCycle? { QuotaCycle(resetAt: resetAt, duration: windowDuration) }
}
