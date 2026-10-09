import Foundation

/// What a window's reading and recent pace say about the rest of its cycle, weighed in the order of the cases: a
/// window without a reset has no outlook, an empty one is exhausted, one of unknown length has no estimate, and a pace
/// of nothing is no usage.
public enum QuotaOutlook: Hashable, Sendable {
    /// The window has no reset time.
    case untimed
    /// Nothing is left.
    case exhausted
    /// The window's length is unknown, so there is no pace to go by.
    case noEstimate
    /// The recent pace uses nothing.
    case noUsage
    /// The readings give no time for the window to run out.
    case insufficientData
    /// At the recent pace the rest lasts `in` seconds; `beforeReset` says whether that ends before the window resets.
    case exhausts(in: TimeInterval, beforeReset: Bool)
}

/// Quota calculations shared by the providers, the store, the alerts and the desktop, as pure functions of readings
/// and time.
public enum QuotaMath {
    /// The share of a window left when its service reports the share used, kept within 0...100.
    public static func remaining(usedPercent: Double) -> Double {
        max(0, min(100, 100 - usedPercent))
    }

    /// Where a window's insights start reading its stored readings, for every provider whatever the statistics range: a
    /// week back, or the start of its current cycle when that is earlier, so the burn rate of a longer window sees its
    /// whole cycle. A week back for a window without a reading.
    public static func historyStart(for snapshot: UsageSnapshot?, now: Date) -> Date {
        let lookback = now.addingTimeInterval(-AlertPolicy.insightsLookback)
        return min(lookback, snapshot?.cycle?.start ?? lookback)
    }

    /// A window's burn rate, how long its rest lasts at that rate, and the times it hit its cap, from its stored readings
    /// since `historyStart(for:now:)`. The burn rate takes the readings of the current cycle; cap hits are counted over
    /// the last week. Without a reading there is no burn rate, only cap hits.
    public static func insights(snapshot: UsageSnapshot?, samples: [QuotaSample], now: Date) -> UsageInsights {
        let burn = snapshot?.remainingPct == nil ? nil : UsageAnalytics.burnRate(samples: samples, cycle: snapshot?.cycle, now: now)
        let week = now.addingTimeInterval(-AlertPolicy.insightsLookback)
        let caps = UsageAnalytics.capStats(samples: samples.filter { $0.timestamp >= week }, now: now)
        return UsageInsights(burnRatePctPerHour: burn?.pctPerHour,
                             timeToExhaust: snapshot.flatMap(\.remainingPct).flatMap { burn?.timeToExhaust(remainingPct: $0) },
                             weeklyCapHits: caps.hits, weeklyWaitTotal: caps.totalWait,
                             weeklyWaitLongest: caps.longestWait, weeklyWaitLongestAt: caps.longestAt)
    }

    /// The window's outlook at `now`.
    public static func outlook(snapshot: UsageSnapshot, insights: UsageInsights?, now: Date) -> QuotaOutlook {
        guard snapshot.resetAt != nil else { return .untimed }
        guard let remaining = snapshot.remainingPct else { return .noEstimate }
        if remaining <= AlertPolicy.exhaustedRemaining { return .exhausted }
        guard snapshot.cycle != nil else { return .noEstimate }
        if insights?.burnRatePctPerHour == 0 { return .noUsage }
        guard let exhaustion = exhaustion(insights: insights, resetAt: snapshot.resetAt, now: now) else { return .insufficientData }
        return .exhausts(in: exhaustion.interval, beforeReset: exhaustion.beforeReset)
    }

    /// How long a window's rest lasts at its recent pace, and whether that ends before the window resets, whatever
    /// else the outlook weighs first. A window without a reset counts as running out before it. Nil when the pace
    /// gives no time.
    public static func exhaustion(insights: UsageInsights?, resetAt: Date?, now: Date) -> (interval: TimeInterval, beforeReset: Bool)? {
        guard let interval = insights?.timeToExhaust, interval > 0, interval.isFinite else { return nil }
        return (interval, resetAt.map { interval < $0.timeIntervalSince(now) } ?? true)
    }

    /// The share of the window used by its reset at the recent pace, at most all of it. Nil without a pace or a reset
    /// still ahead.
    public static func projectedUsedAtReset(usedPct: Double, insights: UsageInsights?, resetAt: Date?, now: Date) -> Double? {
        guard let rate = insights?.burnRatePctPerHour, let resetAt, resetAt > now else { return nil }
        return min(100, usedPct + rate * resetAt.timeIntervalSince(now) / 3600)
    }

    /// Tokens per hour the window's consumers spent over the observed part of its current cycle; usage from before
    /// `usage` begins is not guessed at.
    public static func tokensPerHour(snapshot: UsageSnapshot, consumers: Set<String>, usage: [UsageBucket], now: Date) -> Double? {
        guard !consumers.isEmpty, let cycle = snapshot.cycle, let oldest = usage.first?.start else { return nil }
        let start = max(cycle.start, oldest)
        let hours = now.timeIntervalSince(start) / 3600
        guard hours > 0 else { return nil }
        let tokens = usage.reduce(0) { total, bucket in
            consumers.contains(bucket.agentId) && bucket.start >= start && bucket.start < now ? total + bucket.total : total
        }
        return (Double(tokens) / hours).rounded()
    }
}
