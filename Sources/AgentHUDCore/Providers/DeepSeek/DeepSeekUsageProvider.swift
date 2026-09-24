import Foundation

/// Harness exposes local usage, not subscription quota windows.
public actor DeepSeekUsageProvider: UsageProvider, LedgerRecording {
    private let directory: URL
    private let transcripts: DeepSeekTranscriptStore
    private let clock: @Sendable () -> Date
    private let readBalance: @Sendable () async throws -> DeepSeekBalance?
    private let readProcessStarts: @Sendable () async -> [Date]
    private var lastBalance: (at: Date, result: Result<DeepSeekBalance?, UsageProviderError>)?
    private var lastProcessStarts: (at: Date, starts: [Date])?

    public init(directory: URL, transcripts: DeepSeekTranscriptStore,
                readBalance: @escaping @Sendable () async throws -> DeepSeekBalance? = { nil },
                readProcessStarts: @escaping @Sendable () async -> [Date] = { [] },
                clock: @escaping @Sendable () -> Date = { Date() }) {
        self.directory = directory; self.transcripts = transcripts; self.clock = clock
        self.readBalance = readBalance
        self.readProcessStarts = readProcessStarts
    }

    public static func standard(ledger: UsageLedger) -> DeepSeekUsageProvider {
        let directory = DeepSeekLocator.dataDirectory
        return DeepSeekUsageProvider(directory: directory, transcripts: DeepSeekTranscriptStore(
            root: directory.appendingPathComponent("sessions"), ledger: ledger),
            readBalance: { try await DeepSeekBalanceClient(directory: directory).fetch() },
            readProcessStarts: { await DeepSeekRuntime.processStarts(directory: directory) })
    }

    public nonisolated var watchedDirectories: [URL]? { [transcripts.root] }

    public func refreshAccountUsage(historyHours: Int) async {
        guard DeepSeekLocator.isInstalled(directory: directory) else { return }
        let now = clock()
        if lastBalance == nil || now.timeIntervalSince(lastBalance!.at) >= UsageRefresh.accountRequestSpacing {
            do { lastBalance = (now, .success(try await readBalance())) }
            catch {
                if Task.isCancelled { return }
                lastBalance = (now, .failure(UsageProviderError(error.localizedDescription)))
            }
        }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), weekAgo = now.addingTimeInterval(-7 * 86400)
        let cutoff = min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600))
        let indexed = await transcripts.index(since: cutoff)
        let processStarts = await processStarts(for: indexed.sessions, now: now)
        let balance: DeepSeekBalance?, balanceAt: Date?, balanceNotice: String?
        switch lastBalance?.result {
        case .success(let value): (balance, balanceAt, balanceNotice) = (value, value == nil ? nil : lastBalance?.at, nil)
        case .failure(let error): (balance, balanceAt, balanceNotice) = (nil, nil, error.message)
        case nil: (balance, balanceAt, balanceNotice) = (nil, nil, nil)
        }
        let installed = DeepSeekLocator.isInstalled(directory: directory)
        let models = Set(indexed.sessions.flatMap { [$0.transcript.model] + $0.transcript.models }).sorted()
        let consumers = models.map { AgentDescriptor(id: "deepseek-model:\($0)", vendor: "DeepSeek", model: $0,
                                                     source: L10n.sourceDeepSeekSessions, enabled: true) }
        let sessions = indexed.sessions.filter { !$0.transcript.isSubagent }.map { session in
            let t = session.transcript
            return LiveSession(id: "deepseek:\(t.id!)", agentId: "deepseek-model:\(t.model)",
                               task: t.title ?? t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "DeepSeek Harness",
                               terminal: t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: t.startedAt ?? session.modifiedAt,
                               endedAt: t.isLive(processStarts: processStarts) ? nil : (t.lastActivityAt ?? t.startedAt ?? session.modifiedAt),
                               pctOfWindow: nil, tokensIn: t.inputTokens, tokensOut: t.outputTokens,
                               client: "DeepSeek Harness", transcriptPath: session.path,
                               cacheReadTokens: t.cachedInputTokens, observedAt: now, workingDirectory: t.cwd)
        }.sorted { a, b in
            if a.isLive != b.isLive { return a.isLive }
            return (a.endedAt ?? a.startedAt) > (b.endedAt ?? b.startedAt)
        }
        let descriptor = AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness",
                                         source: L10n.sourceDeepSeekSessions, enabled: false)
        let discovered = consumers.isEmpty
            ? (agents.contains { $0.id.hasPrefix("deepseek-model:") } ? [] : [descriptor]) : consumers
        let notice = [indexed.notice, balanceNotice].compactMap { $0 }.joined(separator: " · ")
        let costs = await transcripts.costs(since: cutoff)
        var sessionCosts: [String: [String: Decimal]] = [:]
        for session in indexed.sessions {
            if let estimate = costs.logs[session.path] { sessionCosts["deepseek:\(session.transcript.id!)"] = estimate }
        }
        let billing = APIBilling(vendor: "DeepSeek", balances: balance?.balances ?? [], isAvailable: balance?.isAvailable,
                                 updatedAt: balanceAt, costs: costs.buckets, sessionCosts: sessionCosts, notice: balanceNotice)
        return UsageReport(generatedAt: now, snapshots: [], sessions: sessions,
                           notice: notice.isEmpty ? nil : notice, discoveredAgents: installed ? discovered : [], consumers: consumers,
                           indexing: indexed.indexing,
                           sourceNotices: notice.isEmpty ? [:] : ["DeepSeek": notice], billing: installed ? [billing] : [],
                           completions: indexed.sessions.flatMap { $0.transcript.completions ?? [] },
                           turns: indexed.sessions.flatMap { $0.transcript.sessionTurns })
    }

    /// Process starts only decide a running turn whose log went quiet, so the process table is inspected only then,
    /// at most every 30 seconds. Nil means it was not consulted and the turn keeps running.
    private func processStarts(for sessions: [DeepSeekTranscriptStore.Session], now: Date) async -> [Date]? {
        let quiet = sessions.contains { session in
            !session.transcript.isSubagent && session.transcript.sessionTurns.last?.state == .running
                && now.timeIntervalSince(session.modifiedAt) >= 120
        }
        guard quiet else { lastProcessStarts = nil; return nil }
        if let last = lastProcessStarts, now.timeIntervalSince(last.at) < 30 { return last.starts }
        let starts = await readProcessStarts()
        lastProcessStarts = (now, starts)
        return starts
    }
}
