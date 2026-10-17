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
    /// WorkBuddy's plan ceilings, from the 429s its transcripts logged; nil for every other source.
    private var ceilings: WorkBuddyCeilings?

    /// - ceilings: WorkBuddy's ceilings, kept in memory when not given.
    init(source: AdditionalSource, readQuota: @escaping @Sendable () async throws -> ProviderQuota,
         readSessions: @escaping @Sendable (Date) async -> ProviderSessions,
         history: QuotaHistoryStore,
         readCompletions: @escaping @Sendable (Date) throws -> [SessionCompletion] = { _ in [] },
         clock: @escaping @Sendable () -> Date = { Date() },
         botQuotaDirectory: URL? = nil,
         refreshSessions: @escaping @Sendable (Int) async -> Void = { _ in },
         watchedDirectories: [URL]? = nil, fileChanges: @escaping @Sendable (Set<String>?) async -> Void = { _ in },
         ceilings: WorkBuddyCeilings? = nil,
         ledger: UsageLedger = .inMemory()) {
        self.source = source; self.readQuota = readQuota; self.readSessions = readSessions
        noteChanges = fileChanges
        self.readCompletions = readCompletions
        self.refreshSessions = refreshSessions
        self.history = history; self.clock = clock
        self.botQuotaDirectory = botQuotaDirectory
        self.watchedDirectories = watchedDirectories
        self.ceilings = source == .workbuddy ? ceilings ?? WorkBuddyCeilings() : nil
        self.ledger = ledger
        sessionLedger = SessionLedger(source: source.rawValue, ledger: ledger)
    }

    /// `settings` holds the consent GitHub Copilot's quota reading waits for.
    /// `grokBotRoutineRuns` is asked to learn the runs behind newly billed sub-agent conversations, in Cursor's account step.
    static func standard(_ source: AdditionalSource, settings: SettingsStore, ledger: UsageLedger,
                         persistHistory: Bool = true, grokBotRoutineRuns: GrokBotRoutineRuns? = nil) -> AdditionalUsageProvider {
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
                let read = await cursor.sessions(since: Date().addingTimeInterval(-Double(max(168, hours)) * 3600))
                await grokBotRoutineRuns?.refresh(billed: read.sessions.map(\.id)) { try await cursor.grokBotAutomations(agentIDs: $0) }
            }
        }, watchedDirectories: local.roots + (CompletionHooks.Source(rawValue: source.rawValue).map {
            [CompletionHooks.directory.appendingPathComponent($0.rawValue)]
        } ?? []), fileChanges: { await local.fileChanges($0) },
        ceilings: source == .workbuddy
            ? WorkBuddyCeilings(fileURL: persistHistory ? AppSupport.directory.appendingPathComponent("workbuddy-ceilings.json") : nil) : nil,
        ledger: ledger)
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
        // WorkBuddy keeps its own names for its sessions and meters them itself; both come from its database in one read.
        let metering = source == .workbuddy ? WorkBuddyMetering.read(sessions: local.sessions, models: local.models) : nil
        if let titles = metering?.titles, !titles.isEmpty {
            for index in local.sessions.indices { if let title = titles[local.sessions[index].id] { local.sessions[index].title = title } }
        }
        if source == .grok {
            let targets = GrokSessionOrigins.read(sessionIDs: Set(local.sessions.filter { $0.client == "Grok CLI" }.map(\.id)))
            for index in local.sessions.indices where local.sessions[index].client == "Grok CLI" {
                local.sessions[index].navigationTarget = targets[local.sessions[index].id]
                for completion in local.sessions[index].completions.indices {
                    local.sessions[index].completions[completion].navigationTarget = targets[local.sessions[index].id]
                }
            }
        }
        // Tokens and models come from every session of `historyHours`; sessions, their turns and completions, and the
        // completion hooks' records only from those active in the session window that the client did not archive. A
        // billed Grok Bot run comes whatever its age, for its bot's tree; the merge keeps it only under a listed bot or
        // while it is recent itself.
        let sessionStart = SessionWindow.start(at: now)
        var listed = local.sessions.filter {
            !$0.archived && ($0.lastSeen.map { $0 >= sessionStart } == true || GrokBotUsage.billedSubagentID($0.id) != nil)
        }
        var hookCompletions: [SessionCompletion] = [], hookNotice: String?
        do { hookCompletions = try readCompletions(sessionStart) }
        catch { hookNotice = L10n.text("完成提醒记录读取失败", "Turn completion records could not be read") }
        // A stop hook finishes the running turns it follows when the client's own log records no end.
        let finished = Dictionary(hookCompletions.map { ($0.sessionID, RecordCoding.milliseconds($0.completedAt)) }, uniquingKeysWith: max)
        for index in listed.indices {
            guard let stop = finished[listed[index].id] else { continue }
            listed[index].turns = listed[index].turns.map { SessionPhase.stopped($0, atMs: stop) }
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
        var windows = quota.scopedWindows(source)
        var quotaWindowIDs = quota.quotaWindowIDs
        // WorkBuddy's ceilings say what its account is: while one is shown, or one the app shows is left to retire, the
        // account's windows are complete; with neither there is no account, rather than one no reading named.
        var noAccount = false
        if var ceilings {
            let rows = ceilings.rows(logged: local.limitHits, indexing: local.indexing != nil, agents: agents,
                                     apiReached: local.apiReached, account: account, now: now)
            self.ceilings = ceilings
            windows += rows.windows
            switch rows.inventory {
            case .pending: break
            case .none: noAccount = true
            case .complete(let ids): quotaWindowIDs = (quotaWindowIDs ?? Set(quota.windows.map(\.id))).union(ids)
            }
        }
        // Account-wide imports are the same on every machine signed into the account, so their totals are kept per account.
        let usageAccount = local.sessions.contains(where: \.accountWide)
            ? (quota.isSignedIn ? account.id : (failed ? lastAccount?.id : nil) ?? "provider:" + source.vendor.lowercased()) : nil
        await record(local, account: usageAccount, since: since, now: now)
        let consumers = Set(local.sessions.flatMap(\.events).map(\.model)).sorted().map {
            AgentDescriptor(id: "\(source.rawValue)-model:\($0)", vendor: source.vendor,
                            model: source == .workbuddy ? WorkBuddySessions.displayName($0, apiReached: local.apiReached)
                                                         : ModelCatalog.consumerName(of: "\(source.rawValue)-model:\($0)"),
                            source: L10n.sourceAdditionalUsage, enabled: true)
        }
        let sessions = listed.compactMap { item -> LiveSession? in
            guard let start = item.startedAt ?? item.events.map(\.timestamp).min(), let end = item.lastSeen else { return nil }
            let model = item.events.max { $0.timestamp < $1.timestamp }?.model ?? "Unknown"
            // The newest turn is the one observed last.
            let evidence = SessionPhase.SourceEvidence(turn: item.turns.max { $0.observedAtMs < $1.observedAtMs })
            let isRunning = SessionPhase.read(evidence, rule: .turns, at: now).inFlight
            return LiveSession(id: item.id, agentId: "\(source.rawValue)-model:\(model)", task: item.title,
                               terminal: item.workspace.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: start, endedAt: isRunning ? nil : end, pctOfWindow: nil,
                               tokensIn: item.events.reduce(0) { $0 + $1.input }, tokensOut: item.events.reduce(0) { $0 + $1.output },
                               client: item.client, transcriptPath: item.path,
                               cacheReadTokens: item.events.reduce(0) { $0 + $1.cacheRead }, accountWide: item.accountWide,
                               observedAt: evidence.newest,
                               workingDirectory: item.workspace, lastActivityAt: end, navigationTarget: item.navigationTarget,
                               activity: item.activity)
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
        // A plan's rows shown in this group, such as a GLM plan only WorkBuddy carries, are linked by their own provider to
        // the calls that went through the plan, never to every model this client used.
        let quotaIDs = Set(windows.map(\.id) + agents.filter { $0.vendor == source.vendor && $0.billingPool == nil }.map(\.id))
        let accounts: [String: [AccountObservation]]?
        // A declared inventory carries the account it belongs to even while the app's quota answer stays empty: the
        // ceilings make a reading of their own, and an inventory nobody reports cannot retire the rows it leaves out.
        if quota.isSignedIn || quotaWindowIDs != nil {
            accounts = [source.vendor: [AccountObservation(account: account, client: quota.client, label: quota.label,
                plan: quota.plan, observedAt: observedAt, aliases: quota.accountAliases,
                quotaWindowIDs: quotaWindowIDs.map { Set($0.map(account.windowID)) },
                wallets: quota.wallets.map { $0.observed(at: $0.observedAt ?? observedAt) }, sourceInfo: quota.sourceInfo)]]
        } else if quota.signedOut || noAccount, quotaNotice == nil {
            // A successful signed-out/absent client read confirms that previously retained accounts are no longer current,
            // and so does a WorkBuddy pass with no ceiling to show or retire.
            accounts = [source.vendor: []]
        } else { accounts = nil }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: sessions,
            notice: notice.isEmpty ? nil : notice, discoveredAgents: descriptors, consumers: consumers,
            indexing: local.indexing, insightsByAgent: insights, subscriptions: quota.plan.map { [source.vendor: $0] } ?? [:],
            sourceNotices: displayNotice.isEmpty ? [:] : [source.vendor: displayNotice], quotaNotices: quotaNotice.map { [source.vendor: $0] } ?? [:],
            readingIssues: quotaNotice.map { [source.vendor: .readFailed($0)] } ?? [:],
            consumerIdsByQuota: Dictionary(uniqueKeysWithValues: quotaIDs.map { ($0, consumerIDs) }),
            completions: listed.flatMap(\.completions) + hookCompletions, turns: listed.flatMap(\.turns),
            accounts: accounts,
            forgottenAccountProviders: quota.forgetAccounts ? [source.vendor] : nil,
            sessionCredits: (metering?.credits.isEmpty ?? true) ? nil : metering?.credits)
    }
}
