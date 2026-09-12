import Foundation

/// Real data for the Claude rows.
/// Quota: the Claude Code engine's SDK control protocol (`get_usage`). Sessions/tokens/heatmap: local transcripts.
/// Trends: persisted quota samples.
public struct ClaudeCodeProvider: UsageProvider {
    public static let liveThreshold: TimeInterval = 120

    /// Engine queries spawn a process, so they run at most this often regardless of the poll interval.
    public static let engineMinimumInterval: TimeInterval = 120

    private let engine: ClaudeEngineUsageClient?
    private let engineCache = EngineUsageCache()
    private let transcripts: ClaudeTranscriptStore
    private let history: QuotaHistoryStore
    private let accountProfileURL: URL?
    private let calendar: Calendar
    private let clock: @Sendable () -> Date

    public init(
        engine: ClaudeEngineUsageClient?,
        transcripts: ClaudeTranscriptStore,
        history: QuotaHistoryStore,
        accountProfileURL: URL? = nil,
        calendar: Calendar = .current,
        clock: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.engine = engine
        self.transcripts = transcripts
        self.history = history
        self.accountProfileURL = accountProfileURL
        self.calendar = calendar
        self.clock = clock
    }

    /// Production wiring: the engine binary if present, plus local caches.
    public static func standard() -> ClaudeCodeProvider {
        ClaudeCodeProvider(
            engine: ClaudeEngineLocator.find().map {
                ClaudeEngineUsageClient(executable: $0, workingDirectory: ClaudeEngineUsageClient.defaultWorkingDirectory)
            },
            transcripts: ClaudeTranscriptStore(cacheURL: ClaudeTranscriptStore.defaultCacheURL),
            history: QuotaHistoryStore(fileURL: QuotaHistoryStore.defaultFileURL),
            accountProfileURL: ClaudeSubscription.accountProfileURL
        )
    }

    public func refreshAccountUsage(historyHours: Int) async {
        let now = clock()
        do {
            let (result, fetchedAt, fresh) = try await engineCache.fetch(client: engine, now: now, minimumInterval: Self.engineMinimumInterval)
            if fresh {
                await history.append(result.usage.rows.map {
                    QuotaSample(agentId: $0.id, timestamp: fetchedAt, remainingPct: $0.window.remainingPct)
                }, now: now)
            }
        } catch { /* The cached result carries the account error into the next local report. */ }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock()
        let weekAgo = now.addingTimeInterval(-7 * 86400)
        let cutoff = min(weekAgo, now.addingTimeInterval(-TimeInterval(historyHours) * 3600))

        // Local data: one cooperative indexing step (newest files first); the rest continues on later polls.
        let indexed = await transcripts.index(modifiedSince: cutoff)
        let sessions = indexed.sessions
        let indexing = indexed.pending > 0 ? IndexProgress(done: sessions.count, total: sessions.count + indexed.pending) : nil
        let usageEvents = sessions.flatMap(\.usage)
        let observations: [(modelId: String, seenAt: Date)] = sessions.flatMap { session in
            session.modelsSeen.map { (modelId: $0.key, seenAt: $0.value) }
        }
        // All observed models are consumers; quota rows remain the plan's independent windows below.
        let consumers = ClaudeModelDiscovery.discover(observations).map(\.descriptor)

        // 1. Quota from the engine. A login without plan limits (API key, third-party platform) keeps the local data.
        var subscription: String?
        var usage: ClaudeUsage?
        var notice: String?
        var updatedAt = now
        do {
            if let (result, fetchedAt) = try await engineCache.reading() {
                subscription = result.subscriptionType
                updatedAt = fetchedAt
                if result.rateLimitsAvailable {
                    // Keep the engine's observation intact. A deadline passing is not a confirmed reset.
                    usage = result.usage
                } else {
                    notice = ClaudeDataError.planLimitsUnavailable.errorDescription
                }
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            // Quota availability does not determine whether a local turn completed.
            guard !sessions.isEmpty else { throw error }
            notice = error.localizedDescription
        }
        let plan = ClaudeSubscription.plan(type: subscription, profileData: accountProfileURL.flatMap { try? Data(contentsOf: $0) })

        // Quota rows: one per window (session / weekly / weekly per family), each with its own reset cadence.
        let windowRows = usage?.rows ?? []
        let discovered = windowRows.map(\.descriptor)
        var snapshots: [UsageSnapshot] = []
        for row in windowRows {
            snapshots.append(UsageSnapshot(
                agentId: row.id,
                remainingPct: row.window.remainingPct,
                weeklyRemainingPct: usage?.sevenDay?.remainingPct,
                resetAt: row.window.resetsAt,
                windowDuration: row.duration,
                weeklyResetAt: usage?.sevenDay?.resetsAt,
                updatedAt: updatedAt
            ))
        }

        // 3. Hourly history: remaining % per window row, tokens per consumer.
        let quotaSince = now.addingTimeInterval(-TimeInterval(historyHours + 1) * 3600)
        var historySamples: [HistorySample] = []
        for row in windowRows {
            let quota = await history.samples(agentId: row.id, since: quotaSince)
            historySamples += UsageAnalytics.hourlyHistory(
                agentId: row.id, quota: quota, usage: [], hours: historyHours, now: now, calendar: calendar,
                fallbackRemaining: row.window.remainingPct
            )
        }
        // Rows the user still has enabled but the plan no longer reports keep their stored history.
        for agent in agents where agent.enabled && agent.id.hasPrefix("claude-") && !windowRows.contains(where: { $0.id == agent.id }) {
            let quota = await history.samples(agentId: agent.id, since: quotaSince)
            guard !quota.isEmpty else { continue }
            historySamples += UsageAnalytics.hourlyHistory(
                agentId: agent.id, quota: quota, usage: [], hours: historyHours, now: now, calendar: calendar, fallbackRemaining: nil
            )
        }
        // 4. Session list: running first, then most recent.
        let windowStart = usage?.fiveHour?.resetsAt.map { $0.addingTimeInterval(-5 * 3600) } ?? now.addingTimeInterval(-5 * 3600)
        let candidates = sessions.filter { !$0.isSubagent }.sorted { lhs, rhs in
            let lhsLive = lhs.isLive(now: now, threshold: Self.liveThreshold)
            let rhsLive = rhs.isLive(now: now, threshold: Self.liveThreshold)
            if lhsLive != rhsLive { return lhsLive }
            return lhs.lastActivityAt > rhs.lastActivityAt
        }
        let windowTotal = sessions.reduce(0) { $0 + $1.tokens(since: windowStart) }
        let utilization = usage?.fiveHour?.utilizationPct ?? 0
        let listed = candidates.map { session -> LiveSession in
            let live = session.isLive(now: now, threshold: Self.liveThreshold)
            let share = windowTotal > 0 ? Double(session.tokens(since: windowStart)) / Double(windowTotal) : 0
            let agentId = session.dominantAgentId
            return LiveSession(
                id: session.id,
                agentId: agentId,
                task: session.task ?? L10n.text("（未命名会话）", "(untitled session)"),
                terminal: session.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                startedAt: session.startedAt,
                endedAt: live ? nil : session.lastActivityAt,
                pctOfWindow: share * utilization,
                tokensIn: session.tokensIn,
                tokensOut: session.tokensOut,
                client: ClaudeEntrypoint.clientLabel(session.entrypoint),
                transcriptPath: session.path,
                cacheReadTokens: session.cacheReadTokens, observedAt: now
            )
        }

        // 5. Insights from the session window's samples.
        let weekSamples = await history.samples(agentId: ClaudeUsage.sessionRowId, since: weekAgo)
        let sessionCycle = snapshots.first { $0.agentId == ClaudeUsage.sessionRowId }?.cycle
        let burn = UsageAnalytics.burnRate(samples: weekSamples, cycle: sessionCycle, now: now)
        let cap = UsageAnalytics.capStats(samples: weekSamples, now: now)
        let insights = UsageInsights(
            burnRatePctPerHour: burn?.pctPerHour,
            timeToExhaust: burn.flatMap { $0.timeToExhaust(remainingPct: usage?.fiveHour?.remainingPct ?? 0) },
            weeklyCapHits: cap.hits,
            weeklyWaitTotal: cap.totalWait,
            weeklyWaitLongest: cap.longestWait,
            weeklyWaitLongestAt: cap.longestAt,
            weeklyShare: UsageAnalytics.weeklyShare(usage: usageEvents.filter { $0.timestamp >= weekAgo }),
            windowSessionCount: sessions.filter { !$0.isSubagent && $0.lastActivityAt >= windowStart }.count,
            windowUsedPct: utilization
        )

        var insightsByAgent = [ClaudeUsage.sessionRowId: insights]
        for row in windowRows where row.id != ClaudeUsage.sessionRowId {
            let samples = await history.samples(agentId: row.id, since: weekAgo)
            let cycle = snapshots.first { $0.agentId == row.id }?.cycle
            let rowBurn = UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: now)
            let rowCap = UsageAnalytics.capStats(samples: samples, now: now)
            insightsByAgent[row.id] = UsageInsights(
                burnRatePctPerHour: rowBurn?.pctPerHour,
                timeToExhaust: rowBurn?.timeToExhaust(remainingPct: row.window.remainingPct),
                weeklyCapHits: rowCap.hits, weeklyWaitTotal: rowCap.totalWait,
                weeklyWaitLongest: rowCap.longestWait, weeklyWaitLongestAt: rowCap.longestAt,
                weeklyShare: insights.weeklyShare, windowSessionCount: insights.windowSessionCount,
                windowUsedPct: row.window.utilizationPct
            )
        }
        let consumerIds = Set(consumers.map(\.id) + listed.map(\.agentId))
        var consumerIdsByQuota = [ClaudeUsage.sessionRowId: consumerIds, ClaudeUsage.weeklyRowId: consumerIds]
        for id in consumerIds {
            if let info = ClaudeModelInfo.parse(String(id.dropFirst("claude-model:".count))) {
                consumerIdsByQuota["\(ClaudeUsage.weeklyRowId)-\(info.family.lowercased())", default: []].insert(id)
            }
        }
        return UsageReport(
            generatedAt: now,
            snapshots: snapshots,
            sessions: listed,
            history: historySamples,
            activity: UsageAnalytics.activityGrid(usage: usageEvents, since: weekAgo, calendar: calendar),
            insights: insights,
            notice: notice,
            discoveredAgents: discovered,
            subscriptionType: subscription,
            consumers: consumers,
            consumption: usageEvents.filter { $0.timestamp >= cutoff && $0.timestamp <= now },
            indexing: indexing,
            insightsByAgent: insightsByAgent,
            subscriptions: plan.map { ["Claude": $0] } ?? [:],
            sourceNotices: notice.map { ["Claude": $0] } ?? [:],
            consumerIdsByQuota: consumerIdsByQuota,
            completions: sessions.flatMap(\.completions),
            claudeConsumptionSince: indexing == nil ? cutoff : nil,
            turns: sessions.compactMap(\.turn)
        )
    }
}

/// Serialises engine queries and throttles them, since each one spawns a full engine process.
actor EngineUsageCache {
    private var last: (at: Date, result: Result<ClaudeEngineUsage, any Error>)?

    func reading() throws -> (ClaudeEngineUsage, Date)? {
        guard let last else { return nil }
        return (try last.result.get(), last.at)
    }

    /// `fresh` is false when the reading comes from the cache rather than a new engine query.
    func fetch(client: ClaudeEngineUsageClient?, now: Date, minimumInterval: TimeInterval) async throws -> (ClaudeEngineUsage, Date, fresh: Bool) {
        if let last, now.timeIntervalSince(last.at) < minimumInterval {
            return (try last.result.get(), last.at, false)
        }
        let result: Result<ClaudeEngineUsage, any Error>
        do {
            guard let client else { throw ClaudeDataError.engineNotFound }
            result = .success(try await client.fetch())
        }
        catch {
            try Task.checkCancellation()
            result = .failure(error)
        }
        // Failed attempts use the same interval, so completion polling cannot repeatedly spawn a broken engine.
        last = (now, result)
        return (try result.get(), now, true)
    }
}
