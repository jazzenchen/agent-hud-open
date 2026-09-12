import Foundation

/// Keeps the last successful readings across partial failures and app restarts.
public actor RetainedUsageProvider: UsageProvider {
    public nonisolated let initialReport: UsageReport?
    private let provider: any UsageProvider
    private let cacheURL: URL?
    private var latest: UsageReport?

    public init(provider: any UsageProvider, cacheURL: URL? = nil) {
        self.provider = provider
        self.cacheURL = cacheURL
        let saved = cacheURL.flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? JSONDecoder().decode(UsageReport.self, from: $0) }
        initialReport = saved
        latest = saved
    }

    public func refreshAccountUsage(historyHours: Int) async { await provider.refreshAccountUsage(historyHours: historyHours) }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let incoming = try await provider.fetchUsage(agents: agents, historyHours: historyHours)
        try Task.checkCancellation()
        let report = latest.map { incoming.retainingReadings(from: $0) } ?? incoming
        latest = report
        if let cacheURL {
            do {
                try FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
                try JSONEncoder().encode(report).write(to: cacheURL, options: .atomic)
            } catch { NSLog("[AgentHUD] Reading cache write failed: %@", error.localizedDescription) }
        }
        return report
    }
}

extension UsageReport {
    /// An absent reading is not a zero or a confirmed reset. Keep its original observation time.
    func retainingReadings(from previous: UsageReport) -> UsageReport {
        func isActive(_ agent: AgentDescriptor) -> Bool {
            guard let pool = agent.billingPool, pool.product == .plan,
                  let active = activeQuotaPoolIDs?[pool.provider] else { return true }
            return active.contains(pool.id)
        }
        let retired = previous.discoveredAgents.filter { !isActive($0) }
        let retiredWindowIDs = Set(retired.map(\.id))
        let retiredPoolIDs = Set(retired.compactMap { $0.billingPool?.id })
        let currentIDs = Set(snapshots.map(\.agentId))
        let billingIDs = Set(billing.map(\.id))
        let retainedBilling = billing.map { value -> APIBilling in
            guard value.updatedAt == nil, let old = previous.billing.first(where: { $0.id == value.id }) else { return value }
            return APIBilling(vendor: value.vendor, balances: old.balances, isAvailable: old.isAvailable,
                updatedAt: old.updatedAt, costs: value.costs, notice: value.notice, billingPool: value.billingPool)
        } + previous.billing.filter { !billingIDs.contains($0.id) }
        let knownAgents = UsageAggregation.consumersUnion([discoveredAgents, consumers, previous.discoveredAgents, previous.consumers])
        let failedIDs = Set(knownAgents.filter { sourceNotices[$0.vendor] != nil }.map(\.id))
        let retainedSessions = UsageAggregation.sessionsUnion([sessions, previous.sessions.filter { failedIDs.contains($0.agentId) }])
        return UsageReport(generatedAt: generatedAt,
            snapshots: snapshots + previous.snapshots.filter { !currentIDs.contains($0.agentId) && !retiredWindowIDs.contains($0.agentId) },
            sessions: retainedSessions, history: UsageAggregation.historyUnion([history, previous.history]).filter { $0.hourStart >= generatedAt.addingTimeInterval(-QuotaHistoryStore.retention) },
            activity: activity, insights: insights, notice: notice,
            discoveredAgents: UsageAggregation.consumersUnion([discoveredAgents, previous.discoveredAgents.filter(isActive)]),
            subscriptionType: subscriptionType ?? previous.subscriptionType,
            consumers: UsageAggregation.consumersUnion([consumers, previous.consumers.filter(isActive)]),
            consumption: UsageAggregation.usageUnion([consumption, previous.consumption.filter { failedIDs.contains($0.agentId) }]),
            indexing: indexing,
            insightsByAgent: previous.insightsByAgent.filter { !retiredWindowIDs.contains($0.key) }.merging(insightsByAgent, uniquingKeysWith: { _, new in new }),
            subscriptions: previous.subscriptions.filter { !retiredPoolIDs.contains($0.key) }.merging(subscriptions, uniquingKeysWith: { _, new in new }),
            sourceNotices: sourceNotices,
            consumerIdsByQuota: previous.consumerIdsByQuota.filter { !retiredWindowIDs.contains($0.key) }.merging(consumerIdsByQuota, uniquingKeysWith: { _, new in new }),
            billing: retainedBilling, codexResetCredits: codexResetCredits ?? previous.codexResetCredits,
            codexResetCreditsObservedAt: codexResetCredits != nil ? codexResetCreditsObservedAt : previous.codexResetCreditsObservedAt,
            completions: completions, claudeConsumptionSince: claudeConsumptionSince, turns: turns,
            services: AgentService.merge([(previous.services ?? []).filter { service in
                // The provider reports all currently usable clients, including during temporary quota failures.
                service.product != .plan || activeQuotaPoolIDs?[service.provider] == nil
            }, services ?? []]),
            activeQuotaPoolIDs: (previous.activeQuotaPoolIDs ?? [:]).merging(activeQuotaPoolIDs ?? [:], uniquingKeysWith: { _, new in new }))
    }
}
