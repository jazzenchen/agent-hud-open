import Foundation

/// Port of the prototype's random series so the demo charts look exactly like the design.
public enum DemoSeries {
    public struct Candle: Hashable, Sendable {
        public let open: Double
        public let close: Double
        public let high: Double
        public let low: Double
    }

    /// Hourly remaining %, starting at 100, dropping randomly, resetting to 100 every 5th bucket.
    public static func series(count: Int, seed: Int) -> [Candle] {
        var random = SeededRandom(seed: seed)
        var value = 100.0
        var out: [Candle] = []
        for i in 0..<count {
            let open = value
            var high = value, low = value
            if i % 5 == 4 {
                value = 100
                high = 100
            } else {
                for _ in 0..<3 {
                    let magnitude = random.next()
                    let spike = random.next() < 0.25 ? 2.2 : 1.0
                    value = max(2, value - magnitude * 9 * spike)
                    low = min(low, value)
                }
            }
            out.append(Candle(open: open, close: value, high: max(high, open, value), low: min(low, open, value)))
        }
        return out
    }

    /// Hourly token consumption (in thousands) per agent: `[hour][agent]`. Working hours 9–23 are busy.
    public static func hourlyTokens(agentCount: Int, hours: Int, seed: Int = 3) -> [[Int]] {
        var random = SeededRandom(seed: seed)
        return (0..<hours).map { hour in
            (0..<agentCount).map { agent in
                let dayHour = (hour + 14) % 24
                let weight = (dayHour >= 9 && dayHour <= 23) ? 1.0 : 0.15
                return Int((random.next() * weight * (agent == 0 ? 40 : 18)).rounded())
            }
        }
    }

    /// 7 × 24 demo token totals (Mon → Sun).
    public static func activity(seed: Int = 7) -> ActivityGrid {
        var random = SeededRandom(seed: seed)
        let tokensByModel: [[[String: Int]]] = (0..<7).map { day in
            (0..<24).map { hour in
                let work: Double
                if hour >= 9 && hour <= 23 && day < 5 {
                    work = 1
                } else if hour >= 13 && hour <= 22 {
                    work = 0.7
                } else {
                    work = 0.1
                }
                return ["demo": Int(min(1, random.next() * work * 1.3) * 1_000_000)]
            }
        }
        return ActivityGrid(tokensByModel: tokensByModel)
    }

    /// Seeds used for the four enabled agents in the design; extra agents get derived seeds.
    public static func lineSeed(index: Int) -> Int {
        let seeds = [5, 17, 29, 41]
        return index < seeds.count ? seeds[index] : 53 + index * 12
    }
}
