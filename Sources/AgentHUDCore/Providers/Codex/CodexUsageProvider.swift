import Foundation

public actor CodexUsageProvider: UsageProvider {
    private let readLimits: @Sendable () async throws -> CodexRateLimits
    private let transcripts: CodexTranscriptStore
    private let history: QuotaHistoryStore
    private let clock: @Sendable () -> Date
    private var lastQuota: (at: Date, result: Result<CodexRateLimits, UsageProviderError>)?

    public init(readLimits: @escaping @Sendable () async throws -> CodexRateLimits,
                transcripts: CodexTranscriptStore, history: QuotaHistoryStore,
                clock: @escaping @Sendable () -> Date = { Date() }) {
        self.readLimits = readLimits; self.transcripts = transcripts; self.history = history; self.clock = clock
    }

    public static func standard() -> CodexUsageProvider {
        let directory = CodexLocator.dataDirectory
        return CodexUsageProvider(readLimits: {
            guard let executable = CodexLocator.find() else {
                throw UsageProviderError(L10n.text("安装并登录后即可读取额度", "Install and sign in to read quota"))
            }
            return try await CodexAppServerClient(executable: executable, dataDirectory: directory).fetch()
        }, transcripts: .standard(directory: directory),
           history: QuotaHistoryStore(fileURL: AppSupport.directory.appendingPathComponent("codex-quota-history.json")))
    }

    private func quota(now: Date) async -> (CodexRateLimits?, Date, String?) {
        if lastQuota == nil || now.timeIntervalSince(lastQuota!.at) >= 120 {
            do {
                let limits = try await readLimits()
                lastQuota = (now, .success(limits))
                await history.append(limits.rows.map { QuotaSample(agentId: $0.id, timestamp: now, remainingPct: $0.window.remainingPct) }, now: now)
            }
            catch { lastQuota = (now, .failure(UsageProviderError(error.localizedDescription))) }
        }
        switch lastQuota!.result {
        case .success(let limits): return (limits, lastQuota!.at, nil)
        case .failure(let error): return (nil, lastQuota!.at, error.message)
        }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), calendar = Calendar.current
        let weekAgo = now.addingTimeInterval(-7 * 86400)
        async let quotaResult = quota(now: now)
        let indexed = await transcripts.index(since: min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600)))
        let (limits, fetchedAt, failure) = await quotaResult
        let windows = limits?.rows ?? []
        let events = indexed.sessions.flatMap { $0.transcript.usage.map(\.event) }
        let models = Set(indexed.sessions.flatMap { $0.transcript.usage.map(\.model) }).sorted()
        let consumers = models.map { AgentDescriptor(id: "codex-model:\($0)", vendor: "Codex", model: $0,
                                                     source: L10n.sourceCodexAppServer, enabled: true) }
        let snapshots = windows.map { row in
            UsageSnapshot(agentId: row.id, remainingPct: row.window.remainingPct, weeklyRemainingPct: row.weekly?.remainingPct,
                          resetAt: row.window.resetAt, windowDuration: row.window.duration,
                          weeklyResetAt: row.weekly?.resetAt, updatedAt: fetchedAt)
        }
        var quotaHistory: [HistorySample] = []
        var byAgent: [String: UsageInsights] = [:]
        let sessionCount = indexed.sessions.filter { !$0.transcript.isSubagent }.count
        for (row, snapshot) in zip(windows, snapshots) {
            let historyStart = now.addingTimeInterval(-Double(max(historyHours, 168)) * 3600)
            let samples = await history.samples(agentId: row.id, since: min(historyStart, snapshot.cycle?.start ?? historyStart))
            // Do not invent historical readings before Agent HUD first observed the account.
            if let first = samples.first {
                let hours = min(historyHours, Int(now.timeIntervalSince(first.timestamp) / 3600) + 1)
                quotaHistory += UsageAnalytics.hourlyHistory(agentId: row.id, quota: samples, usage: [], hours: hours,
                                                             now: now, calendar: calendar, fallbackRemaining: nil)
            }
            let burn = UsageAnalytics.burnRate(samples: samples, cycle: snapshot.cycle, now: now)
            let caps = UsageAnalytics.capStats(samples: samples.filter { $0.timestamp >= weekAgo }, now: now)
            byAgent[row.id] = UsageInsights(burnRatePctPerHour: burn?.pctPerHour,
                                            timeToExhaust: burn?.timeToExhaust(remainingPct: row.window.remainingPct),
                                            weeklyCapHits: caps.hits, weeklyWaitTotal: caps.totalWait,
                                            weeklyWaitLongest: caps.longestWait, weeklyWaitLongestAt: caps.longestAt,
                                            weeklyShare: [:], windowSessionCount: sessionCount, windowUsedPct: row.window.usedPercent)
        }
        let sessions = indexed.sessions.filter { !$0.transcript.isSubagent }.sorted { a, b in
            let al = a.transcript.isLive(now: now, modifiedAt: a.modifiedAt), bl = b.transcript.isLive(now: now, modifiedAt: b.modifiedAt)
            if al != bl { return al }
            return (a.transcript.lastActivityAt ?? .distantPast) > (b.transcript.lastActivityAt ?? .distantPast)
        }.map { session in
            let t = session.transcript
            return LiveSession(id: t.id!, agentId: "codex-model:\(t.model)",
                               task: session.title ?? t.task ?? t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? L10n.vendorLabel("Codex"),
                               terminal: t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: t.startedAt ?? session.modifiedAt,
                               endedAt: t.isLive(now: now, modifiedAt: session.modifiedAt) ? nil : (t.lastActivityAt ?? session.modifiedAt),
                               pctOfWindow: nil, tokensIn: t.usage.reduce(0) { $0 + $1.input },
                               tokensOut: t.usage.reduce(0) { $0 + $1.output }, client: t.client, transcriptPath: session.path,
                               cacheReadTokens: t.usage.reduce(0) { $0 + $1.cachedInput })
        }
        let cutoff = min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600))
        let notice = failure ?? (windows.isEmpty ? L10n.text("当前账户暂无可用额度信息", "Usage limits are unavailable for this account") : nil)
        let consumerIds = Set(consumers.map(\.id) + sessions.map(\.agentId))
        let quotaIds = Set(["codex"] + windows.map(\.id) + agents.filter { $0.vendor == "Codex" }.map(\.id))
        let consumerIdsByQuota = Dictionary(uniqueKeysWithValues: quotaIds.map { ($0, consumerIds) })
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: sessions, history: quotaHistory,
                           activity: UsageAnalytics.activityGrid(usage: events, since: weekAgo, calendar: calendar),
                           insights: UsageInsights(burnRatePctPerHour: nil, timeToExhaust: nil, weeklyCapHits: 0,
                                                  weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                                                  weeklyShare: UsageAnalytics.weeklyShare(usage: events.filter { $0.timestamp >= weekAgo }),
                                                  windowSessionCount: sessionCount, windowUsedPct: 0),
                           notice: notice, discoveredAgents: windows.map(\.descriptor), consumers: consumers,
                           consumption: events.filter { $0.timestamp >= cutoff && $0.timestamp <= now }, indexing: indexed.indexing, insightsByAgent: byAgent,
                           subscriptions: limits?.plan.map { ["Codex": $0] } ?? [:], sourceNotices: notice.map { ["Codex": $0] } ?? [:],
                           consumerIdsByQuota: consumerIdsByQuota, codexResetCredits: limits?.rateLimitResetCredits,
                           codexResetCreditsObservedAt: limits?.rateLimitResetCredits == nil ? nil : fetchedAt,
                           completions: indexed.sessions.flatMap { $0.transcript.completions ?? [] },
                           turns: indexed.sessions.flatMap { $0.transcript.sessionTurns })
    }
}
