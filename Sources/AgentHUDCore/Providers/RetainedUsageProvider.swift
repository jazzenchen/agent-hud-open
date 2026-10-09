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
    public func fileChanges(_ paths: Set<String>?) async { await provider.fileChanges(paths) }
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
        let report = latest.map { incoming.retainingReadings(from: $0) } ?? incoming.startingRowClocks()
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
                    quotaNotices: quotaNotices, readingIssues: readingIssues, consumerIdsByQuota: consumerIdsByQuota, billing: billing,
                    codexResetCredits: codexResetCredits,
                    codexResetCreditsObservedAt: codexResetCreditsObservedAt, services: services, activeQuotaPoolIDs: activeQuotaPoolIDs,
                    accounts: accounts, forgottenAccountProviders: forgottenAccountProviders, periods: periods,
                    rowSeenAt: rowSeenAt)
    }

    /// The first report of a run with nothing kept from before: every row it lists was seen now.
    func startingRowClocks() -> UsageReport {
        UsageReport(generatedAt: generatedAt, snapshots: snapshots, sessions: sessions, notice: notice,
                    discoveredAgents: discoveredAgents, consumers: consumers, usage: usage, indexing: indexing,
                    insightsByAgent: insightsByAgent, subscriptions: subscriptions, sourceNotices: sourceNotices, quotaNotices: quotaNotices,
                    readingIssues: readingIssues, consumerIdsByQuota: consumerIdsByQuota, billing: billing, codexResetCredits: codexResetCredits,
                    codexResetCreditsObservedAt: codexResetCreditsObservedAt, completions: completions, turns: turns,
                    services: services, activeQuotaPoolIDs: activeQuotaPoolIDs, accounts: accounts,
                    forgottenAccountProviders: forgottenAccountProviders, sessionUsage: sessionUsage, periods: periods,
                    rowSeenAt: Dictionary(discoveredAgents.map { ($0.id, generatedAt) }, uniquingKeysWith: { first, _ in first }))
    }

    /// An absent reading is not a zero or a confirmed reset. Keep its original observation time.
    func retainingReadings(from previous: UsageReport) -> UsageReport {
        /// A plan pool the provider's inventory leaves out retires with its rows, readings and account.
        func isActive(_ agent: AgentDescriptor) -> Bool {
            ReportView.isPresent(agent, seenAt: nil, accounts: nil, activePools: activeQuotaPoolIDs)
        }
        let retiredPoolIDs = Set(previous.discoveredAgents.filter { !isActive($0) }.compactMap { $0.billingPool?.id })
        let cutoff = generatedAt.addingTimeInterval(-QuotaHistoryStore.retention)
        // Only a provider's explicit, sound complete inventory retires omitted windows. A window without a new snapshot
        // keeps its last reading; a snapshot with an unknown value replaces it. Failed reads keep every last reading.
        let inventories = completeQuotaWindowInventories
        let accounts = mergedAccounts(from: previous, retiredPoolIDs: retiredPoolIDs, cutoff: cutoff)
        // Every row a provider reports is seen now; rows from a report that kept no times start their clock now.
        let reportedIDs = Set(discoveredAgents.map(\.id))
        var seen = previous.rowSeenAt ?? [:]
        for agent in previous.discoveredAgents where seen[agent.id] == nil { seen[agent.id] = generatedAt }
        for id in reportedIDs { seen[id] = generatedAt }
        for id in inventories.values.flatMap({ $0 }) { seen[id] = generatedAt }
        // A row retires with its readings and settings once it is no longer present: no provider reported it for the
        // retention period, whatever took it away (a client uninstalled, a window the service dropped, a vendor the app no
        // longer reads), its account left its provider's inventory, its plan pool is inactive, or it has no account while
        // its provider identifies them. A complete account inventory also retires the windows it left out.
        let recent = seen.filter { $0.value >= cutoff }
        let retired = previous.discoveredAgents.filter { agent in
            if let accountID = agent.displayAccountID, let inventory = inventories[accountID], !inventory.contains(agent.id) { return true }
            return !ReportView.isPresent(agent, seenAt: recent, accounts: accounts, activePools: activeQuotaPoolIDs)
        }
        let absentWindowIDs = Set(retired.map(\.id))
        let previousRows = previous.discoveredAgents.filter { !absentWindowIDs.contains($0.id) }
        // Older caches can already mix the two schemas. A still-present summary supersedes both retained and incoming
        // legacy rows, without asserting that the summary's other windows form this pass's complete inventory.
        let superseded = AntigravityClient.supersededLegacyWindowIDs(in: discoveredAgents + previousRows,
                                                                   by: discoveredAgents + previousRows)
        let retiredWindowIDs = absentWindowIDs.union(superseded)
        func isRetained(_ agent: AgentDescriptor) -> Bool { !retiredWindowIDs.contains(agent.id) }
        let reportedRows = discoveredAgents.filter { !superseded.contains($0.id) }
        let reportedSnapshots = snapshots.filter { !superseded.contains($0.agentId) }
        let currentIDs = Set(reportedSnapshots.map(\.agentId))
        let billingIDs = Set(billing.map(\.id))
        let retainedBilling = billing.map { value -> APIBilling in
            guard value.updatedAt == nil, let old = previous.billing.first(where: { $0.id == value.id }) else { return value }
            return APIBilling(vendor: value.vendor, balances: old.balances, isAvailable: old.isAvailable,
                updatedAt: old.updatedAt, costs: value.costs, sessionCosts: value.sessionCosts, notice: value.notice,
                readingIssue: value.readingIssue, billingPool: value.billingPool)
        } + previous.billing.filter { !billingIDs.contains($0.id) }
        let knownAgents = UsageAggregation.consumersUnion([discoveredAgents, consumers, previous.discoveredAgents, previous.consumers])
        // A source whose read or quota reading failed keeps its last sessions; a notice about its local logs or hooks does not.
        let failedIDs = Set(knownAgents.filter { !vendorStatus($0.vendor).isNormal }.map(\.id))
        // Bot's account-scoped local inventory owns its sessions; an unrelated CLI quota failure must not restore a
        // roster that a sign-out or account switch removed. Its file reader already retains unreadable files.
        let retainedSessions = UsageAggregation.sessionsUnion([sessions, previous.sessions.filter {
            failedIDs.contains($0.agentId) && $0.client != "Grok Bot"
        }])
        let retainedSnapshots = reportedSnapshots + previous.snapshots.filter { !currentIDs.contains($0.agentId) && !retiredWindowIDs.contains($0.agentId) }
        let rows = AntigravityClient.summaryNames(UsageAggregation.consumersUnion([reportedRows, previous.discoveredAgents.filter(isRetained)]),
                                                 snapshots: retainedSnapshots)
        return UsageReport(generatedAt: generatedAt,
            snapshots: retainedSnapshots,
            sessions: retainedSessions,
            notice: notice,
            discoveredAgents: rows,
            consumers: UsageAggregation.consumersUnion([consumers, previous.consumers.filter(isActive)]),
            // Recorded usage outlives a failed refresh in the ledger, so the new report's periods are complete.
            usage: usage,
            indexing: indexing,
            insightsByAgent: previous.insightsByAgent.filter { !retiredWindowIDs.contains($0.key) }
                .merging(insightsByAgent.filter { !superseded.contains($0.key) }, uniquingKeysWith: { _, new in new }),
            subscriptions: previous.subscriptions.filter { !retiredPoolIDs.contains($0.key) }.merging(subscriptions, uniquingKeysWith: { _, new in new }),
            sourceNotices: sourceNotices, quotaNotices: quotaNotices, readingIssues: readingIssues,
            consumerIdsByQuota: previous.consumerIdsByQuota.filter { !retiredWindowIDs.contains($0.key) }
                .merging(consumerIdsByQuota.filter { !superseded.contains($0.key) }, uniquingKeysWith: { _, new in new }),
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
            periods: periods,
            rowSeenAt: Dictionary(rows.map { ($0.id, seen[$0.id] ?? generatedAt) }, uniquingKeysWith: { first, _ in first }))
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
