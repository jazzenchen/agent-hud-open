import Foundation

/// Consumption speed of a quota window, in remaining-% per hour.
public struct BurnRate: Hashable, Sendable {
    public let pctPerHour: Double

    public init(pctPerHour: Double) {
        self.pctPerHour = pctPerHour
    }

    /// Least-squares slope over the samples, ignoring buckets that contain a reset (remaining went up).
    /// Returns nil when fewer than two usable samples exist or nothing was consumed.
    public static func estimate(_ samples: [HistorySample]) -> BurnRate? {
        let consumed = samples
            .sorted { $0.hourStart < $1.hourStart }
            .filter { $0.remainingEnd <= $0.remainingStart }
            .map { $0.remainingStart - $0.remainingEnd }
        guard consumed.count >= 2 else { return nil }
        let average = consumed.reduce(0, +) / Double(consumed.count)
        guard average > 0 else { return nil }
        return BurnRate(pctPerHour: average)
    }

    /// Seconds until `remainingPct` reaches zero at this rate.
    public func timeToExhaust(remainingPct: Double) -> TimeInterval? {
        guard pctPerHour > 0 else { return nil }
        return max(0, remainingPct) / pctPerHour * 3600
    }
}
