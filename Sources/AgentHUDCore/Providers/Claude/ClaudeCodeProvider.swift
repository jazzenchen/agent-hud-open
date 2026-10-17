import AgentHUDSupport
import Foundation

/// Real data for the Claude rows.
/// Quota: the Claude Code engine's SDK control protocol (`get_usage`). Sessions and tokens: local transcripts.
/// Burn rate and caps: persisted quota samples.
public struct ClaudeCodeProvider: UsageProvider, LedgerRecording {
    /// How long a quiet log keeps a session in flight: `SessionPhase.Limits.quiet`.
    public static let liveThreshold: TimeInterval = SessionPhase.Limits.quiet

    /// Engine queries spawn a process, so they run at most this often regardless of the poll interval.

    private let engine: ClaudeEngineUsageClient?
    private let engineCache = EngineUsageCache()
    private let transcripts: ClaudeTranscriptStore
    private let history: QuotaHistoryStore
    private let accountProfileURL: URL?
    private let sessionOriginsDirectory: URL?
    /// Where the compaction hook leaves its records (`CompactionHooks`); nil reads none.
    private let compactionsDirectory: URL?
    private let desktopSessions: ClaudeDesktopSessions?
    /// `ClientHome.key` of the configuration directory, separating unidentified logins of different homes.
    private let home: String
    private let clock: @Sendable () -> Date

    public init(
        engine: ClaudeEngineUsageClient?,
        transcripts: ClaudeTranscriptStore,
        history: QuotaHistoryStore,
        accountProfileURL: URL? = nil,
        home: String = "",
        sessionOriginsDirectory: URL? = nil,
        compactionsDirectory: URL? = nil,
        desktopSessions: ClaudeDesktopSessions? = nil,
        clock: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.engine = engine
        self.transcripts = transcripts
        self.history = history
        self.accountProfileURL = accountProfileURL
        self.home = home
        self.sessionOriginsDirectory = sessionOriginsDirectory
        self.compactionsDirectory = compactionsDirectory
        self.desktopSessions = desktopSessions
        self.clock = clock
    }

    /// Production wiring: the engine binary if present, plus the usage ledger. `persistent` false imports no earlier
    /// version's quota history.
    public static func standard(ledger: UsageLedger, persistent: Bool = true) -> ClaudeCodeProvider {
        try? ClaudeSessionOrigins.prepare()
        return ClaudeCodeProvider(
            engine: ClaudeEngineLocator.find().map {
                ClaudeEngineUsageClient(executable: $0, workingDirectory: ClaudeEngineUsageClient.defaultWorkingDirectory)
            },
            transcripts: ClaudeTranscriptStore(ledger: ledger, watchesChanges: true),
            history: QuotaHistoryStore(ledger: ledger, scope: "claude",
                                       importing: persistent ? AppSupport.directory.appendingPathComponent("quota-history.json") : nil),
            accountProfileURL: ClaudeSubscription.accountProfileURL,
            home: ClaudeSubscription.home,
            sessionOriginsDirectory: ClaudeSessionOrigins.directory,
            compactionsDirectory: CompactionHooks.directory,
            desktopSessions: ClaudeDesktopSessions()
        )
    }

    public var watchedDirectories: [URL]? {
        // Built in steps: as one sum of optionals, Xcode 16.4's type checker gives up on it.
        var directories: [URL] = transcripts.roots
        directories.append(AttentionHooks.directory)
        if let sessionOriginsDirectory { directories.append(sessionOriginsDirectory) }
        if let compactionsDirectory { directories.append(compactionsDirectory) }
        directories += desktopSessions?.roots ?? []
        return directories
    }
    public func fileChanges(_ paths: Set<String>?) async { await transcripts.fileChanges(paths) }

    private func account(for reading: EngineUsageCache.Reading) -> ProviderAccount {
        reading.identity?.account ?? .unresolved(provider: "Claude", home: home)
    }

    public func refreshAccountUsage(historyHours: Int) async {
        let now = clock()
        do {
            let profileURL = accountProfileURL
            let (result, fetchedAt) = try await engineCache.fetch(client: engine, now: now) {
                profileURL.flatMap { try? Data(contentsOf: $0) }
            }
            let account = account(for: result)
            await history.append(result.usage.usage.rows.map { $0.scoped(to: account) }.map {
                QuotaSample(agentId: $0.id, timestamp: fetchedAt, remainingPct: $0.window.remainingPct)
            }, now: now)
        } catch { /* The kept result carries the account error into the next local report. */ }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock()
        let weekAgo = now.addingTimeInterval(-AlertPolicy.insightsLookback)
        let cutoff = min(weekAgo, now.addingTimeInterval(-TimeInterval(historyHours) * 3600))

        // Local data: one cooperative indexing step (newest files first); the rest continues on later polls. Tokens and
        // models come from every log of `historyHours`; sessions only from those active in the session window.
        let indexed = await transcripts.index(modifiedSince: cutoff)
        let sessionStart = SessionWindow.start(at: now)
        let sessions = indexed.sessions.filter { $0.lastActivityAt >= sessionStart }
        let indexing = indexed.pending > 0
            ? IndexProgress(done: indexed.sessions.count, total: indexed.sessions.count + indexed.pending) : nil
        let observations: [(modelId: String, seenAt: Date)] = indexed.sessions.flatMap { session in
            session.modelsSeen.map { (modelId: $0.key, seenAt: $0.value) }
        }
        // All observed models are consumers; quota rows remain the plan's independent windows below.
        let consumers = ClaudeModelDiscovery.discover(observations).map(\.descriptor)

        // 1. Quota from the engine. A login without plan limits (API key, third-party platform) keeps the local data; it has
        // no current account, so its notice is shown and holds nothing back. Only a failed engine read is a failed read.
        var usage: ClaudeUsage?
        var notice: String?, failure: String?
        var updatedAt = now
        var reading: EngineUsageCache.Reading?
        do {
            if let (result, fetchedAt) = try await engineCache.reading() {
                reading = result
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
            // Quota availability does not determine whether a local turn completed. Any log read, listed or not, is local
            // data whose tokens and models the report keeps.
            guard !indexed.sessions.isEmpty else { throw error }
            notice = error.localizedDescription
            failure = notice
        }
        let plan = reading?.plan
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

        // 2. Session list: running first, then most recent. A session also runs while the sub-agents and workflow agents
        // it started are at work, after its own agent stopped or went quiet waiting for them. A request for approval is
        // compared with the session's own log, which is what answering it writes to.
        let windowStart = usage?.fiveHour?.resetsAt.map { $0.addingTimeInterval(-5 * 3600) } ?? now.addingTimeInterval(-5 * 3600)
        let agents = Self.workingAgents(sessions, now: now)
        let requests = AttentionHooks.read(source: AttentionHooks.Source.claude, now: now)
        let compactions = compactionsDirectory.map { CompactionHooks.read(source: .claude, now: now, directory: $0) } ?? [:]
        let candidates = sessions.filter { !$0.isSubagent }.map { session in
            let evidence = session.evidence(subagentsAt: agents[session.path], approval: requests[session.id]?.approval)
            return (session: session, evidence: evidence, reading: SessionPhase.read(evidence, rule: .transcript, at: now))
        }.sorted { lhs, rhs in
            if lhs.reading.inFlight != rhs.reading.inFlight { return lhs.reading.inFlight }
            return lhs.session.lastActivityAt > rhs.session.lastActivityAt
        }
        let windowTokens = await transcripts.tokens(since: windowStart)
        let windowTotal = windowTokens.values.reduce(0, +)
        let utilization = usage?.fiveHour?.utilizationPct ?? 0

        // 3. Insights from each window's stored readings; the session window gets them even without a reading.
        var insightsByAgent: [String: UsageInsights] = [:]
        for id in (account == nil ? [] : [sessionRowId]) + windowRows.map(\.id).filter({ $0 != sessionRowId }) {
            let snapshot = snapshots.first { $0.agentId == id }
            let samples = await history.samples(agentId: id, since: QuotaMath.historyStart(for: snapshot, now: now))
            insightsByAgent[id] = QuotaMath.insights(snapshot: snapshot, samples: samples, now: now)
        }
        let consumerIds = Set(consumers.map(\.id) + candidates.map { $0.session.dominantAgentId })
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
        // Origin hooks can arrive while ledger reads suspend this fetch. Use the latest source for both reports.
        let desktop = await desktopSessions?.index(sessionIDs: Set(candidates.map { $0.session.id })) ?? .init()
        let desktopTargets = desktop.targets
        // A session the desktop app archived keeps its usage and leaves the list.
        let shown = candidates.filter { !desktop.archived.contains($0.session.id) }
        let origins = sessionOriginsDirectory.map { ClaudeSessionOrigins.read(directory: $0) } ?? [:]
        func desktopTarget(for id: String, host: ClaudeSessionOrigins.Host) -> SessionNavigationTarget? {
            switch (host, desktopTargets[id]) {
            case (.desktop, .some(.claudeDesktopSession(id: _))), (.cowork, .some(.claudeCoworkSession(id: _))): return desktopTargets[id]
            default: return nil
            }
        }
        func navigationTarget(for session: TranscriptSession) -> SessionNavigationTarget? {
            if let origin = origins[session.id] {
                if session.lastActivityAt > origin.at, let entrypoint = session.entrypoint, !entrypoint.isEmpty {
                    let sameSurface: Bool
                    if entrypoint == "cli" { sameSurface = origin.host == .cli }
                    else if let desktopHost = ClaudeSessionOrigins.desktopHost(entrypoint) { sameSurface = origin.host == desktopHost }
                    else { sameSurface = (origin.host == .cli || origin.host == .unsupported) && origin.target == nil }
                    if !sameSurface {
                        // Hooks can be disabled while a session resumes elsewhere. A newer transcript can clear an old host,
                        // and Desktop has its own exact tab identity; a different CLI pane cannot be guessed from a transcript.
                        return ClaudeSessionOrigins.desktopHost(entrypoint).flatMap { desktopTarget(for: session.id, host: $0) }
                    }
                }
                switch origin.host {
                case .cli: return origin.target
                case .desktop, .cowork: return desktopTarget(for: session.id, host: origin.host)
                case .unsupported: return nil
                }
            }
            return ClaudeSessionOrigins.desktopHost(session.entrypoint).flatMap { desktopTarget(for: session.id, host: $0) }
        }
        let listed = shown.map { session, evidence, reading -> LiveSession in
            let live = reading.inFlight
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
                cacheReadTokens: session.cacheReadTokens, observedAt: evidence.newest, workingDirectory: session.cwd,
                lastActivityAt: session.lastActivityAt,
                navigationTarget: navigationTarget(for: session),
                activity: Self.activity(of: session, subagentsAt: agents[session.path], compaction: compactions[session.id])
            )
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
            quotaNotices: failure.map { ["Claude": $0] } ?? [:],
            readingIssues: failure.map { ["Claude": .readFailed($0)] } ?? [:],
            consumerIdsByQuota: consumerIdsByQuota,
            completions: sessions.flatMap { session in
                session.completions.map { completion in
                    var completion = completion
                    completion.navigationTarget = navigationTarget(for: session)
                    return completion
                }
            },
            turns: shown.compactMap(\.reading.turn),
            // A confirmed login without plan limits has no current subscription account and retires earlier quota rows.
            accounts: reading.map { reading in
                ["Claude": usage == nil ? [] : [AccountObservation(account: self.account(for: reading), home: home,
                    label: reading.identity?.email, plan: plan, observedAt: updatedAt)]]
            }
        )
    }
}

/// Serialises engine queries, each of which spawns a full engine process, and keeps the last reading for the reports
/// between them. How often the engine is asked is the collector's to decide (`UsageRefresh.accountRequestSpacing`).
actor EngineUsageCache {
    /// One engine reading with what the account profile read around it says. The profile can run to megabytes, so
    /// it is decoded once for the reading rather than on every pass that shows it.
    struct Reading: Sendable {
        let usage: ClaudeEngineUsage
        let identity: ClaudeSubscription.Identity?
        let plan: String?
        /// Asked only when there are no plan limits, to say why there are none.
        var signedIn: Bool?
    }

    private var last: (at: Date, result: Result<Reading, any Error>)?

    func reading() throws -> (Reading, Date)? {
        guard let last else { return nil }
        return (try last.result.get(), last.at)
    }

    /// The profile is read before and after the query; a login change in between discards the reading.
    func fetch(client: ClaudeEngineUsageClient?, now: Date,
               profile: @Sendable () -> Data? = { nil }) async throws -> (Reading, Date) {
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
            result = .success(Reading(usage: usage, identity: usage.rateLimitsAvailable ? identity : nil,
                                      plan: ClaudeSubscription.plan(type: usage.subscriptionType, profileData: after),
                                      signedIn: signedIn))
        }
        catch {
            try Task.checkCancellation()
            result = .failure(error)
        }
        // A failed attempt is kept too, so the reports until the next query carry its error.
        last = (now, result)
        return (try result.get(), now)
    }
}

extension ClaudeCodeProvider {
    /// When the sub-agents still at work last did something, keyed by the log of the session that started them. Claude
    /// Code keeps a session's sub-agent and workflow agent logs in a directory named after its own log.
    static func workingAgents(_ sessions: [TranscriptSession], now: Date) -> [String: Date] {
        var latest: [String: Date] = [:]
        for agent in sessions where agent.isSubagent && SessionPhase.read(agent.evidence(), rule: .transcript, at: now).inFlight {
            guard let directory = agent.path.range(of: "/subagents/") else { continue }
            let parent = String(agent.path[..<directory.lowerBound]) + ".jsonl"
            latest[parent] = max(latest[parent] ?? agent.lastActivityAt, agent.lastActivityAt)
        }
        return latest
    }

    /// What a main log says its turn is busy with, with the newest line of its sub-agents at work and the compaction its
    /// client said it began after the log's last line.
    static func activity(of session: TranscriptSession, subagentsAt: Date?, compaction: CompactionHooks.Event?) -> SessionActivity {
        var activity = session.activity ?? SessionActivity()
        activity.subagentsAt = subagentsAt
        activity.compactingSince = CompactionHooks.since(compaction, lastLineAt: session.lastActivityAt)
        return activity
    }

    /// Each session's turn as the provider reports it (`SessionPhase.read(_:rule:at:)`).
    static func turns(_ sessions: [TranscriptSession], agentsWorkingAt: [String: Date],
                      requests: [String: AttentionHooks.Event]) -> [SessionTurn] {
        sessions.compactMap { session in
            session.turn.map {
                SessionPhase.transcriptTurn($0, approval: requests[$0.sessionID]?.approval, subagentsAt: agentsWorkingAt[session.path])
            }
        }
    }

    /// The session's turn while its agents work (`SessionPhase.read(_:rule:at:)`).
    static func turn(_ turn: SessionTurn, agentsWorkingAt: Date?) -> SessionTurn {
        SessionPhase.transcriptTurn(turn, approval: nil, subagentsAt: agentsWorkingAt)
    }

    /// The turns Claude Code said it is blocked on (`SessionPhase.read(_:rule:at:)`). The hook only says it needs the
    /// user; a turn that is still running is waiting for approval, and one that already finished is simply waiting for
    /// the next prompt. A request older than the transcript has been answered.
    static func awaiting(_ turns: [SessionTurn], requests: [String: AttentionHooks.Event]) -> [SessionTurn] {
        turns.map { SessionPhase.transcriptTurn($0, approval: requests[$0.sessionID]?.approval, subagentsAt: nil) }
    }
}
