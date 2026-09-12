import Foundation

/// Serves the design's demo data with deterministic seeded history.
public struct DemoUsageProvider: UsageProvider {
    public init() {}

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = Date()
        return Self.report(agents: agents, historyHours: historyHours, now: now)
    }

    public static func report(agents: [AgentDescriptor], historyHours: Int, now: Date) -> UsageReport {
        let consumers = agents.filter { DemoData.quota[$0.id] != nil }
        let hourStart = Calendar.current.date(bySetting: .minute, value: 0, of: now).map {
            Calendar.current.date(bySetting: .second, value: 0, of: $0) ?? $0
        } ?? now
        let tokens = DemoSeries.hourlyTokens(agentCount: max(1, consumers.count), hours: historyHours)
        var history: [HistorySample] = []
        var consumption: [UsageEvent] = []
        for (index, agent) in consumers.enumerated() {
            let candles = DemoSeries.series(count: historyHours, seed: DemoSeries.lineSeed(index: index))
            for (hour, candle) in candles.enumerated() {
                let start = hourStart.addingTimeInterval(TimeInterval(hour - historyHours + 1) * 3600)
                if agent.enabled {
                    history.append(HistorySample(
                        agentId: agent.id,
                        hourStart: start,
                        remainingStart: candle.open,
                        remainingEnd: candle.close,
                        tokens: tokens[hour][index] * 1000
                    ))
                }
                // Demo events retain sub-hour positions so every chart granularity is populated.
                for quarter in 0..<4 {
                    let timestamp = start.addingTimeInterval(Double(quarter) * 900)
                    guard timestamp <= now else { continue }
                    let total = tokens[hour][index] * 250
                    consumption.append(.init(timestamp: timestamp, agentId: agent.id,
                                             tokensIn: total * 4 / 5, tokensOut: total - total * 4 / 5, cacheReadTokens: total * 2))
                }
            }
        }
        return UsageReport(
            generatedAt: now,
            snapshots: DemoData.snapshots(now: now),
            sessions: DemoData.sessions(now: now),
            history: history,
            activity: UsageAnalytics.activityGrid(usage: consumption, since: now.addingTimeInterval(-7 * 86400), calendar: .current),
            insights: DemoData.insights(now: now),
            consumers: consumers,
            consumption: consumption,
            subscriptions: ["Claude": "max_20x", "Codex": "prolite"],
            codexResetCredits: DemoData.codexResetCredits(now: now)
        )
    }
}
