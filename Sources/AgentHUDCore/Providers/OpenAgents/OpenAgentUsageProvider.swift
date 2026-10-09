import AgentHUDSupport
import Foundation

/// Clients supply request observations. Billing services supply account observations, exactly once per pool.
actor OpenAgentUsageProvider: UsageProvider, LedgerRecording {
    private let credentials: @Sendable () -> [OpenAgentCredential]
    private let sessions: @Sendable (Date) async -> OpenAgentLocalStore.Result
    private let fetchQuota: @Sendable (OpenAgentCredential, Date) async throws -> ProviderQuota
    private let identify: @Sendable (OpenAgentCredential) async throws -> OpenAgentCredential
    private let apiServices: @Sendable () -> [AgentService]
    private struct Identity: Sendable {
        let at: Date
        let pool: BillingPool
        var notice: String? = nil
    }
    private var identities: [String: Identity] = [:]
    private let identityCacheURL: URL?
    private let history: QuotaHistoryStore
    private let clock: @Sendable () -> Date
    private struct QuotaResult: Sendable {
        let credential: OpenAgentCredential
        let quota: ProviderQuota?
        let notice: String?
        let at: Date
        var isActive = true
        /// The last reading that succeeded, and its plan, kept through failures that are not a sign-out.
        var readAt: Date? = nil
        var plan: String? = nil
    }
    /// Nil until the first account scan completes; an empty result is an observed empty inventory.
    private var cached: [String: QuotaResult]?
    nonisolated let watchedDirectories: [URL]?
    private let noteChanges: @Sendable (Set<String>?) async -> Void
    private let ledger: UsageLedger
    private let sessionLedger: SessionLedger
    static let source = "open-agents"
    init(credentials: @escaping @Sendable () -> [OpenAgentCredential],
         sessions: @escaping @Sendable (Date) async -> OpenAgentLocalStore.Result,
         fetchQuota: @escaping @Sendable (OpenAgentCredential, Date) async throws -> ProviderQuota,
         history: QuotaHistoryStore, identify: @escaping @Sendable (OpenAgentCredential) async throws -> OpenAgentCredential = { $0 }, clock: @escaping @Sendable () -> Date = { Date() },
         identityCacheURL: URL? = nil,
         apiServices: @escaping @Sendable () -> [AgentService] = { [] },
         watchedDirectories: [URL]? = nil, fileChanges: @escaping @Sendable (Set<String>?) async -> Void = { _ in },
         ledger: UsageLedger = .inMemory()) {
        self.credentials = credentials; self.sessions = sessions; self.fetchQuota = fetchQuota
        noteChanges = fileChanges
        self.history = history; self.identify = identify; self.clock = clock
        self.apiServices = apiServices
        self.identityCacheURL = identityCacheURL
        self.watchedDirectories = watchedDirectories
        self.ledger = ledger
        sessionLedger = SessionLedger(source: Self.source, ledger: ledger)
        if let data = identityCacheURL.flatMap({ try? Data(contentsOf: $0) }),
           let saved = try? JSONDecoder().decode([String: BillingPool].self, from: data) {
            identities = saved.filter { $0.value.evidence == .account }.mapValues { Identity(at: .distantPast, pool: $0) }
        }
    }
    static func standard(ledger: UsageLedger, persistHistory: Bool = true) -> OpenAgentUsageProvider {
        let paths = OpenAgentPaths(home: FileManager.default.homeDirectoryForCurrentUser, environment: ProcessInfo.processInfo.environment)
        let local = OpenAgentLocalStore(paths: paths)
        return .init(credentials: { OpenAgentCredentials.discover() }, sessions: { await local.index(since: $0) },
            fetchQuota: { try await OpenAgentQuotaClient().fetch($0, now: $1) },
            history: QuotaHistoryStore(ledger: ledger, scope: Self.source,
                importing: persistHistory ? AppSupport.directory.appendingPathComponent("open-agent-quota-history.json") : nil),
            identify: { try await OpenAgentQuotaClient().identify($0) },
            identityCacheURL: persistHistory ? AppSupport.directory.appendingPathComponent("open-agent-identities.json") : nil,
            apiServices: { AgentAPIServiceDiscovery.discover() },
            watchedDirectories: [paths.openCode, paths.piTurns] + paths.roots(for: .kimi) + paths.roots(for: .pi),
            fileChanges: { await local.fileChanges($0) }, ledger: ledger)
    }

    func fileChanges(_ paths: Set<String>?) async { await noteChanges(paths) }

    /// Token totals of every open agent client from the period holding `since`.
    func usage(since: Date) async -> [UsageBucket] {
        (try? await ledger.buckets(since: since, source: Self.source)) ?? []
    }

    /// Writes each session's usage once the index is complete and something changed.
    private func record(_ local: OpenAgentLocalStore.Result, since: Date) async {
        guard local.indexing == nil else { return }
        // Usage recorded under a hash of a route's provider and model, or of its provider alone, joins the route's id.
        var moves: [String: String] = [:], seen = Set<String>()
        for session in local.sessions {
            for event in session.events where seen.insert(event.agentId).inserted {
                guard let provider = event.attribution?.providerID, let model = session.models[event.agentId] else { continue }
                let prefix = "\(session.client.rawValue)-model:"
                moves[prefix + RecordCoding.hash([provider, model])] = event.agentId
                moves[prefix + "\(model)#" + RecordCoding.hash([provider])] = event.agentId
            }
        }
        try? await ledger.moveConsumers(moves)
        await sessionLedger.record(files: local.files, revision: local.revision, window: SessionContributions.windowStart(since)) {
            local.sessions.map { ($0.id, $0.events) }
        }
    }
    func refreshAccountUsage(historyHours: Int) async {
        let now = clock()
        let raw = OpenAgentCredentials.merge(credentials().filter { $0.isUsable(at: now) })
        let activeCredentials = Set(raw.map { $0.pool.id })
        let savedIdentities = identities.filter { $0.value.pool.evidence == .account }.mapValues(\.pool)
        identities = identities.filter { activeCredentials.contains($0.key) }
        let prior = identities, identify = identify
        var resolved: [(String, OpenAgentCredential, Identity)] = []
        for credential in raw {
            if let known = prior[credential.pool.id], known.pool.evidence == .account {
                resolved.append((credential.pool.id, .init(service: credential.service, token: credential.token,
                    pool: known.pool, headers: credential.headers, clients: credential.clients, expiresAt: credential.expiresAt), known))
                continue
            }
            do {
                let identified = try await identify(credential)
                resolved.append((credential.pool.id, identified, Identity(at: now, pool: identified.pool)))
            } catch {
                let notice = L10n.text("账户归属未确认", "Account identity unconfirmed") + ": " + error.localizedDescription
                resolved.append((credential.pool.id, credential, Identity(at: now, pool: credential.pool, notice: notice)))
            }
        }
        for (key, _, identity) in resolved { identities[key] = identity }
        let confirmed = identities.filter { $0.value.pool.evidence == .account }.mapValues(\.pool)
        if confirmed != savedIdentities, let identityCacheURL {
            do {
                try FileManager.default.createDirectory(at: identityCacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
                try JSONEncoder().encode(confirmed).write(to: identityCacheURL, options: .atomic)
            } catch { NSLog("[AgentHUD] Account identity cache write failed: %@", error.localizedDescription) }
        }
        let resolvedCredentials = resolved.sorted { $0.0 < $1.0 }.map { $0.1 }
        let aliases = Dictionary(grouping: resolvedCredentials, by: { $0.pool.id })
        let accounts = OpenAgentCredentials.merge(resolvedCredentials)
        let active = Set(accounts.map { $0.pool.id })
        let old = (cached ?? [:]).filter { active.contains($0.key) }, fetch = fetchQuota
        let identityNotices = Dictionary(uniqueKeysWithValues: resolved.map { ($0.0, $0.2.notice) })
        var results: [QuotaResult] = []
        accounts: for account in accounts.sorted(by: { $0.pool.id < $1.pool.id }) {
            var failures: [String] = [], allUnauthorized = true
            let identityNotice = identityNotices[account.pool.id] ?? nil
            for alias in aliases[account.pool.id] ?? [account] {
                do {
                    let quota = try await fetch(alias, now)
                    results.append(.init(credential: account, quota: quota, notice: identityNotice, at: now, readAt: now, plan: quota.plan))
                    continue accounts
                } catch {
                    failures.append(error.localizedDescription)
                    if (error as? ProviderHTTPError)?.isAuthentication != true { allUnauthorized = false }
                }
            }
            results.append(.init(credential: account, quota: nil,
                notice: ([identityNotice].compactMap { $0 } + Array(Set(failures)).sorted()).joined(separator: " · "),
                at: now, isActive: !allUnauthorized, readAt: old[account.pool.id]?.readAt, plan: old[account.pool.id]?.plan))
        }
        if Task.isCancelled { return }
        for result in results {
            if result.at != old[result.credential.pool.id]?.at, let quota = result.quota {
                await history.append(quota.windows.compactMap { window in
                    window.remaining.map { QuotaSample(agentId: window.id, timestamp: result.at, remainingPct: $0) }
                }, now: now)
            }
        }
        cached = Dictionary(uniqueKeysWithValues: results.map { ($0.credential.pool.id, $0) })
    }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), since = clock().addingTimeInterval(-max(AlertPolicy.insightsLookback, Double(historyHours) * 3600))
        // Read from the start of the day, where the ledger replaces the sessions from: OpenCode's database returns only
        // replies from the time it is given.
        let readFrom = SessionContributions.windowStart(since)
        var local = await sessions(readFrom)
        // Pi's observer keeps active runs fresh. An expired heartbeat ends activity without claiming success.
        for index in local.sessions.indices where local.sessions[index].client == .pi {
            local.sessions[index].turns = local.sessions[index].turns.map { SessionPhase.lapsed($0, at: now) }
        }
        let quotas = (cached ?? [:]).values.sorted { $0.credential.pool.id < $1.credential.pool.id }
        // Empty sets explicitly retire expired, removed, rejected, or superseded pools.
        var activePools: [String: Set<String>] = ["Kimi": [], "GLM": [], "OpenCode Go": []]
        for result in quotas where result.isActive { activePools[result.credential.pool.provider, default: []].insert(result.credential.pool.id) }
        await record(local, since: readFrom)
        let events = local.sessions.flatMap(\.events)
        var consumers: [String: AgentDescriptor] = [:]
        for item in local.sessions {
            for event in item.events {
                consumers[event.agentId] = AgentDescriptor(id: event.agentId, vendor: item.client.name, model: ModelCatalog.consumerName(of: event.agentId),
                    source: L10n.text("本地记录 · 计费归属未确认", "Local records · billing unconfirmed"), enabled: true,
                    billingPool: event.attribution?.pool)
            }
        }
        let live = local.sessions.compactMap { item -> LiveSession? in
            guard let start = item.start, let end = item.end, end >= since else { return nil }
            let last = item.events.max(by: { $0.timestamp < $1.timestamp })
            let agentID = item.currentModel?.id ?? last?.agentId ?? "\(item.client.rawValue)-model:Unknown"
            if consumers[agentID] == nil {
                consumers[agentID] = AgentDescriptor(id: agentID, vendor: item.client.name, model: ModelCatalog.consumerName(of: agentID),
                    source: L10n.text("本地会话", "Local session"), enabled: true)
            }
            // The newest turn is the last one listed.
            let running = SessionPhase.read(.init(turn: item.turns.last), rule: .turns, at: now).inFlight
            let unique = UsageAggregation.usageUnion([item.events])
            return LiveSession(id: item.id, agentId: agentID, task: item.title,
                terminal: item.workspace.map { URL(fileURLWithPath: $0).lastPathComponent }, startedAt: start, endedAt: running ? nil : end,
                pctOfWindow: nil, tokensIn: unique.reduce(0) { $0 + $1.tokensIn }, tokensOut: unique.reduce(0) { $0 + $1.tokensOut },
                client: item.client.name, transcriptPath: item.path.isEmpty ? nil : item.path, cacheReadTokens: unique.reduce(0) { $0 + $1.cacheReadTokens }, observedAt: now,
                workingDirectory: item.workspace, lastActivityAt: end, navigationTarget: item.navigation?.target)
        }
        var snapshots: [UsageSnapshot] = [], descriptors: [AgentDescriptor] = []
        var insights: [String: UsageInsights] = [:]
        var notices = local.notices, plans: [String: String] = [:], links: [String: Set<String>] = [:]
        var accounts: [String: [AccountObservation]] = ["Kimi": [], "GLM": [], "OpenCode Go": []]
        var services = apiServices()
        for result in quotas {
            let pool = result.credential.pool
            if let error = result.notice {
                // Shown under its vendor. The pool's account holds back the pool's rows; the vendor's other pools were read
                // on their own.
                let key = pool.provider
                notices[key] = [notices[key], "\(pool.label): \(error)"].compactMap { $0 }.joined(separator: " · ")
            }
            if result.isActive {
                services += result.credential.clients.sorted().map {
                    AgentService(client: $0, provider: pool.provider, product: .plan, accountID: pool.id,
                                 region: pool.realm == "CN" ? .china : pool.realm == "International" ? .international : nil)
                }
            }
            guard let quota = result.quota else {
                // A reading that failed without a sign-out keeps the account current, at its last reading, with the reason.
                if result.isActive {
                    accounts[pool.provider, default: []].append(AccountObservation(account: ProviderAccount(pool: pool), label: pool.label,
                        plan: result.plan, observedAt: result.readAt ?? result.at, quotaNotice: result.notice,
                        readingIssue: result.notice.map(ReadingIssue.readFailed)))
                }
                continue
            }
            if let plan = quota.plan {
                plans[pool.id] = plan
            }
            if result.isActive {
                // A reading whose account the service could not confirm is not verified either.
                // The pool's label names the account, where its windows are named by their periods.
                accounts[pool.provider, default: []].append(AccountObservation(account: ProviderAccount(pool: pool), label: pool.label,
                    plan: quota.plan, observedAt: result.at, quotaNotice: result.notice, readingIssue: result.notice.map(ReadingIssue.unverified)))
            }
            for window in quota.windows {
                let snapshot = UsageSnapshot(agentId: window.id, remainingPct: window.remaining, resetAt: window.reset,
                                             windowDuration: window.duration, updatedAt: result.at)
                snapshots.append(snapshot)
                descriptors.append(.init(id: window.id, vendor: pool.provider, model: window.label, shortModel: window.shortLabel ?? window.label,
                    source: result.credential.clients.sorted().joined(separator: ", "), enabled: true, billingPool: pool,
                    account: ProviderAccount(pool: pool), allModels: window.allModels))
                // Historical records lacking a pool must not inherit the current credential's quota.
                links[window.id] = Set(events.filter { $0.attribution?.pool == pool }.map(\.agentId))
                let readings = await history.samples(agentId: window.id, since: QuotaMath.historyStart(for: snapshot, now: now))
                insights[window.id] = QuotaMath.insights(snapshot: snapshot, samples: readings, now: now)
            }
        }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: live,
            notice: notices.isEmpty ? nil : notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · "),
            discoveredAgents: descriptors, consumers: consumers.values.sorted { $0.id < $1.id },
            indexing: local.indexing, insightsByAgent: insights, subscriptions: plans, sourceNotices: notices,
            // Every reading belongs to one pool, whose account carries its notice: none is the whole vendor's.
            quotaNotices: [:], readingIssues: [:], consumerIdsByQuota: links,
            completions: local.sessions.flatMap { item in
                item.completions.map { event in
                    var current = event
                    current.navigationTarget = item.navigation?.target
                    return current
                }
            }, turns: local.sessions.flatMap(\.turns), services: services,
            activeQuotaPoolIDs: cached == nil ? nil : activePools, accounts: cached == nil ? nil : accounts)
    }
}
