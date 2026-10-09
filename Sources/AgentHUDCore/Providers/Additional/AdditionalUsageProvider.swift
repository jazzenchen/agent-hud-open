import AgentHUDSupport
import Foundation

actor AdditionalUsageProvider: UsageProvider, LedgerRecording {
    let source: AdditionalSource
    private let readQuota: @Sendable () async throws -> ProviderQuota
    private let readSessions: @Sendable (Date) async -> ProviderSessions
    private let noteChanges: @Sendable (Set<String>?) async -> Void
    private let refreshSessions: @Sendable (Int) async -> Void
    private let readCompletions: @Sendable (Date) throws -> [SessionCompletion]
    private let history: QuotaHistoryStore
    private let clock: @Sendable () -> Date
    private let botQuotaDirectory: URL?
    private var lastQuota: (at: Date, result: Result<ProviderQuota, UsageProviderError>)?
    /// The account the last reading that succeeded resolved, which a failed reading keeps: usage kept per account must
    /// not move to another key and back whenever a quota request fails.
    private var lastAccount: ProviderAccount?
    nonisolated let watchedDirectories: [URL]?
    /// Cursor's usage is the account's, from every device it signs in on, so this Mac going quiet says nothing about it.
    nonisolated var seesLocalWork: Bool { source != .cursor }
    private let ledger: UsageLedger
    private let sessionLedger: SessionLedger

    init(source: AdditionalSource, readQuota: @escaping @Sendable () async throws -> ProviderQuota,
         readSessions: @escaping @Sendable (Date) async -> ProviderSessions,
         history: QuotaHistoryStore,
         readCompletions: @escaping @Sendable (Date) throws -> [SessionCompletion] = { _ in [] },
         clock: @escaping @Sendable () -> Date = { Date() },
         botQuotaDirectory: URL? = nil,
         refreshSessions: @escaping @Sendable (Int) async -> Void = { _ in },
         watchedDirectories: [URL]? = nil, fileChanges: @escaping @Sendable (Set<String>?) async -> Void = { _ in },
         ledger: UsageLedger = .inMemory()) {
        self.source = source; self.readQuota = readQuota; self.readSessions = readSessions
        noteChanges = fileChanges
        self.readCompletions = readCompletions
        self.refreshSessions = refreshSessions
        self.history = history; self.clock = clock
        self.botQuotaDirectory = botQuotaDirectory
        self.watchedDirectories = watchedDirectories
        self.ledger = ledger
        sessionLedger = SessionLedger(source: source.rawValue, ledger: ledger)
    }

    /// `settings` holds the consent GitHub Copilot's quota reading waits for.
    static func standard(_ source: AdditionalSource, settings: SettingsStore, ledger: UsageLedger,
                         persistHistory: Bool = true) -> AdditionalUsageProvider {
        let local = AdditionalLocalStore(source: source)
        let cursor = CursorClient()
        let grok = GrokClient()
        let consented = CopilotClient.consent(in: settings)
        return AdditionalUsageProvider(source: source, readQuota: {
            switch source {
            case .antigravity: return try await AntigravityClient().fetch()
            case .cursor: return try await cursor.quota()
            case .grok: return try await grok.fetch()
            case .copilot: return try await CopilotClient(enabled: consented).fetch()
            case .openclaw, .hermes, .zcode, .codebuddy, .workbuddy, .qwen: return ProviderQuota()
            }
        }, readSessions: { since in
            if source == .cursor { return await cursor.savedSessions }
            return await local.index(since: since)
        }, history: QuotaHistoryStore(ledger: ledger, scope: source.rawValue,
            importing: persistHistory ? AppSupport.directory.appendingPathComponent("\(source.rawValue)-quota-history.json") : nil),
        readCompletions: { since in
            guard let hook = CompletionHooks.Source(rawValue: source.rawValue) else { return [] }
            return try CompletionHooks.read(source: hook, since: since)
        }, botQuotaDirectory: source == .grok ? grok.botDirectory : nil, refreshSessions: { hours in
            if source == .cursor {
                _ = await cursor.sessions(since: Date().addingTimeInterval(-Double(max(168, hours)) * 3600))
            }
        }, watchedDirectories: local.roots + (CompletionHooks.Source(rawValue: source.rawValue).map {
            [CompletionHooks.directory.appendingPathComponent($0.rawValue)]
        } ?? []), fileChanges: { await local.fileChanges($0) }, ledger: ledger)
    }

    func fileChanges(_ paths: Set<String>?) async {
        await noteChanges(paths)
        if source == .grok, let paths, let botQuotaDirectory,
           GrokBotQuota.isQuotaChange(paths, in: botQuotaDirectory) {
            await refreshAccountUsage(historyHours: 24)
        }
    }

    /// This source's 15-minute token totals from the period holding `since`.
    func usage(since: Date) async -> [UsageBucket] {
        (try? await ledger.buckets(since: since, source: source.rawValue)) ?? []
    }

    /// Writes each session's usage once the index is complete and something changed.
    private func record(_ local: ProviderSessions, account: String?, since: Date, now: Date) async {
        guard local.indexing == nil else { return }
        // A database reader returns its own last days, and Cursor's dashboard its days from local midnight.
        let readerStart = local.start ?? source.readerWindow.map { SessionContributions.nextDayStart(now.addingTimeInterval(-$0)) }
        let window = SessionContributions.windowStart(since, readerStart: readerStart)
        await sessionLedger.record(files: local.files, revision: local.revision, account: account, window: window,
                                   runningTotals: source == .hermes, now: now) {
            local.sessions.map { session in (session.id, session.events.map { $0.usage(source: source) }) }
        }
    }

    func refreshAccountUsage(historyHours: Int) async {
        await refreshSessions(historyHours)
        let now = clock()
        do {
            let result = try await readQuota()
            try Task.checkCancellation()
            let observedAt = result.observedAt ?? now
            lastQuota = (observedAt, .success(result))
            if result.forgetAccounts { await history.removeAll() }
            await history.append(result.scopedWindows(source).compactMap { window in
                window.remaining.map { .init(agentId: window.id, timestamp: window.observedAt ?? observedAt, remainingPct: $0) }
            }, now: now)
        } catch {
            if Task.isCancelled { return }
            lastQuota = (now, .failure(UsageProviderError(error.localizedDescription)))
        }
    }

    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), weekAgo = now.addingTimeInterval(-AlertPolicy.insightsLookback)
        let since = min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600))
        var local = await readSessions(since)
        if source == .grok {
            let targets = GrokSessionOrigins.read(sessionIDs: Set(local.sessions.filter { $0.client == "Grok CLI" }.map(\.id)))
            for index in local.sessions.indices where local.sessions[index].client == "Grok CLI" {
                local.sessions[index].navigationTarget = targets[local.sessions[index].id]
                for completion in local.sessions[index].completions.indices {
                    local.sessions[index].completions[completion].navigationTarget = targets[local.sessions[index].id]
                }
            }
        }
        var hookCompletions: [SessionCompletion] = [], hookNotice: String?
        do { hookCompletions = try readCompletions(since) }
        catch { hookNotice = L10n.text("完成提醒记录读取失败", "Turn completion records could not be read") }
        // A stop hook finishes the running turns it follows when the client's own log records no end.
        let finished = Dictionary(hookCompletions.map { ($0.sessionID, RecordCoding.milliseconds($0.completedAt)) }, uniquingKeysWith: max)
        for index in local.sessions.indices {
            guard let stop = finished[local.sessions[index].id] else { continue }
            local.sessions[index].turns = local.sessions[index].turns.map { SessionPhase.stopped($0, atMs: stop) }
        }
        let quota: ProviderQuota, quotaNotice: String?
        let observedAt = lastQuota?.at ?? now
        switch lastQuota?.result {
        case .success(let value): (quota, quotaNotice) = (value, value.notice)
        case .failure(let error): (quota, quotaNotice) = (ProviderQuota(), error.message)
        case nil: (quota, quotaNotice) = (ProviderQuota(), nil)
        }
        let account = quota.resolvedAccount(source)
        if quota.forgetAccounts { lastAccount = nil }
        if quota.isSignedIn { lastAccount = account }
        let failed = if case .failure = lastQuota?.result { true } else { false }
        let windows = quota.scopedWindows(source)
        // Account-wide imports are the same on every machine signed into the account, so their totals are kept per account.
        let usageAccount = local.sessions.contains(where: \.accountWide)
            ? (quota.isSignedIn ? account.id : (failed ? lastAccount?.id : nil) ?? "provider:" + source.vendor.lowercased()) : nil
        await record(local, account: usageAccount, since: since, now: now)
        let consumers = Set(local.sessions.flatMap(\.events).map(\.model)).sorted().map {
            AgentDescriptor(id: "\(source.rawValue)-model:\($0)", vendor: source.vendor, model: ModelCatalog.consumerName(of: "\(source.rawValue)-model:\($0)"),
                            source: L10n.sourceAdditionalUsage, enabled: true)
        }
        let sessions = local.sessions.compactMap { item -> LiveSession? in
            guard let start = item.startedAt ?? item.events.map(\.timestamp).min(),
                  let end = item.lastActivity ?? item.events.map(\.timestamp).max(), end >= since else { return nil }
            let model = item.events.max { $0.timestamp < $1.timestamp }?.model ?? "Unknown"
            // The newest turn is the one observed last.
            let turn = item.turns.max { $0.observedAtMs < $1.observedAtMs }
            let isRunning = SessionPhase.read(.init(turn: turn), rule: .turns, at: now).inFlight
            return LiveSession(id: item.id, agentId: "\(source.rawValue)-model:\(model)", task: item.title,
                               terminal: item.workspace.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: start, endedAt: isRunning ? nil : end, pctOfWindow: nil,
                               tokensIn: item.events.reduce(0) { $0 + $1.input }, tokensOut: item.events.reduce(0) { $0 + $1.output },
                               client: item.client, transcriptPath: item.path,
                               cacheReadTokens: item.events.reduce(0) { $0 + $1.cacheRead }, accountWide: item.accountWide, observedAt: now,
                               workingDirectory: item.workspace, lastActivityAt: end, navigationTarget: item.navigationTarget)
        }
        let snapshots = windows.map {
            UsageSnapshot(agentId: $0.id, remainingPct: $0.remaining, resetAt: $0.reset, windowDuration: $0.duration,
                          updatedAt: $0.observedAt ?? observedAt)
        }
        var insights: [String: UsageInsights] = [:]
        for snapshot in snapshots {
            let readings = await history.samples(agentId: snapshot.agentId, since: QuotaMath.historyStart(for: snapshot, now: now))
            insights[snapshot.agentId] = QuotaMath.insights(snapshot: snapshot, samples: readings, now: now)
        }
        // Every notice is shown; only a quota reading that failed holds back the vendor's alerts, levels and retained sessions.
        let displayNotice = [quota.displayNotice, local.notice, hookNotice].compactMap { $0 }.joined(separator: " · ")
        let notice = [quotaNotice, displayNotice.isEmpty ? nil : displayNotice].compactMap { $0 }.joined(separator: " · ")
        let descriptors = windows.map {
            AgentDescriptor(id: $0.id, vendor: source.vendor, model: $0.label, shortModel: $0.shortLabel ?? $0.label,
                            source: L10n.sourceAdditionalUsage, enabled: true, account: account, allModels: $0.allModels)
        }
        let consumerIDs = Set(consumers.map(\.id))
        let quotaIDs = Set(windows.map(\.id) + agents.filter { $0.vendor == source.vendor }.map(\.id))
        let accounts: [String: [AccountObservation]]?
        if quota.isSignedIn {
            accounts = [source.vendor: [AccountObservation(account: account, client: quota.client, label: quota.label,
                plan: quota.plan, observedAt: observedAt, aliases: quota.accountAliases,
                quotaWindowIDs: quota.quotaWindowIDs.map { Set($0.map(account.windowID)) },
                wallets: quota.wallets.map { $0.observed(at: $0.observedAt ?? observedAt) }, sourceInfo: quota.sourceInfo)]]
        } else if quota.signedOut, quotaNotice == nil {
            // A successful signed-out/absent client read confirms that previously retained accounts are no longer current.
            accounts = [source.vendor: []]
        } else { accounts = nil }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: sessions,
            notice: notice.isEmpty ? nil : notice, discoveredAgents: descriptors, consumers: consumers,
            indexing: local.indexing, insightsByAgent: insights, subscriptions: quota.plan.map { [source.vendor: $0] } ?? [:],
            sourceNotices: displayNotice.isEmpty ? [:] : [source.vendor: displayNotice], quotaNotices: quotaNotice.map { [source.vendor: $0] } ?? [:],
            readingIssues: quotaNotice.map { [source.vendor: .readFailed($0)] } ?? [:],
            consumerIdsByQuota: Dictionary(uniqueKeysWithValues: quotaIDs.map { ($0, consumerIDs) }),
            completions: local.sessions.flatMap(\.completions) + hookCompletions, turns: local.sessions.flatMap(\.turns),
            accounts: accounts,
            forgottenAccountProviders: quota.forgetAccounts ? [source.vendor] : nil)
    }
}
