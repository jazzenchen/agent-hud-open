import Foundation

/// Independent, additive token dimensions. Input includes cache writes, but never cache reads.
public struct TokenDimensions: OptionSet, Hashable, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let input = Self(rawValue: 1)
    public static let output = Self(rawValue: 2)
    public static let cache = Self(rawValue: 4)
    public static let fresh: Self = [.input, .output]
    public static let all: Self = [.input, .output, .cache]
    public static let choices: [(value: Self, label: String)] = [(.input, "In"), (.output, "Out"), (.cache, "Cache")]
    public var label: String { Self.choices.filter { contains($0.value) }.map(\.label).joined(separator: " + ") }
    public func count(input: Int, output: Int, cache: Int) -> Int {
        (contains(.input) ? input : 0) + (contains(.output) ? output : 0) + (contains(.cache) ? cache : 0)
    }
    public func count(_ event: UsageEvent) -> Int {
        count(input: event.tokensIn, output: event.tokensOut, cache: event.cacheReadTokens)
    }
}

public enum StatsRange: Int, CaseIterable, Sendable, Hashable {
    case hours5 = 5
    case hours24 = 24
    case days7 = 168

    public var label: String {
        switch self {
        case .hours5: return L10n.text("5 小时", "5 h")
        case .hours24: return L10n.text("24 小时", "24 h")
        case .days7: return L10n.text("7 天", "7 days")
        }
    }

    public var hours: Int { rawValue }

    public var recentLabel: String { L10n.text("近 \(label)", "Last \(label)") }

    public func interval(endingAt now: Date) -> DateInterval {
        DateInterval(start: now.addingTimeInterval(-Double(hours) * 3600), end: now)
    }
}

public enum TokenBucketSize: Int, CaseIterable, Sendable, Hashable {
    case minutes15 = 15
    case minutes30 = 30
    case hour1 = 60
    case day1 = 1440

    public var duration: TimeInterval { Double(rawValue) * 60 }
    public var label: String {
        switch self {
        case .hour1: "1hr"
        case .day1: "1d"
        default: "\(rawValue)min"
        }
    }
}

/// One stack on the token chart, preserving integer counts and its position on the shared time axis.
public struct TokenColumn: Hashable, Sendable, Identifiable {
    public let interval: DateInterval
    public let tokens: [Int]
    public var id: Date { interval.start }
    public var total: Int { tokens.reduce(0, +) }
}

/// Pure transforms from history samples to drawable series.
public enum ChartData {
    /// Polyline reproducing the prototype's shape: x in 0…1, y = remaining % (0…100).
    /// A reset (remainingStart != previous remainingEnd) is drawn as a vertical jump at the bucket boundary.
    public static func remainingPath(_ samples: [HistorySample]) -> [CGPoint] {
        guard let first = samples.first else { return [] }
        let n = Double(samples.count)
        var points = [CGPoint(x: 0, y: first.remainingStart)]
        for (i, sample) in samples.enumerated() {
            let x0 = Double(i) / n, x1 = Double(i + 1) / n
            if i > 0, sample.remainingStart != samples[i - 1].remainingEnd {
                points.append(CGPoint(x: x0, y: sample.remainingStart))
            }
            points.append(CGPoint(x: x1, y: sample.remainingEnd))
        }
        return points
    }

    /// Same polyline in "used %" terms (rises with consumption, drops at a reset).
    public static func usedPath(_ samples: [HistorySample]) -> [CGPoint] {
        remainingPath(samples).map { CGPoint(x: $0.x, y: 100 - $0.y) }
    }

    /// Positions observed hours on the requested time axis; a new source does not acquire invented history.
    public static func usedPath(_ samples: [HistorySample], interval: DateInterval) -> [CGPoint] {
        let visible = samples.filter { $0.hourStart.addingTimeInterval(3600) > interval.start && $0.hourStart < interval.end }
            .sorted { $0.hourStart < $1.hourStart }
        var points: [CGPoint] = []
        for (index, sample) in visible.enumerated() {
            let x0 = max(0, sample.hourStart.timeIntervalSince(interval.start) / interval.duration)
            let x1 = min(1, sample.hourStart.addingTimeInterval(3600).timeIntervalSince(interval.start) / interval.duration)
            if index == 0 || sample.remainingStart != visible[index - 1].remainingEnd {
                points.append(CGPoint(x: x0, y: 100 - sample.remainingStart))
            }
            points.append(CGPoint(x: x1, y: 100 - sample.remainingEnd))
        }
        return points
    }

    /// Sums `values` into `buckets` groups of (nearly) equal size, preserving order.
    public static func bucketed(_ values: [Int], buckets: Int) -> [Int] {
        guard buckets > 0, !values.isEmpty else { return [] }
        var result = Array(repeating: 0, count: min(buckets, values.count))
        let count = values.count
        for (i, value) in values.enumerated() {
            let bucket = min(result.count - 1, i * result.count / count)
            result[bucket] += value
        }
        return result
    }

    /// Bucket original events on local quarter-hour/hour boundaries within the exact, half-open range.
    /// Empty periods retain their position, and integer token counts are never rounded or interpolated.
    public static func tokenBars(usage: [UsageEvent], agentIds: [String], range: StatsRange, bucketSize: TokenBucketSize = .hour1, now: Date, calendar: Calendar = .current, dimensions: TokenDimensions = .fresh) -> [TokenColumn] {
        let interval = range.interval(endingAt: now)
        let duration = bucketSize.duration
        let periods: [DateInterval]
        if bucketSize == .day1 {
            // Calendar days start at local midnight and can span 23 or 25 hours at DST changes.
            var days: [DateInterval] = []
            var start = calendar.startOfDay(for: interval.start)
            while start < interval.end {
                let day = calendar.dateInterval(of: .day, for: start)!
                days.append(day)
                start = day.end
            }
            periods = days
        } else {
            let firstHour = calendar.dateInterval(of: .hour, for: interval.start)!.start
            let firstBucket = firstHour.addingTimeInterval(floor(interval.start.timeIntervalSince(firstHour) / duration) * duration)
            let count = Int(ceil(interval.end.timeIntervalSince(firstBucket) / duration))
            periods = (0..<count).map { DateInterval(start: firstBucket.addingTimeInterval(Double($0) * duration), duration: duration) }
        }
        let dayIndices = bucketSize == .day1
            ? Dictionary(uniqueKeysWithValues: periods.enumerated().map { ($0.element.start, $0.offset) }) : [:]
        var values = Array(repeating: Array(repeating: 0, count: agentIds.count), count: periods.count)
        let indices = Dictionary(uniqueKeysWithValues: agentIds.enumerated().map { ($0.element, $0.offset) })
        for event in usage where event.timestamp >= interval.start && event.timestamp < interval.end {
            guard let agent = indices[event.agentId] else { continue }
            let column = bucketSize == .day1 ? dayIndices[calendar.startOfDay(for: event.timestamp)]!
                : Int(event.timestamp.timeIntervalSince(periods[0].start) / duration)
            values[column][agent] += dimensions.count(event)
        }
        return values.enumerated().map { index, tokens in
            TokenColumn(interval: periods[index], tokens: tokens)
        }
    }

    /// Hit-test whole time buckets, including empty ones and the chart's rightmost edge.
    public static func tokenColumn(at date: Date, in columns: [TokenColumn]) -> TokenColumn? {
        columns.first { date >= $0.interval.start && date < $0.interval.end }
            ?? columns.last.flatMap { date == $0.interval.end ? $0 : nil }
    }

    /// Four x-axis labels: three absolute hours and "现在 HH:mm".
    public static func axisLabels(range: StatsRange, now: Date, calendar: Calendar = .current) -> [String] {
        let hours = Double(range.hours)
        let offsets = [hours, hours * 2 / 3, hours / 3]
        let earlier = offsets.map { offset -> String in
            let date = now.addingTimeInterval(-offset * 3600)
            return weekdayTime(date, calendar: calendar)
        }
        let hh = calendar.component(.hour, from: now), mm = calendar.component(.minute, from: now)
        return earlier + [L10n.text("现在", "Now") + String(format: " %02d:%02d", hh, mm)]
    }

    public static func weekdayName(_ date: Date, calendar: Calendar = .current) -> String {
        L10n.weekdayNames[calendar.component(.weekday, from: date) - 1]
    }

    /// "周二 16:10"
    public static func weekdayTime(_ date: Date, calendar: Calendar = .current) -> String {
        let hh = calendar.component(.hour, from: date), mm = calendar.component(.minute, from: date)
        return "\(weekdayName(date, calendar: calendar)) \(String(format: "%02d:%02d", hh, mm))"
    }

}
