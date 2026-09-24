import Foundation

/// Providers that write their token events to the usage ledger themselves.
protocol LedgerRecording {}

/// Joins independent vendors, reading them one after another into one ledger pass. A missing or signed-out source
/// does not suppress another vendor's data, and a failing source keeps the usage it recorded before.
public struct CombinedUsageProvider: UsageProvider {
    public struct Source: Sendable {
        public let vendor: String
        public let provider: any UsageProvider
        public init(_ vendor: String, _ provider: any UsageProvider) { self.vendor = vendor; self.provider = provider }
    }
    /// The last result of every vendor, reused while its signals stay quiet.
    private actor Results {
        struct Entry {
            let result: Result<UsageReport, UsageProviderError>
            let agents: [AgentDescriptor]
            let historyHours: Int
        }
        private var entries: [String: Entry] = [:]
        func entry(_ vendor: String) -> Entry? { entries[vendor] }
        func store(_ entry: Entry, for vendor: String) { entries[vendor] = entry }
        func reports() -> [String: UsageReport] { entries.compactMapValues { try? $0.result.get() } }

        /// Each session's breakdown with the counts it was read at, valid while the ledger keeps the writes it was read from.
        struct Breakdown: Sendable {
            let counts: [Int]
            let usage: SessionUsage?
        }
        private var breakdowns: [String: Breakdown] = [:]
        private var breakdownGeneration: Int?
        /// The ledger's write mark when the breakdowns were read.
        private var breakdownMark = 0
        func breakdowns(generation: Int) -> (values: [String: Breakdown], mark: Int) {
            if breakdownGeneration != generation { breakdowns = [:]; breakdownGeneration = generation; breakdownMark = 0 }
            return (breakdowns, breakdownMark)
        }
        func storeBreakdowns(_ values: [String: Breakdown], mark: Int) { breakdowns = values; breakdownMark = mark }
    }

    private let vendors: [Source]
    private let ledger: UsageLedger
    private let results = Results()
    public init(_ sources: [Source], ledger: UsageLedger = .inMemory()) {
        vendors = sources
        self.ledger = ledger
    }

    public static func standard(ledger: UsageLedger = .open()) -> CombinedUsageProvider {
        removeLegacyCaches(in: AppSupport.directory)
        return CombinedUsageProvider([
            Source("Claude", ClaudeCodeProvider.standard(ledger: ledger)),
            Source("Codex", CodexUsageProvider.standard(ledger: ledger)),
            Source("DeepSeek", DeepSeekUsageProvider.standard(ledger: ledger)),
        ] + AdditionalSource.allCases.map { Source($0.vendor, AdditionalUsageProvider.standard($0, ledger: ledger)) }
          + [Source("Open agents", OpenAgentUsageProvider.standard(ledger: ledger))], ledger: ledger)
    }

    public func refreshAccountUsage(historyHours: Int) async {
        for step in accountRefreshSteps { await step(historyHours) }
    }

    public var accountRefreshSteps: [AccountRefreshStep] { vendors.flatMap(\.provider.accountRefreshSteps) }

    public var watchedDirectories: [URL]? {
        var directories: [URL] = []
        for source in vendors {
            guard let watched = source.provider.watchedDirectories else { return nil }
            directories += watched
        }
        return directories
    }

    /// One source per vendor, so a change under one client's directories reads only that client.
    public var sources: [UsageSource] {
        vendors.map { UsageSource(name: $0.vendor, directories: $0.provider.watchedDirectories, accountSteps: $0.provider.accountRefreshSteps) }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        try await fetchUsage(agents: agents, historyHours: historyHours, sources: nil)
    }

    /// Vendors outside `names` keep their last result, unless they have none yet or it was read for other agents or hours.
    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int, sources names: Set<String>?) async throws -> UsageReport {
        await ledger.beginPass()
        var results: [(Int, UsageReport?, String?)] = []
        for (index, source) in vendors.enumerated() {
            let previous = await self.results.entry(source.vendor)
            let result: Result<UsageReport, UsageProviderError>
            if let previous, names.map({ !$0.contains(source.vendor) }) ?? false,
               previous.agents == agents, previous.historyHours == historyHours {
                result = previous.result
            } else {
                do { result = .success(try await source.provider.fetchUsage(agents: agents, historyHours: historyHours)) }
                catch { result = .failure(UsageProviderError(error.localizedDescription)) }
                await self.results.store(.init(result: result, agents: agents, historyHours: historyHours), for: source.vendor)
            }
            switch result {
            case .success(let report): results.append((index, report, nil))
            case .failure(let error): results.append((index, nil, error.message))
            }
        }
        await ledger.commitPass()
        let reports = results.compactMap { $0.1 }
        var notices: [String: String] = [:]
        for (index, report, error) in results {
            if let report { notices.merge(report.sourceNotices, uniquingKeysWith: { _, new in new }) }
            if let message = error ?? (report?.sourceNotices.isEmpty == true ? report?.notice : nil) { notices[vendors[index].vendor] = message }
        }
        guard !reports.isEmpty else { throw UsageProviderError(notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · ")) }
        let now = reports.map(\.generatedAt).max() ?? Date()
        let weekAgo = now.addingTimeInterval(-7 * 86400)
        // The ledger holds every recorded source, including one whose refresh just failed; other providers report periods themselves.
        let reported = results.filter { !(vendors[$0.0].provider is any LedgerRecording) }.flatMap { $0.1?.usage ?? [] }
        let usage = ((try? await ledger.buckets(since: min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600)))) ?? []) + reported
        var periods = (try? await ledger.periods(endingAt: now)) ?? UsagePeriods()
        periods.add(reported, endingAt: now)
        let sessions = reports.flatMap(\.sessions)
        let progress = reports.compactMap(\.indexing)
        let sessionUsage = await breakdowns(of: sessions)
        return UsageReport(generatedAt: now, snapshots: Dictionary(grouping: reports.flatMap(\.snapshots), by: \.agentId).values.compactMap { $0.max { $0.updatedAt < $1.updatedAt } }.sorted { $0.agentId < $1.agentId },
                           sessions: sessions.sorted { a, b in
                               if a.isLive != b.isLive { return a.isLive }
                               return (a.endedAt ?? a.startedAt) > (b.endedAt ?? b.startedAt)
                           },
                           notice: notices.isEmpty ? nil : notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · "),
                           discoveredAgents: UsageAggregation.consumersUnion(reports.map(\.discoveredAgents)), consumers: UsageAggregation.consumersUnion(reports.map(\.consumers)),
                           usage: usage.sorted { ($0.start, $0.account ?? "", $0.agentId) < ($1.start, $1.account ?? "", $1.agentId) },
                           indexing: progress.isEmpty ? nil : IndexProgress(done: progress.reduce(0) { $0 + $1.done }, total: progress.reduce(0) { $0 + $1.total }),
                           insightsByAgent: reports.reduce(into: [:]) { $0.merge($1.insightsByAgent, uniquingKeysWith: { _, new in new }) },
                           subscriptions: reports.reduce(into: [:]) { $0.merge($1.subscriptions, uniquingKeysWith: { _, new in new }) }, sourceNotices: notices,
                           consumerIdsByQuota: reports.reduce(into: [:]) { $0.merge($1.consumerIdsByQuota, uniquingKeysWith: { $0.union($1) }) },
                           billing: Self.mergeBilling(reports.flatMap(\.billing)), codexResetCredits: reports.first { $0.codexResetCredits != nil }?.codexResetCredits,
                           codexResetCreditsObservedAt: reports.first { $0.codexResetCredits != nil }?.codexResetCreditsObservedAt,
                           completions: reports.flatMap(\.completions),
                           turns: reports.flatMap(\.turns), services: AgentService.merge(reports.map { $0.services ?? [] }),
                           activeQuotaPoolIDs: reports.compactMap(\.activeQuotaPoolIDs).reduce(into: [String: Set<String>]()) {
                               $0.merge($1, uniquingKeysWith: { $0.union($1) })
                           },
                           accounts: reports.compactMap(\.accounts).reduce(into: [String: [AccountObservation]]()) {
                               $0.merge($1, uniquingKeysWith: +)
                           },
                           forgottenAccountProviders: reports.compactMap(\.forgottenAccountProviders).reduce(nil) { ($0 ?? []).union($1) },
                           sessionUsage: sessionUsage, periods: periods)
    }

    /// Where each session's tokens went. A session is read again when its counts move or when the ledger has written any
    /// log it is made of since its breakdown was read: sub-agents spend without the session's own log changing, and a
    /// source can record a session's events a pass or more after first reporting it.
    private func breakdowns(of sessions: [LiveSession]) async -> [String: SessionUsage] {
        let (known, mark) = await results.breakdowns(generation: await ledger.generation)
        let changed = await ledger.changedKeys(after: mark), now = await ledger.writeMark
        var kept: [String: Results.Breakdown] = [:], requests: [SessionUsageRequest] = []
        for session in sessions {
            let counts = [session.tokensIn, session.tokensOut, session.cacheReadTokens], request = SessionUsageRequest(session)
            if let breakdown = known[session.id], breakdown.counts == counts, !request.touches(changed) {
                kept[session.id] = breakdown
            } else {
                requests.append(request)
            }
        }
        if !requests.isEmpty, let read = try? await ledger.sessionUsage(requests) {
            let counts = Dictionary(sessions.map { ($0.id, [$0.tokensIn, $0.tokensOut, $0.cacheReadTokens]) }, uniquingKeysWith: { first, _ in first })
            for request in requests { kept[request.sessionID] = .init(counts: counts[request.sessionID] ?? [], usage: read[request.sessionID]) }
            await results.storeBreakdowns(kept, mark: now)
        } else {
            // Nothing could be read: keep what was known, and the mark it was read at, so the next pass reads it again.
            for request in requests { kept[request.sessionID] = known[request.sessionID] }
            await results.storeBreakdowns(kept, mark: requests.isEmpty ? now : mark)
        }
        return kept.compactMapValues(\.usage)
    }
    /// Each vendor's checks come from its last result, so a quiet vendor is read again only when its activity ages.
    public func sourceChecks() async -> [String: [Date]] {
        await results.reports().mapValues(\.activityChecks).filter { !$0.value.isEmpty }
    }

    /// Each vendor's next account reading comes from its last result: its quota moves while its own work runs. A vendor
    /// whose last read failed has no result to judge and keeps the account interval.
    public func accountChecks(since: [String: Date], now: Date) async -> [String: Date] {
        let reports = await results.reports()
        return vendors.reduce(into: [:]) { due, vendor in
            guard let last = since[vendor.vendor], let report = reports[vendor.vendor] else { return }
            due[vendor.vendor] = report.accountCheck(since: last, now: now, seesLocalWork: vendor.provider.seesLocalWork)
        }
    }

    /// Parse caches and report copies of earlier versions, replaced by the usage ledger.
    static func removeLegacyCaches(in directory: URL) {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: directory.path) else { return }
        for name in names where name.hasSuffix(".json") && ["transcripts-cache", "codex-transcripts-", "deepseek-transcripts-"].contains(where: name.hasPrefix) {
            try? FileManager.default.removeItem(at: directory.appendingPathComponent(name))
        }
    }

    static func mergeBilling(_ values: [APIBilling]) -> [APIBilling] {
        Dictionary(grouping: values, by: \.id).values.compactMap { observations in
            guard let latest = observations.max(by: { ($0.updatedAt ?? .distantPast) < ($1.updatedAt ?? .distantPast) }) else { return nil }
            // Costs come from the ledger; the newest observation that carries them is the most complete.
            let costs = observations.sorted { ($0.updatedAt ?? .distantPast) > ($1.updatedAt ?? .distantPast) }
                .first { !$0.costs.isEmpty || !$0.sessionCosts.isEmpty } ?? latest
            return APIBilling(vendor: latest.billingPool?.provider ?? latest.vendor, balances: latest.balances, isAvailable: latest.isAvailable,
                updatedAt: latest.updatedAt, costs: costs.costs, sessionCosts: costs.sessionCosts,
                notice: latest.notice, billingPool: latest.billingPool)
        }.sorted { $0.id < $1.id }
    }

}
