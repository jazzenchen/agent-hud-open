import Foundation

/// The last successful local observations survive failed polls and app restarts.
/// Retention happens before cloud publication so every surface receives the same readings.
public actor RetainedUsageProvider: UsageProvider {
    public nonisolated let initialReport: UsageReport?
    private let provider: any UsageProvider
    private let cacheURL: URL?
    private var latest: UsageReport?

    public init(provider: any UsageProvider, cacheURL: URL? = nil) {
        self.provider = provider
        self.cacheURL = cacheURL
        initialReport = cacheURL.flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? JSONDecoder().decode(UsageReport.self, from: $0) }
        latest = initialReport
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let incoming: UsageReport
        do { incoming = try await provider.fetchUsage(agents: agents, historyHours: historyHours) }
        catch {
            try Task.checkCancellation()
            guard let latest else { throw error }
            NSLog("[AgentHUD] Local refresh failed: %@", error.localizedDescription)
            return latest
        }
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
            snapshots: snapshots + previous.snapshots.filter { !currentIDs.contains($0.agentId) },
            sessions: retainedSessions, history: UsageAggregation.historyUnion([history, previous.history]).filter { $0.hourStart >= generatedAt.addingTimeInterval(-QuotaHistoryStore.retention) },
            activity: activity, insights: insights, notice: notice,
            discoveredAgents: UsageAggregation.consumersUnion([discoveredAgents, previous.discoveredAgents]),
            subscriptionType: subscriptionType ?? previous.subscriptionType,
            consumers: UsageAggregation.consumersUnion([consumers, previous.consumers]),
            consumption: UsageAggregation.usageUnion([consumption, previous.consumption.filter { failedIDs.contains($0.agentId) }]),
            indexing: indexing,
            insightsByAgent: previous.insightsByAgent.merging(insightsByAgent, uniquingKeysWith: { _, new in new }),
            subscriptions: previous.subscriptions.merging(subscriptions, uniquingKeysWith: { _, new in new }),
            sourceNotices: sourceNotices,
            consumerIdsByQuota: previous.consumerIdsByQuota.merging(consumerIdsByQuota, uniquingKeysWith: { _, new in new }),
            billing: retainedBilling, codexResetCredits: codexResetCredits ?? previous.codexResetCredits,
            codexResetCreditsObservedAt: codexResetCredits != nil ? codexResetCreditsObservedAt : previous.codexResetCreditsObservedAt,
            completions: completions, claudeConsumptionSince: claudeConsumptionSince, turns: turns,
            services: AgentService.merge([previous.services ?? [], services ?? []]))
    }
}
