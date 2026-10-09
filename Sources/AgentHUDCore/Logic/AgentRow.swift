import Foundation

/// One agent as shown in the hover panel / menu: descriptor + latest reading.
public struct AgentRow: Hashable, Sendable, Identifiable {
    public let agent: AgentDescriptor
    public let remainingPct: Double?
    public let level: StatusLevel?
    public let resetAt: Date?
    public let weeklyRemainingPct: Double?
    /// Index into `AgentPalette` (position among enabled agents).
    public let paletteIndex: Int
    /// The account this window belongs to, when the provider identifies accounts.
    public let account: AccountObservation?
    /// What the surfaces weigh about the window's reading at the time the row was made; `level` follows its `showsLevel`.
    public let assessment: ReadingAssessment

    public var id: String { agent.id }

    /// Other accounts show their last reading without a status level, so they stay out of the glow and alerts.
    public var isCurrentAccount: Bool { assessment.isCurrentAccount }

    /// Share of the window already consumed; the UI shows usage, not what is left.
    public var usedPct: Double? { remainingPct.map { max(0, min(100, 100 - $0)) } }

    public var missingQuotaLabel: String {
        assessment.observedAt == nil ? "—" : "N/A"
    }

    public func resetLabel(now: Date, compact: Bool = false) -> String {
        guard isCurrentAccount else { return "—" }
        if let resetAt {
            if resetAt <= now { return L10n.text("等待更新", "Pending update") }
            if resetAt.timeIntervalSince(now) < 60 { return "<1m" }
        }
        return compact ? Countdown.resetLabelCompact(resetAt, now: now) : Countdown.resetLabel(resetAt, now: now)
    }
}
