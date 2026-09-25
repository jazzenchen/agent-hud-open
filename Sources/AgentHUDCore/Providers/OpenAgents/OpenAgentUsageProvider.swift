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
    }
    /// Nil until the first account scan completes; an empty result is an observed empty inventory.
    private var cached: [String: QuotaResult]?
    nonisolated let watchedDirectories: [URL]?
    private let ledger: UsageLedger
    private let sessionLedger: SessionLedger
    static let source = "open-agents"
    init(credentials: @escaping @Sendable () -> [OpenAgentCredential],
         sessions: @escaping @Sendable (Date) async -> OpenAgentLocalStore.Result,
         fetchQuota: @escaping @Sendable (OpenAgentCredential, Date) async throws -> ProviderQuota,
         history: QuotaHistoryStore, identify: @escaping @Sendable (OpenAgentCredential) async throws -> OpenAgentCredential = { $0 }, clock: @escaping @Sendable () -> Date = { Date() },
         identityCacheURL: URL? = nil,
         apiServices: @escaping @Sendable () -> [AgentService] = { [] },
         watchedDirectories: [URL]? = nil, ledger: UsageLedger = .inMemory()) {
        self.credentials = credentials; self.sessions = sessions; self.fetchQuota = fetchQuota
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
            watchedDirectories: [paths.openCode, paths.piTurns] + paths.roots(for: .kimi) + paths.roots(for: .pi), ledger: ledger)
    }

    /// Token totals of every open agent client from the period holding `since`.
    func usage(since: Date) async -> [UsageBucket] {
        (try? await ledger.buckets(since: since, source: Self.source)) ?? []
    }

    /// Writes each session's usage once the index is complete and something changed.
    private func record(_ local: OpenAgentLocalStore.Result, since: Date) async {
        guard local.indexing == nil else { return }
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
            if let known = prior[credential.pool.id], (known.pool.evidence == .account || now.timeIntervalSince(known.at) < UsageRefresh.accountRequestSpacing) {
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
            if let value = old[account.pool.id], now.timeIntervalSince(value.at) < UsageRefresh.accountRequestSpacing {
                results.append(.init(credential: account, quota: value.quota, notice: value.notice, at: value.at, isActive: value.isActive))
                continue
            }
            var failures: [String] = [], allUnauthorized = true
            let identityNotice = identityNotices[account.pool.id] ?? nil
            for alias in aliases[account.pool.id] ?? [account] {
                do {
                    results.append(.init(credential: account, quota: try await fetch(alias, now), notice: identityNotice, at: now))
                    continue accounts
                } catch {
                    failures.append(error.localizedDescription)
                    if (error as? ProviderHTTPError)?.isAuthentication != true { allUnauthorized = false }
                }
            }
            results.append(.init(credential: account, quota: nil,
                notice: ([identityNotice].compactMap { $0 } + Array(Set(failures)).sorted()).joined(separator: " · "),
                at: now, isActive: !allUnauthorized))
        }
        if Task.isCancelled { return }
        for result in results {
            if result.at != old[result.credential.pool.id]?.at, let quota = result.quota {
                await history.append(quota.windows.map { .init(agentId: $0.id, timestamp: result.at, remainingPct: $0.remaining) }, now: now)
            }
        }
        cached = Dictionary(uniqueKeysWithValues: results.map { ($0.credential.pool.id, $0) })
    }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), since = clock().addingTimeInterval(-Double(max(168, historyHours)) * 3600)
        var local = await sessions(since)
        // Pi's observer keeps active runs fresh. An expired heartbeat ends activity without claiming success.
        for index in local.sessions.indices where local.sessions[index].client == .pi {
            local.sessions[index].turns = local.sessions[index].turns.map { turn in
                guard turn.state == .running, now.timeIntervalSince1970 - Double(turn.observedAtMs) / 1000 >= 120 else { return turn }
                return SessionTurn(provider: turn.provider, sessionID: turn.sessionID, turnID: turn.turnID,
                    state: .ended, startedAtMs: turn.startedAtMs, observedAtMs: turn.observedAtMs)
            }
        }
        let quotas = (cached ?? [:]).values.sorted { $0.credential.pool.id < $1.credential.pool.id }
        // Empty sets explicitly retire expired, removed, rejected, or superseded pools.
        var activePools: [String: Set<String>] = ["Kimi": [], "GLM": [], "OpenCode Go": []]
        for result in quotas where result.isActive { activePools[result.credential.pool.provider, default: []].insert(result.credential.pool.id) }
        await record(local, since: since)
        let events = local.sessions.flatMap(\.events)
        var consumers: [String: AgentDescriptor] = [:]
        for item in local.sessions {
            for event in item.events {
                let route = event.attribution?.providerID ?? "Unknown"
                consumers[event.agentId] = AgentDescriptor(id: event.agentId, vendor: item.client.name,
                    model: "\(item.models[event.agentId] ?? "Unknown") · \(route)",
                    source: L10n.text("本地记录 · 计费归属未确认", "Local records · billing unconfirmed"), enabled: true,
                    billingPool: event.attribution?.pool)
            }
        }
        let live = local.sessions.compactMap { item -> LiveSession? in
            guard let start = item.start, let end = item.end, end >= since else { return nil }
            let last = item.events.max(by: { $0.timestamp < $1.timestamp })
            let agentID = item.currentModel?.id ?? last?.agentId ?? "\(item.client.rawValue)-model:Unknown"
            if consumers[agentID] == nil {
                consumers[agentID] = AgentDescriptor(id: agentID, vendor: item.client.name,
                    model: item.currentModel.map { "\($0.name) · \($0.provider)" } ?? "Unknown",
                    source: L10n.text("本地会话", "Local session"), enabled: true)
            }
            // A quiet log does not end a turn: one tool call can take minutes without writing a line. Only silence
            // long enough to mean the client is gone does.
            let running = item.turns.last.map {
                $0.state == .running && now.timeIntervalSince1970 - Double($0.observedAtMs) / 1000 < UsageRefresh.abandonedTurnTimeout
            } ?? false
            let unique = UsageAggregation.usageUnion([item.events])
            return LiveSession(id: item.id, agentId: agentID, task: item.title,
                terminal: item.workspace.map { URL(fileURLWithPath: $0).lastPathComponent }, startedAt: start, endedAt: running ? nil : end,
                pctOfWindow: nil, tokensIn: unique.reduce(0) { $0 + $1.tokensIn }, tokensOut: unique.reduce(0) { $0 + $1.tokensOut },
                client: item.client.name, transcriptPath: item.path.isEmpty ? nil : item.path, cacheReadTokens: unique.reduce(0) { $0 + $1.cacheReadTokens }, observedAt: now,
                workingDirectory: item.workspace)
        }
        var snapshots: [UsageSnapshot] = [], descriptors: [AgentDescriptor] = []
        var insights: [String: UsageInsights] = [:]
        var notices = local.notices, plans: [String: String] = [:], links: [String: Set<String>] = [:]
        var accounts: [String: [AccountObservation]] = ["Kimi": [], "GLM": [], "OpenCode Go": []]
        var services = apiServices()
        for result in quotas {
            let pool = result.credential.pool
            if let error = result.notice {
                let key = pool.provider == "OpenCode Go" ? "OpenCode" : pool.provider
                notices[key] = [notices[key], "\(pool.label): \(error)"].compactMap { $0 }.joined(separator: " · ")
            }
            if result.isActive {
                services += result.credential.clients.sorted().map {
                    AgentService(client: $0, provider: pool.provider, product: .plan, accountID: pool.id,
                                 region: pool.realm == "CN" ? .china : pool.realm == "International" ? .international : nil)
                }
            }
            guard let quota = result.quota else { continue }
            if let plan = quota.plan {
                plans[pool.id] = plan
            }
            if result.isActive {
                accounts[pool.provider, default: []].append(AccountObservation(account: ProviderAccount(pool: pool), plan: quota.plan, observedAt: result.at))
            }
            for window in quota.windows {
                snapshots.append(.init(agentId: window.id, remainingPct: window.remaining, resetAt: window.reset,
                    windowDuration: window.duration, updatedAt: result.at))
                descriptors.append(.init(id: window.id, vendor: pool.provider, model: window.label + (quota.plan.map { " · " + $0 } ?? ""),
                    source: result.credential.clients.sorted().joined(separator: ", "), enabled: true, billingPool: pool,
                    account: ProviderAccount(pool: pool)))
                // Historical records lacking a pool must not inherit the current credential's quota.
                links[window.id] = Set(events.filter { $0.attribution?.pool == pool }.map(\.agentId))
                let readings = await history.samples(agentId: window.id, since: since)
                let snapshot = snapshots.last!
                let burn = UsageAnalytics.burnRate(samples: readings, cycle: snapshot.cycle, now: now)
                let caps = UsageAnalytics.capStats(samples: readings, now: now)
                insights[window.id] = UsageInsights(burnRatePctPerHour: burn?.pctPerHour,
                    timeToExhaust: burn?.timeToExhaust(remainingPct: window.remaining), weeklyCapHits: caps.hits,
                    weeklyWaitTotal: caps.totalWait, weeklyWaitLongest: caps.longestWait, weeklyWaitLongestAt: caps.longestAt)
            }
        }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: live,
            notice: notices.isEmpty ? nil : notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · "),
            discoveredAgents: descriptors, consumers: consumers.values.sorted { $0.id < $1.id },
            indexing: local.indexing, insightsByAgent: insights, subscriptions: plans, sourceNotices: notices, consumerIdsByQuota: links,
            completions: local.sessions.flatMap(\.completions), turns: local.sessions.flatMap(\.turns), services: services,
            activeQuotaPoolIDs: cached == nil ? nil : activePools, accounts: cached == nil ? nil : accounts)
    }
}
