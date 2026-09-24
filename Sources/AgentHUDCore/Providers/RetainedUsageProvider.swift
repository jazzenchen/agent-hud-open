import Foundation

/// Keeps the last successful readings across partial failures and app restarts.
public actor RetainedUsageProvider: UsageProvider {
    public nonisolated let initialReport: UsageReport?
    private let provider: any UsageProvider
    private let cacheURL: URL?
    /// Local polls run every few seconds; the restart copy is rewritten at most this often.
    private let saveInterval: TimeInterval
    private var latest: UsageReport?
    private var savedAt: Date?

    public init(provider: any UsageProvider, cacheURL: URL? = nil, saveInterval: TimeInterval = UsageRefresh.accountInterval) {
        self.provider = provider
        self.cacheURL = cacheURL
        self.saveInterval = saveInterval
        let saved = cacheURL.flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? JSONDecoder().decode(UsageReport.self, from: $0) }
        initialReport = saved
        latest = saved
    }

    public func refreshAccountUsage(historyHours: Int) async { await provider.refreshAccountUsage(historyHours: historyHours) }
    public nonisolated var accountRefreshSteps: [AccountRefreshStep] { provider.accountRefreshSteps }
    public nonisolated var watchedDirectories: [URL]? { provider.watchedDirectories }
    public nonisolated var sources: [UsageSource] { provider.sources }
    public func sourceChecks() async -> [String: [Date]] { await provider.sourceChecks() }
    public func accountChecks(since: [String: Date], now: Date) async -> [String: Date] {
        await provider.accountChecks(since: since, now: now)
    }
    public nonisolated var seesLocalWork: Bool { provider.seesLocalWork }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        try await fetchUsage(agents: agents, historyHours: historyHours, sources: nil)
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int, sources: Set<String>?) async throws -> UsageReport {
        let incoming = try await provider.fetchUsage(agents: agents, historyHours: historyHours, sources: sources)
        try Task.checkCancellation()
        let report = latest.map { incoming.retainingReadings(from: $0) } ?? incoming
        latest = report
        if let cacheURL, savedAt.map({ report.generatedAt.timeIntervalSince($0) >= saveInterval }) ?? true {
            savedAt = report.generatedAt
            do {
                try FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
                try JSONEncoder().encode(report.restartCopy).write(to: cacheURL, options: .atomic)
            } catch { NSLog("[AgentHUD] Reading cache write failed: %@", error.localizedDescription) }
        }
        return report
    }
}

extension UsageReport {
    /// Turns and completions only matter as they happen, and a restart never reports earlier ones, so the copy leaves them out.
    /// Session breakdowns are read from the ledger again by the first pass, so the copy that is rewritten every few minutes
    /// does not carry a week of them.
    var restartCopy: UsageReport {
        UsageReport(generatedAt: generatedAt, snapshots: snapshots, sessions: sessions,
                    notice: notice, discoveredAgents: discoveredAgents, consumers: consumers, usage: usage,
                    indexing: indexing, insightsByAgent: insightsByAgent, subscriptions: subscriptions, sourceNotices: sourceNotices,
                    consumerIdsByQuota: consumerIdsByQuota, billing: billing, codexResetCredits: codexResetCredits,
                    codexResetCreditsObservedAt: codexResetCreditsObservedAt, services: services, activeQuotaPoolIDs: activeQuotaPoolIDs,
                    accounts: accounts, forgottenAccountProviders: forgottenAccountProviders, periods: periods)
    }

    /// An absent reading is not a zero or a confirmed reset. Keep its original observation time.
    func retainingReadings(from previous: UsageReport) -> UsageReport {
        func isActive(_ agent: AgentDescriptor) -> Bool {
            guard let pool = agent.billingPool, pool.product == .plan,
                  let active = activeQuotaPoolIDs?[pool.provider] else { return true }
            return active.contains(pool.id)
        }
        let retiredPools = previous.discoveredAgents.filter { !isActive($0) }
        let retiredPoolIDs = Set(retiredPools.compactMap { $0.billingPool?.id })
        let cutoff = generatedAt.addingTimeInterval(-QuotaHistoryStore.retention)
        let currentIDs = Set(snapshots.map(\.agentId))
        // A successful Codex response is the complete window inventory for that account.
        // Omitted buckets are retired; a failed read or an account switched away keeps its last readings.
        let confirmedCodex = Set((self.accounts?["Codex"] ?? []).filter {
            $0.isCurrent && $0.quotaNotice == nil && sourceNotices["Codex"] == nil
        }.map(\.account.id))
        let accounts = mergedAccounts(from: previous, retiredPoolIDs: retiredPoolIDs, cutoff: cutoff)
        let knownAccountIDs = Set(accounts?.values.flatMap { $0.map(\.account.id) } ?? [])
        // Rows of an account unseen for the retention period retire with its readings and settings.
        // Rows without an account belong to a provider version that could not identify accounts; they are dropped.
        let retiredRows = previous.discoveredAgents.filter { agent in
            if let account = agent.account, confirmedCodex.contains(account.id), !currentIDs.contains(agent.id) { return true }
            if let account = agent.account, agent.billingPool == nil { return accounts != nil && !knownAccountIDs.contains(account.id) }
            return agent.account == nil && agent.billingPool == nil && accounts?[agent.vendor]?.isEmpty == false
        }
        let retired = retiredPools + retiredRows
        let retiredWindowIDs = Set(retired.map(\.id))
        func isRetained(_ agent: AgentDescriptor) -> Bool { isActive(agent) && !retiredWindowIDs.contains(agent.id) }
        let billingIDs = Set(billing.map(\.id))
        let retainedBilling = billing.map { value -> APIBilling in
            guard value.updatedAt == nil, let old = previous.billing.first(where: { $0.id == value.id }) else { return value }
            return APIBilling(vendor: value.vendor, balances: old.balances, isAvailable: old.isAvailable,
                updatedAt: old.updatedAt, costs: value.costs, sessionCosts: value.sessionCosts, notice: value.notice, billingPool: value.billingPool)
        } + previous.billing.filter { !billingIDs.contains($0.id) }
        let knownAgents = UsageAggregation.consumersUnion([discoveredAgents, consumers, previous.discoveredAgents, previous.consumers])
        let failedIDs = Set(knownAgents.filter { sourceNotices[$0.vendor] != nil }.map(\.id))
        let retainedSessions = UsageAggregation.sessionsUnion([sessions, previous.sessions.filter { failedIDs.contains($0.agentId) }])
        return UsageReport(generatedAt: generatedAt,
            snapshots: snapshots + previous.snapshots.filter { !currentIDs.contains($0.agentId) && !retiredWindowIDs.contains($0.agentId) },
            sessions: retainedSessions,
            notice: notice,
            discoveredAgents: UsageAggregation.consumersUnion([discoveredAgents, previous.discoveredAgents.filter(isRetained)]),
            consumers: UsageAggregation.consumersUnion([consumers, previous.consumers.filter(isActive)]),
            // Recorded usage outlives a failed refresh in the ledger, so the new report's periods are complete.
            usage: usage,
            indexing: indexing,
            insightsByAgent: previous.insightsByAgent.filter { !retiredWindowIDs.contains($0.key) }.merging(insightsByAgent, uniquingKeysWith: { _, new in new }),
            subscriptions: previous.subscriptions.filter { !retiredPoolIDs.contains($0.key) }.merging(subscriptions, uniquingKeysWith: { _, new in new }),
            sourceNotices: sourceNotices,
            consumerIdsByQuota: previous.consumerIdsByQuota.filter { !retiredWindowIDs.contains($0.key) }.merging(consumerIdsByQuota, uniquingKeysWith: { _, new in new }),
            billing: retainedBilling, codexResetCredits: codexResetCredits ?? (sameCurrentAccount(as: previous, provider: "Codex") ? previous.codexResetCredits : nil),
            codexResetCreditsObservedAt: codexResetCredits != nil ? codexResetCreditsObservedAt
                : sameCurrentAccount(as: previous, provider: "Codex") ? previous.codexResetCreditsObservedAt : nil,
            completions: completions, turns: turns,
            services: AgentService.merge([(previous.services ?? []).filter { service in
                // The provider reports all currently usable clients, including during temporary quota failures.
                service.product != .plan || activeQuotaPoolIDs?[service.provider] == nil
            }, services ?? []]),
            activeQuotaPoolIDs: (previous.activeQuotaPoolIDs ?? [:]).merging(activeQuotaPoolIDs ?? [:], uniquingKeysWith: { _, new in new }),
            accounts: accounts,
            sessionUsage: Self.retainedSessionUsage(sessionUsage, previous: previous.sessionUsage, sessions: retainedSessions),
            // Like `usage`, the periods come from the ledger, which keeps what a failed refresh recorded before.
            periods: periods)
    }

    /// Sessions kept from a failed source keep the breakdown they had.
    private static func retainedSessionUsage(_ current: [String: SessionUsage]?, previous: [String: SessionUsage]?,
                                             sessions: [LiveSession]) -> [String: SessionUsage]? {
        guard let previous else { return current }
        var merged = current ?? [:]
        for session in sessions where merged[session.id] == nil { merged[session.id] = previous[session.id] }
        return merged
    }

    /// A provider's reported list replaces its current accounts; accounts it no longer reports become last readings.
    /// A provider that did not report this poll keeps its previous inventory unchanged, and a forgotten provider has none.
    private func mergedAccounts(from previous: UsageReport, retiredPoolIDs: Set<String>, cutoff: Date) -> [String: [AccountObservation]]? {
        guard accounts != nil || previous.accounts != nil || forgottenAccountProviders != nil else { return nil }
        var merged = previous.accounts ?? [:]
        for (provider, current) in accounts ?? [:] {
            let ids = Set(current.map(\.id)), aliases = Set(current.flatMap { $0.aliases ?? [] })
            merged[provider] = current + (previous.accounts?[provider] ?? []).filter {
                !ids.contains($0.id) && !aliases.contains($0.account.id)
            }.map { $0.with(isCurrent: false) }
        }
        for provider in forgottenAccountProviders ?? [] {
            merged[provider] = []
        }
        return merged.mapValues { observations in
            observations.filter { $0.observedAt >= cutoff && !retiredPoolIDs.contains($0.account.id) }
        }
    }

    private func sameCurrentAccount(as previous: UsageReport, provider: String) -> Bool {
        guard let current = accounts?[provider] else { return true }
        return Set(current.filter(\.isCurrent).map(\.account.id)) == Set((previous.accounts?[provider] ?? []).filter(\.isCurrent).map(\.account.id))
    }
}
