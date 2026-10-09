import Foundation

/// Hover copy for a time-limited quota, using that window's existing burn-rate estimate.
public enum QuotaForecast {
    /// `AlertPolicy.maximumReadingAge`.
    public static var maximumReadingAge: TimeInterval { AlertPolicy.maximumReadingAge }

    /// The text of the window's outlook. A window that runs out only after its reset says what it will have used by
    /// then, as its island row does.
    public static func hint(snapshot: UsageSnapshot, insights: UsageInsights?, now: Date) -> String? {
        guard let remaining = snapshot.remainingPct else { return nil }
        let used = max(0, min(100, 100 - remaining))
        return text(of: QuotaMath.outlook(snapshot: snapshot, insights: insights, now: now),
                    projectedUsedAtReset: QuotaMath.projectedUsedAtReset(usedPct: used, insights: insights, resetAt: snapshot.resetAt, now: now))
    }

    /// An outlook in words: nothing for a window without a reset, how long the rest lasts for one that runs out before
    /// its reset, and `projectedUsedAtReset`, the share its pace uses by the reset, for one that runs out after it.
    package static func text(of outlook: QuotaOutlook, projectedUsedAtReset: Double?) -> String? {
        switch outlook {
        case .untimed: return nil
        case .exhausted: return L10n.text("已耗尽", "Exhausted")
        case .noEstimate: return L10n.text("暂无预测", "No estimate")
        case .noUsage: return L10n.text("暂无消耗", "No usage")
        case .insufficientData: return L10n.text("记录不足", "Insufficient data")
        case .exhausts(let interval, let beforeReset):
            guard beforeReset else { return projectedUsedAtReset.map(byReset) ?? L10n.text("记录不足", "Insufficient data") }
            // Keep the duration tied to the provider's latest estimate; don't simulate unobserved consumption.
            let exhaustion = Countdown.forecast(interval)
            return L10n.text("耗尽 ~\(exhaustion)", "Exhausts ~\(exhaustion)")
        }
    }

    /// The share of the window used by its reset, in words.
    public static func byReset(_ projectedUsedPct: Double) -> String {
        L10n.text("重置时 \(Int(projectedUsedPct.rounded()))%", "\(Int(projectedUsedPct.rounded()))% by reset")
    }
}
