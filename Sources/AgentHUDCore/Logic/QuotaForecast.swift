import Foundation

/// Hover copy for a time-limited quota, using that window's existing burn-rate estimate.
public enum QuotaForecast {
    /// Old observations remain visible but must not generate new quota alerts.
    public static let maximumReadingAge: TimeInterval = 30 * 60

    public static func hint(snapshot: UsageSnapshot, insights: UsageInsights?, now: Date) -> String? {
        guard snapshot.resetAt != nil else { return nil }
        if snapshot.remainingPct <= 0 {
            return L10n.text("已耗尽", "Exhausted")
        }
        guard snapshot.cycle != nil else {
            return L10n.text("暂无预测", "No estimate")
        }
        if insights?.burnRatePctPerHour == 0 {
            return L10n.text("暂无消耗", "No usage")
        }
        guard let interval = insights?.timeToExhaust, interval > 0, interval.isFinite else {
            return L10n.text("记录不足", "Insufficient data")
        }
        // Keep the duration tied to the provider's latest estimate; don't simulate unobserved consumption.
        let exhaustion = duration(interval)
        return L10n.text("耗尽 ~\(exhaustion)", "Exhausts ~\(exhaustion)")
    }

    private static func duration(_ interval: TimeInterval) -> String {
        let minutes = Int(ceil(interval / 60))
        let hours = minutes / 60
        if hours > 0 {
            if minutes % 60 == 0 { return L10n.text("\(hours)小时", "\(hours)h") }
            return L10n.text("\(hours)小时\(minutes % 60)分", "\(hours)h \(minutes % 60)m")
        }
        return L10n.text("\(minutes)分", "\(minutes)m")
    }
}
