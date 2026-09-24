import AgentHUDSupport
import Foundation

/// Real data for the Claude rows.
/// Quota: the Claude Code engine's SDK control protocol (`get_usage`). Sessions and tokens: local transcripts.
/// Burn rate and caps: persisted quota samples.
public struct ClaudeCodeProvider: UsageProvider, LedgerRecording {
    public static let liveThreshold: TimeInterval = 120

    /// Engine queries spawn a process, so they run at most this often regardless of the poll interval.
    public static let engineMinimumInterval = UsageRefresh.accountRequestSpacing

    private let engine: ClaudeEngineUsageClient?
    private let engineCache = EngineUsageCache()
    private let transcripts: ClaudeTranscriptStore
    private let history: QuotaHistoryStore
    private let accountProfileURL: URL?
    /// `ClientHome.key` of the configuration directory, separating unidentified logins of different homes.
    private let home: String
    private let clock: @Sendable () -> Date

    public init(
        engine: ClaudeEngineUsageClient?,
        transcripts: ClaudeTranscriptStore,
        history: QuotaHistoryStore,
        accountProfileURL: URL? = nil,
        home: String = "",
        clock: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.engine = engine
        self.transcripts = transcripts
        self.history = history
        self.accountProfileURL = accountProfileURL
        self.home = home
        self.clock = clock
    }

    /// Production wiring: the engine binary if present, plus the usage ledger.
    public static func standard(ledger: UsageLedger) -> ClaudeCodeProvider {
        ClaudeCodeProvider(
            engine: ClaudeEngineLocator.find().map {
                ClaudeEngineUsageClient(executable: $0, workingDirectory: ClaudeEngineUsageClient.defaultWorkingDirectory)
            },
            transcripts: ClaudeTranscriptStore(ledger: ledger, watchesChanges: true),
            history: QuotaHistoryStore(ledger: ledger, scope: "claude", importing: AppSupport.directory.appendingPathComponent("quota-history.json")),
            accountProfileURL: ClaudeSubscription.accountProfileURL,
            home: ClaudeSubscription.home
        )
    }

    public var watchedDirectories: [URL]? { transcripts.roots + [AttentionHooks.directory] }

    private func account(for reading: EngineUsageCache.Reading) -> ProviderAccount {
        reading.identity?.account ?? .unresolved(provider: "Claude", home: home)
    }

    public func refreshAccountUsage(historyHours: Int) async {
        let now = clock()
        do {
            let profileURL = accountProfileURL
            let (result, fetchedAt, fresh) = try await engineCache.fetch(client: engine, now: now, minimumInterval: Self.engineMinimumInterval) {
                profileURL.flatMap { try? Data(contentsOf: $0) }
            }
            if fresh {
                let account = account(for: result)
                await history.append(result.usage.usage.rows.map { $0.scoped(to: account) }.map {
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
        var reading: EngineUsageCache.Reading?
        do {
            if let (result, fetchedAt) = try await engineCache.reading() {
                reading = result
                subscription = result.usage.subscriptionType
                updatedAt = fetchedAt
                if result.usage.rateLimitsAvailable {
                    // Keep the engine's observation intact. A deadline passing is not a confirmed reset.
                    usage = result.usage.usage
                } else {
                    notice = (result.signedIn == false ? ClaudeDataError.signedOut : .planLimitsUnavailable).errorDescription
                }
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            // Quota availability does not determine whether a local turn completed.
            guard !sessions.isEmpty else { throw error }
            notice = error.localizedDescription
        }
        let plan = ClaudeSubscription.plan(type: subscription, profileData: reading?.profileData)
        let account = reading.map(account(for:))
        let sessionRowId = account?.windowID(ClaudeUsage.sessionRowId) ?? ClaudeUsage.sessionRowId

        // Quota rows: one per window (session / weekly / weekly per family), each with its own reset cadence.
        let windowRows = account.map { account in (usage?.rows ?? []).map { $0.scoped(to: account) } } ?? []
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

        // 2. Session list: running first, then most recent.
        let windowStart = usage?.fiveHour?.resetsAt.map { $0.addingTimeInterval(-5 * 3600) } ?? now.addingTimeInterval(-5 * 3600)
        let candidates = sessions.filter { !$0.isSubagent }.sorted { lhs, rhs in
            let lhsLive = lhs.isLive(now: now, threshold: Self.liveThreshold)
            let rhsLive = rhs.isLive(now: now, threshold: Self.liveThreshold)
            if lhsLive != rhsLive { return lhsLive }
            return lhs.lastActivityAt > rhs.lastActivityAt
        }
        let windowTokens = await transcripts.tokens(since: windowStart)
        let windowTotal = windowTokens.values.reduce(0, +)
        let utilization = usage?.fiveHour?.utilizationPct ?? 0
        let listed = candidates.map { session -> LiveSession in
            let live = session.isLive(now: now, threshold: Self.liveThreshold)
            let share = windowTotal > 0 ? Double(windowTokens[session.path] ?? 0) / Double(windowTotal) : 0
            let agentId = session.dominantAgentId
            return LiveSession(
                id: session.id,
                agentId: agentId,
                task: session.task ?? L10n.text("（未命名会话）", "(untitled session)"),
                terminal: session.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                startedAt: session.startedAt,
                endedAt: live ? nil : session.lastActivityAt,
                // A session that spent nothing in the current window has no share of it, rather than a share of zero.
                pctOfWindow: share > 0 ? share * utilization : nil,
                tokensIn: session.tokensIn,
                tokensOut: session.tokensOut,
                client: ClaudeEntrypoint.clientLabel(session.entrypoint),
                transcriptPath: session.path,
                cacheReadTokens: session.cacheReadTokens, observedAt: now
            )
        }

        // 3. Insights from the session window's samples.
        let weekSamples = await history.samples(agentId: sessionRowId, since: weekAgo)
        let sessionCycle = snapshots.first { $0.agentId == sessionRowId }?.cycle
        let burn = UsageAnalytics.burnRate(samples: weekSamples, cycle: sessionCycle, now: now)
        let cap = UsageAnalytics.capStats(samples: weekSamples, now: now)
        let insights = UsageInsights(
            burnRatePctPerHour: burn?.pctPerHour,
            timeToExhaust: burn.flatMap { $0.timeToExhaust(remainingPct: usage?.fiveHour?.remainingPct ?? 0) },
            weeklyCapHits: cap.hits,
            weeklyWaitTotal: cap.totalWait,
            weeklyWaitLongest: cap.longestWait,
            weeklyWaitLongestAt: cap.longestAt
        )

        var insightsByAgent: [String: UsageInsights] = account == nil ? [:] : [sessionRowId: insights]
        for row in windowRows where row.id != sessionRowId {
            let samples = await history.samples(agentId: row.id, since: weekAgo)
            let cycle = snapshots.first { $0.agentId == row.id }?.cycle
            let rowBurn = UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: now)
            let rowCap = UsageAnalytics.capStats(samples: samples, now: now)
            insightsByAgent[row.id] = UsageInsights(
                burnRatePctPerHour: rowBurn?.pctPerHour,
                timeToExhaust: rowBurn?.timeToExhaust(remainingPct: row.window.remainingPct),
                weeklyCapHits: rowCap.hits, weeklyWaitTotal: rowCap.totalWait,
                weeklyWaitLongest: rowCap.longestWait, weeklyWaitLongestAt: rowCap.longestAt
            )
        }
        let consumerIds = Set(consumers.map(\.id) + listed.map(\.agentId))
        var consumerIdsByQuota: [String: Set<String>] = [:]
        if let account {
            consumerIdsByQuota[sessionRowId] = consumerIds
            consumerIdsByQuota[account.windowID(ClaudeUsage.weeklyRowId)] = consumerIds
            for id in consumerIds {
                if let info = ClaudeModelInfo.parse(String(id.dropFirst("claude-model:".count))) {
                    consumerIdsByQuota[account.windowID("\(ClaudeUsage.weeklyRowId)-\(info.family.lowercased())"), default: []].insert(id)
                }
            }
        }
        return UsageReport(
            generatedAt: now,
            snapshots: snapshots,
            sessions: listed,
            notice: notice,
            discoveredAgents: discovered,
            consumers: consumers,
            indexing: indexing,
            insightsByAgent: insightsByAgent,
            subscriptions: plan.map { ["Claude": $0] } ?? [:],
            sourceNotices: notice.map { ["Claude": $0] } ?? [:],
            consumerIdsByQuota: consumerIdsByQuota,
            completions: sessions.flatMap(\.completions),
            turns: Self.awaiting(sessions.compactMap(\.turn), now: now),
            // A login without plan limits has no current subscription account; earlier accounts keep their last readings.
            accounts: reading.map { reading in
                ["Claude": usage == nil ? [] : [AccountObservation(account: self.account(for: reading), home: home,
                    label: reading.identity?.email, plan: plan, observedAt: updatedAt)]]
            }
        )
    }
}

/// Serialises engine queries and throttles them, since each one spawns a full engine process.
actor EngineUsageCache {
    /// One engine reading with the account profile read around it.
    struct Reading: Sendable {
        let usage: ClaudeEngineUsage
        let identity: ClaudeSubscription.Identity?
        let profileData: Data?
        /// Asked only when there are no plan limits, to say why there are none.
        var signedIn: Bool?
    }

    private var last: (at: Date, result: Result<Reading, any Error>)?

    func reading() throws -> (Reading, Date)? {
        guard let last else { return nil }
        return (try last.result.get(), last.at)
    }

    /// `fresh` is false when the reading comes from the cache rather than a new engine query.
    /// The profile is read before and after the query; a login change in between discards the reading.
    func fetch(client: ClaudeEngineUsageClient?, now: Date, minimumInterval: TimeInterval,
               profile: @Sendable () -> Data? = { nil }) async throws -> (Reading, Date, fresh: Bool) {
        if let last, now.timeIntervalSince(last.at) < minimumInterval {
            return (try last.result.get(), last.at, false)
        }
        let result: Result<Reading, any Error>
        do {
            guard let client else { throw ClaudeDataError.engineNotFound }
            let before = profile()
            let usage = try await client.fetch()
            let after = profile()
            let identity = ClaudeSubscription.identity(profileData: after)
            // An API-key or third-party login can leave an old profile behind; only plan limits make it the reading's account.
            guard ClaudeSubscription.identity(profileData: before) == identity else { throw ClaudeDataError.accountChanged }
            let signedIn = usage.rateLimitsAvailable ? nil : await client.isSignedIn()
            result = .success(Reading(usage: usage, identity: usage.rateLimitsAvailable ? identity : nil, profileData: after,
                                      signedIn: signedIn))
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

extension ClaudeCodeProvider {
    /// A turn Claude Code said it is blocked on. The hook only says it needs the user; a turn that is still running is
    /// waiting for approval, and one that already finished is simply waiting for the next prompt. A request older than
    /// the transcript has been answered.
    static func awaiting(_ turns: [SessionTurn], now: Date) -> [SessionTurn] {
        awaiting(turns, requests: AttentionHooks.read(source: AttentionHooks.Source.claude, now: now))
    }

    static func awaiting(_ turns: [SessionTurn], requests: [String: AttentionHooks.Event]) -> [SessionTurn] {
        guard !requests.isEmpty else { return turns }
        return turns.map { turn in
            guard turn.state == .running, let request = requests[turn.sessionID],
                  RecordCoding.milliseconds(request.at) > turn.observedAtMs else { return turn }
            return SessionTurn(provider: turn.provider, sessionID: turn.sessionID, turnID: turn.turnID,
                               state: .waitingForApproval, startedAtMs: turn.startedAtMs,
                               observedAtMs: RecordCoding.milliseconds(request.at), message: request.message ?? turn.message)
        }
    }
}
