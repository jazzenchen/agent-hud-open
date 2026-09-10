import Foundation

/// Harness exposes local usage, not subscription quota windows.
public actor DeepSeekUsageProvider: UsageProvider {
    private let directory: URL
    private let transcripts: DeepSeekTranscriptStore
    private let clock: @Sendable () -> Date
    private let readBalance: @Sendable () async throws -> DeepSeekBalance?
    private let readProcessStarts: @Sendable () async -> [Date]
    private var lastBalance: (at: Date, result: Result<DeepSeekBalance?, UsageProviderError>)?

    public init(directory: URL, transcripts: DeepSeekTranscriptStore,
                readBalance: @escaping @Sendable () async throws -> DeepSeekBalance? = { nil },
                readProcessStarts: @escaping @Sendable () async -> [Date] = { [] },
                clock: @escaping @Sendable () -> Date = { Date() }) {
        self.directory = directory; self.transcripts = transcripts; self.clock = clock
        self.readBalance = readBalance
        self.readProcessStarts = readProcessStarts
    }

    public static func standard() -> DeepSeekUsageProvider {
        let directory = DeepSeekLocator.dataDirectory
        return DeepSeekUsageProvider(directory: directory, transcripts: DeepSeekTranscriptStore(
            root: directory.appendingPathComponent("sessions"),
            cacheURL: AppSupport.directory.appendingPathComponent("deepseek-transcripts-v1.json")),
            readBalance: { try await DeepSeekBalanceClient(directory: directory).fetch() },
            readProcessStarts: { await DeepSeekRuntime.processStarts(directory: directory) })
    }

    private func balance(now: Date) async -> (DeepSeekBalance?, Date?, String?) {
        guard DeepSeekLocator.isInstalled(directory: directory) else { return (nil, nil, nil) }
        if lastBalance == nil || now.timeIntervalSince(lastBalance!.at) >= 120 {
            do { lastBalance = (now, .success(try await readBalance())) }
            catch { lastBalance = (now, .failure(UsageProviderError(error.localizedDescription))) }
        }
        switch lastBalance!.result {
        case .success(let balance): return (balance, balance == nil ? nil : lastBalance!.at, nil)
        case .failure(let error): return (nil, nil, error.message)
        }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), weekAgo = now.addingTimeInterval(-7 * 86400)
        let cutoff = min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600))
        async let balanceResult = balance(now: now)
        async let processStartsResult = readProcessStarts()
        let indexed = await transcripts.index(since: cutoff)
        let processStarts = await processStartsResult
        let (balance, balanceAt, balanceNotice) = await balanceResult
        let installed = DeepSeekLocator.isInstalled(directory: directory)
        let events = indexed.sessions.flatMap { $0.transcript.usage.map(\.event) }.filter { $0.timestamp >= cutoff && $0.timestamp <= now }
        let models = Set(indexed.sessions.flatMap { [$0.transcript.model] + $0.transcript.usage.map(\.model) }).sorted()
        let consumers = models.map { AgentDescriptor(id: "deepseek-model:\($0)", vendor: "DeepSeek", model: $0,
                                                     source: L10n.sourceDeepSeekSessions, enabled: true) }
        let sessions = indexed.sessions.filter { !$0.transcript.isSubagent }.map { session in
            let t = session.transcript
            return LiveSession(id: "deepseek:\(t.id!)", agentId: "deepseek-model:\(t.model)",
                               task: t.title ?? t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "DeepSeek Harness",
                               terminal: t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: t.startedAt ?? session.modifiedAt,
                               endedAt: t.isLive(now: now, modifiedAt: session.modifiedAt, processStarts: processStarts) ? nil : (t.lastActivityAt ?? t.startedAt ?? session.modifiedAt),
                               pctOfWindow: nil, tokensIn: t.usage.reduce(0) { $0 + $1.input }, tokensOut: t.usage.reduce(0) { $0 + $1.output },
                               client: "DeepSeek Harness", transcriptPath: session.path,
                               cacheReadTokens: t.usage.reduce(0) { $0 + $1.cachedInput })
        }.sorted { a, b in
            if a.isLive != b.isLive { return a.isLive }
            return (a.endedAt ?? a.startedAt) > (b.endedAt ?? b.startedAt)
        }
        let descriptor = AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness",
                                         source: L10n.sourceDeepSeekSessions, enabled: false)
        let discovered = consumers.isEmpty
            ? (agents.contains { $0.id.hasPrefix("deepseek-model:") } ? [] : [descriptor]) : consumers
        let notice = [indexed.notice, balanceNotice].compactMap { $0 }.joined(separator: " · ")
        let costs = indexed.sessions.flatMap { session in
            session.transcript.usage.map { usage in
                APIBilling.CostSample(timestamp: usage.timestamp, sessionId: "deepseek:\(session.transcript.id!)", model: usage.model,
                                      amounts: Dictionary(uniqueKeysWithValues: ["CNY", "USD"].compactMap { currency in
                    DeepSeekPricing.estimate(usage, currency: currency).map { (currency, $0) }
                }))
            }
        }
        let billing = APIBilling(vendor: "DeepSeek", balances: balance?.balances ?? [], isAvailable: balance?.isAvailable,
                                 updatedAt: balanceAt, costs: costs, notice: balanceNotice)
        return UsageReport(generatedAt: now, snapshots: [], sessions: sessions, history: [],
                           activity: UsageAnalytics.activityGrid(usage: events, since: weekAgo, calendar: .current),
                           insights: UsageInsights(burnRatePctPerHour: nil, timeToExhaust: nil, weeklyCapHits: 0,
                                                  weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                                                  weeklyShare: UsageAnalytics.weeklyShare(usage: events.filter { $0.timestamp >= weekAgo }),
                                                  windowSessionCount: sessions.count, windowUsedPct: 0),
                           notice: notice.isEmpty ? nil : notice, discoveredAgents: installed ? discovered : [], consumers: consumers,
                           consumption: events, indexing: indexed.indexing,
                           sourceNotices: notice.isEmpty ? [:] : ["DeepSeek": notice], billing: installed ? [billing] : [],
                           completions: indexed.sessions.flatMap { $0.transcript.completions ?? [] })
    }
}
