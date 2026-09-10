import Foundation

/// Joins independent vendors. A missing or signed-out source does not suppress another vendor's data.
public struct CombinedUsageProvider: UsageProvider {
    public struct Source: Sendable {
        public let vendor: String
        public let provider: any UsageProvider
        public init(_ vendor: String, _ provider: any UsageProvider) { self.vendor = vendor; self.provider = provider }
    }
    private let sources: [Source]
    public init(_ sources: [Source]) { self.sources = sources }

    public static func standard() -> CombinedUsageProvider {
        CombinedUsageProvider([
            Source("Claude", ClaudeCodeProvider.standard()),
            Source("Codex", CodexUsageProvider.standard()),
            Source("DeepSeek", DeepSeekUsageProvider.standard()),
        ] + AdditionalSource.allCases.map { Source($0.vendor, AdditionalUsageProvider.standard($0)) }
          + [Source("Open agents", OpenAgentUsageProvider.standard())])
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let results = await withTaskGroup(of: (Int, UsageReport?, String?).self) { group in
            for (index, source) in sources.enumerated() {
                group.addTask {
                    do { return (index, try await source.provider.fetchUsage(agents: agents, historyHours: historyHours), nil) }
                    catch { return (index, nil, error.localizedDescription) }
                }
            }
            var values: [(Int, UsageReport?, String?)] = []
            for await value in group { values.append(value) }
            return values.sorted { $0.0 < $1.0 }
        }
        let reports = results.compactMap { $0.1 }
        var notices: [String: String] = [:]
        for (index, report, error) in results {
            if let report { notices.merge(report.sourceNotices, uniquingKeysWith: { _, new in new }) }
            if let message = error ?? (report?.sourceNotices.isEmpty == true ? report?.notice : nil) { notices[sources[index].vendor] = message }
        }
        guard !reports.isEmpty else { throw UsageProviderError(notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · ")) }
        let now = reports.map(\.generatedAt).max() ?? Date()
        let consumption = UsageAggregation.usageUnion(reports.map(\.consumption))
        let weekAgo = now.addingTimeInterval(-7 * 86400)
        let week = consumption.filter { $0.timestamp >= weekAgo && $0.timestamp <= now }
        var totals: [String: Int] = [:]
        for sample in week {
            totals[sample.agentId, default: 0] += sample.total
        }
        let sum = totals.values.reduce(0, +)
        let progress = reports.compactMap(\.indexing)
        return UsageReport(generatedAt: now, snapshots: Dictionary(grouping: reports.flatMap(\.snapshots), by: \.agentId).values.compactMap { $0.max { $0.updatedAt < $1.updatedAt } }.sorted { $0.agentId < $1.agentId },
                           sessions: reports.flatMap(\.sessions).sorted { a, b in
                               if a.isLive != b.isLive { return a.isLive }
                               return (a.endedAt ?? a.startedAt) > (b.endedAt ?? b.startedAt)
                           }, history: UsageAggregation.historyUnion(reports.map(\.history)),
                           activity: UsageAnalytics.activityGrid(usage: week, since: weekAgo, calendar: .current),
                           insights: UsageInsights(burnRatePctPerHour: nil, timeToExhaust: nil, weeklyCapHits: 0,
                                                  weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                                                  weeklyShare: sum > 0 ? totals.mapValues { Double($0) / Double(sum) } : [:],
                                                  windowSessionCount: reports.reduce(0) { $0 + $1.insights.windowSessionCount }, windowUsedPct: 0),
                           notice: notices.isEmpty ? nil : notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · "),
                           discoveredAgents: UsageAggregation.consumersUnion(reports.map(\.discoveredAgents)), consumers: UsageAggregation.consumersUnion(reports.map(\.consumers)),
                           consumption: consumption, indexing: progress.isEmpty ? nil : IndexProgress(done: progress.reduce(0) { $0 + $1.done }, total: progress.reduce(0) { $0 + $1.total }),
                           insightsByAgent: reports.reduce(into: [:]) { $0.merge($1.insightsByAgent, uniquingKeysWith: { _, new in new }) },
                           subscriptions: reports.reduce(into: [:]) { $0.merge($1.subscriptions, uniquingKeysWith: { _, new in new }) }, sourceNotices: notices,
                           consumerIdsByQuota: reports.reduce(into: [:]) { $0.merge($1.consumerIdsByQuota, uniquingKeysWith: { $0.union($1) }) },
                           billing: Self.mergeBilling(reports.flatMap(\.billing)), codexResetCredits: reports.first { $0.codexResetCredits != nil }?.codexResetCredits,
                           codexResetCreditsObservedAt: reports.first { $0.codexResetCredits != nil }?.codexResetCreditsObservedAt,
                           completions: reports.flatMap(\.completions),
                           claudeConsumptionSince: reports.compactMap(\.claudeConsumptionSince).min(),
                           turns: reports.flatMap(\.turns), services: AgentService.merge(reports.map { $0.services ?? [] }))
    }
    static func mergeBilling(_ values: [APIBilling]) -> [APIBilling] {
        Dictionary(grouping: values, by: \.id).values.compactMap { observations in
            guard let latest = observations.max(by: { ($0.updatedAt ?? .distantPast) < ($1.updatedAt ?? .distantPast) }) else { return nil }
            return APIBilling(vendor: latest.billingPool?.provider ?? latest.vendor, balances: latest.balances, isAvailable: latest.isAvailable,
                updatedAt: latest.updatedAt, costs: UsageAggregation.costUnion(observations.sorted { ($0.updatedAt ?? .distantPast) > ($1.updatedAt ?? .distantPast) }.map(\.costs)),
                notice: latest.notice, billingPool: latest.billingPool)
        }.sorted { $0.id < $1.id }
    }

}
