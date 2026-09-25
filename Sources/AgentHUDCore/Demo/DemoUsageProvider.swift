import Foundation

/// Serves the design's demo data with deterministic seeded usage.
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
        var usage: [UsageBucket] = []
        // The weeks before the history fill the month the charts can show, an hour to a bucket, from a series of their own
        // so the recent week looks the same whatever the month holds.
        let earlier = max(0, StatsRange.days30.hours - historyHours)
        let older = DemoSeries.hourlyTokens(agentCount: max(1, consumers.count), hours: earlier, seed: 5)
        for (index, agent) in consumers.enumerated() {
            for hour in 0..<earlier {
                let total = older[hour][index] * 1_000
                usage.append(.init(start: hourStart.addingTimeInterval(TimeInterval(hour - earlier - historyHours + 1) * 3600), agentId: agent.id,
                                   tokensIn: total * 4 / 5, tokensOut: total - total * 4 / 5, cacheReadTokens: total * 2))
            }
            for hour in 0..<historyHours {
                let start = hourStart.addingTimeInterval(TimeInterval(hour - historyHours + 1) * 3600)
                // Demo usage fills every quarter hour so every chart granularity is populated.
                for quarter in 0..<4 {
                    let bucket = start.addingTimeInterval(Double(quarter) * 900)
                    guard bucket <= now else { continue }
                    let total = tokens[hour][index] * 250
                    usage.append(.init(start: bucket, agentId: agent.id,
                                       tokensIn: total * 4 / 5, tokensOut: total - total * 4 / 5, cacheReadTokens: total * 2))
                }
            }
        }
        var periods = UsagePeriods()
        periods.add(usage, endingAt: now)
        return UsageReport(
            generatedAt: now,
            snapshots: DemoData.snapshots(now: now),
            sessions: DemoData.sessions(now: now),
            // The demo reports the rows it was given, as a provider reports the rows it read.
            discoveredAgents: agents,
            consumers: consumers,
            usage: usage,
            insightsByAgent: Dictionary(uniqueKeysWithValues: consumers.map { ($0.id, DemoData.insights(now: now)) }),
            subscriptions: ["Claude": "max_20x", "Codex": "prolite"],
            consumerIdsByQuota: Dictionary(uniqueKeysWithValues: consumers.map { ($0.id, Set([$0.id])) }),
            codexResetCredits: DemoData.codexResetCredits(now: now),
            sessionUsage: DemoData.sessionUsage(now: now),
            periods: periods
        )
    }
}
