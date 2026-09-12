import Foundation

/// Pure transforms from quota samples + transcript usage to the report's derived series.
public enum UsageAnalytics {
    public static func hourStart(_ date: Date, calendar: Calendar) -> Date {
        calendar.dateInterval(of: .hour, for: date)?.start ?? date
    }

    /// Hourly buckets for one agent over the last `hours` hours (ending in the current hour).
    /// Remaining % carries forward between samples; hours before the first sample take the first known value
    /// (or `fallbackRemaining`) so the line stays continuous.
    public static func hourlyHistory(
        agentId: String,
        quota: [QuotaSample],
        usage: [UsageEvent],
        hours: Int,
        now: Date,
        calendar: Calendar,
        fallbackRemaining: Double?
    ) -> [HistorySample] {
        guard hours > 0 else { return [] }
        let currentHour = hourStart(now, calendar: calendar)
        let sorted = quota.sorted { $0.timestamp < $1.timestamp }
        var carried = sorted.first?.remainingPct ?? fallbackRemaining ?? 100
        var tokensByHour: [Date: Int] = [:]
        for event in usage where event.agentId == agentId {
            tokensByHour[hourStart(event.timestamp, calendar: calendar), default: 0] += event.total
        }
        var samplesByHour: [Date: [QuotaSample]] = [:]
        for sample in sorted {
            samplesByHour[hourStart(sample.timestamp, calendar: calendar), default: []].append(sample)
        }
        return (0..<hours).map { index in
            let start = currentHour.addingTimeInterval(TimeInterval(index - hours + 1) * 3600)
            let inHour = samplesByHour[start] ?? []
            let remainingStart = inHour.first?.remainingPct ?? carried
            let remainingEnd = inHour.last?.remainingPct ?? carried
            carried = remainingEnd
            return HistorySample(
                agentId: agentId,
                hourStart: start,
                remainingStart: remainingStart,
                remainingEnd: remainingEnd,
                tokens: tokensByHour[start] ?? 0
            )
        }
    }

    /// 7 × 24 grid (Mon → Sun) of token consumption since `since`.
    public static func activityGrid(usage: [UsageEvent], since: Date, calendar: Calendar, dimensions: TokenDimensions = .fresh) -> ActivityGrid {
        var cells = Array(repeating: Array(repeating: [String: Int](), count: 24), count: 7)
        for event in usage where event.timestamp >= since {
            let weekday = calendar.component(.weekday, from: event.timestamp) // 1 = Sunday
            let row = (weekday + 5) % 7 // Monday = 0
            let hour = calendar.component(.hour, from: event.timestamp)
            cells[row][hour][event.agentId, default: 0] += dimensions.count(event)
        }
        return ActivityGrid(tokensByModel: cells)
    }

    /// Time-weighted average over the observed part of this reset cycle, including idle time.
    public static func burnRate(samples: [QuotaSample], cycle: QuotaCycle?, now: Date) -> BurnRate? {
        guard let cycle, now >= cycle.start, now < cycle.resetAt else { return nil }
        let sampled = sampledQuota(samples, cycle: cycle, now: now)
        guard let first = sampled.first, let last = sampled.last else { return nil }
        let elapsed = last.timestamp.timeIntervalSince(first.timestamp)
        guard elapsed >= cycle.sampleInterval else { return nil }
        return BurnRate(pctPerHour: (first.remainingPct - last.remainingPct) / (elapsed / 3600))
    }

    /// Keep the observed baseline, each cycle-aligned bucket's last reading, and the latest partial bucket.
    /// A quota increase breaks the series (for example an early reset); never bridge across it.
    static func sampledQuota(_ samples: [QuotaSample], cycle: QuotaCycle, now: Date) -> [QuotaSample] {
        let current = samples.filter { $0.timestamp >= cycle.start && $0.timestamp <= now && $0.timestamp < cycle.resetAt }
            .sorted { $0.timestamp < $1.timestamp }
        var startIndex = 0
        for index in current.indices.dropFirst() where current[index].remainingPct > current[index - 1].remainingPct {
            startIndex = index
        }
        let segment = current.dropFirst(startIndex)
        guard let first = segment.first else { return [] }
        var buckets: [Int: QuotaSample] = [:]
        for sample in segment {
            let bucket = Int(sample.timestamp.timeIntervalSince(cycle.start) / cycle.sampleInterval)
            buckets[bucket] = sample
        }
        var result = [first]
        for key in buckets.keys.sorted() {
            if let sample = buckets[key], sample.timestamp > result.last!.timestamp { result.append(sample) }
        }
        return result
    }

    public struct CapStats: Hashable, Sendable {
        public let hits: Int
        public let totalWait: TimeInterval
        public let longestWait: TimeInterval
        public let longestAt: Date?

        public static let none = CapStats(hits: 0, totalWait: 0, longestWait: 0, longestAt: nil)
    }

    /// Times the window hit its cap (remaining ≤ `threshold`) and how long each outage lasted.
    public static func capStats(samples: [QuotaSample], threshold: Double = 0.5, now: Date) -> CapStats {
        let sorted = samples.sorted { $0.timestamp < $1.timestamp }
        var hits = 0
        var total: TimeInterval = 0
        var longest: TimeInterval = 0
        var longestAt: Date?
        var cappedSince: Date?
        for sample in sorted {
            let capped = sample.remainingPct <= threshold
            if capped, cappedSince == nil {
                cappedSince = sample.timestamp
                hits += 1
            } else if !capped, let start = cappedSince {
                let wait = sample.timestamp.timeIntervalSince(start)
                total += wait
                if wait > longest {
                    longest = wait
                    longestAt = start
                }
                cappedSince = nil
            }
        }
        if let start = cappedSince {
            let wait = now.timeIntervalSince(start)
            total += wait
            if wait > longest {
                longest = wait
                longestAt = start
            }
        }
        return CapStats(hits: hits, totalWait: total, longestWait: longest, longestAt: longestAt)
    }

    /// Share of tokens per agent (sums to 1 when there is any usage).
    public static func weeklyShare(usage: [UsageEvent]) -> [String: Double] {
        var totals: [String: Int] = [:]
        for event in usage { totals[event.agentId, default: 0] += event.total }
        let sum = totals.values.reduce(0, +)
        guard sum > 0 else { return [:] }
        return totals.mapValues { Double($0) / Double(sum) }
    }
}
