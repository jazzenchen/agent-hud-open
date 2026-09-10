import Foundation

/// Clients supply request observations. Billing services supply account observations, exactly once per pool.
actor OpenAgentUsageProvider: UsageProvider {
    private let credentials: @Sendable () -> [OpenAgentCredential]
    private let sessions: @Sendable (Date) async -> OpenAgentLocalStore.Result
    private let fetchQuota: @Sendable (OpenAgentCredential, Date) async throws -> ProviderQuota
    private let identify: @Sendable (OpenAgentCredential) async -> OpenAgentCredential
    private let apiServices: @Sendable () -> [AgentService]
    private var identities: [String: (at: Date, credential: OpenAgentCredential)] = [:]
    private let history: QuotaHistoryStore
    private let clock: @Sendable () -> Date
    private struct QuotaResult: Sendable {
        let credential: OpenAgentCredential
        let quota: ProviderQuota?
        let notice: String?
        let at: Date
    }
    private var cached: [String: QuotaResult] = [:]
    init(credentials: @escaping @Sendable () -> [OpenAgentCredential],
         sessions: @escaping @Sendable (Date) async -> OpenAgentLocalStore.Result,
         fetchQuota: @escaping @Sendable (OpenAgentCredential, Date) async throws -> ProviderQuota,
         history: QuotaHistoryStore, identify: @escaping @Sendable (OpenAgentCredential) async -> OpenAgentCredential = { $0 }, clock: @escaping @Sendable () -> Date = { Date() },
         apiServices: @escaping @Sendable () -> [AgentService] = { [] }) {
        self.credentials = credentials; self.sessions = sessions; self.fetchQuota = fetchQuota
        self.history = history; self.identify = identify; self.clock = clock
        self.apiServices = apiServices
    }
    static func standard(persistHistory: Bool = true) -> OpenAgentUsageProvider {
        let local = OpenAgentLocalStore(paths: .init(home: FileManager.default.homeDirectoryForCurrentUser, environment: ProcessInfo.processInfo.environment))
        return .init(credentials: { OpenAgentCredentials.discover() }, sessions: { await local.index(since: $0) },
            fetchQuota: { try await OpenAgentQuotaClient().fetch($0, now: $1) },
            history: QuotaHistoryStore(fileURL: persistHistory ? AppSupport.directory.appendingPathComponent("open-agent-quota-history.json") : nil),
            identify: { await OpenAgentQuotaClient().identify($0) }, apiServices: { AgentAPIServiceDiscovery.discover() })
    }
    private func quotas(now: Date) async -> [QuotaResult] {
        let raw = OpenAgentCredentials.merge(credentials())
        let activeCredentials = Set(raw.map { $0.pool.id })
        identities = identities.filter { activeCredentials.contains($0.key) }
        let prior = identities, identify = identify
        let resolved = await withTaskGroup(of: (String, OpenAgentCredential).self) { group in
            for credential in raw {
                group.addTask {
                    if let known = prior[credential.pool.id], (known.credential.pool.evidence == .account || now.timeIntervalSince(known.at) < 120) {
                        return (credential.pool.id, .init(service: known.credential.service, token: credential.token,
                            pool: known.credential.pool, headers: credential.headers, clients: credential.clients))
                    }
                    return (credential.pool.id, await identify(credential))
                }
            }
            var results: [(String, OpenAgentCredential)] = []
            for await result in group { results.append(result) }
            return results
        }
        for (key, credential) in resolved {
            if identities[key] == nil || now.timeIntervalSince(identities[key]!.at) >= 120 {
                identities[key] = (now, credential)
            }
        }
        let resolvedCredentials = resolved.sorted { $0.0 < $1.0 }.map { $0.1 }
        let aliases = Dictionary(grouping: resolvedCredentials, by: { $0.pool.id })
        let accounts = OpenAgentCredentials.merge(resolvedCredentials)
        let active = Set(accounts.map { $0.pool.id })
        cached = cached.filter { active.contains($0.key) }
        let old = cached, fetch = fetchQuota
        let results = await withTaskGroup(of: QuotaResult.self) { group in
            for account in accounts {
                group.addTask {
                    if let value = old[account.pool.id], now.timeIntervalSince(value.at) < 120 {
                        return .init(credential: account, quota: value.quota, notice: value.notice, at: value.at)
                    }
                    var failures: [String] = []
                    for alias in aliases[account.pool.id] ?? [account] {
                        do { return .init(credential: account, quota: try await fetch(alias, now), notice: nil, at: now) }
                        catch { failures.append(error.localizedDescription) }
                    }
                    return .init(credential: account, quota: nil, notice: Array(Set(failures)).sorted().joined(separator: " · "), at: now)
                }
            }
            var all: [QuotaResult] = []
            for await result in group { all.append(result) }
            return all.sorted { $0.credential.pool.id < $1.credential.pool.id }
        }
        for result in results {
            if result.at != cached[result.credential.pool.id]?.at, let quota = result.quota {
                await history.append(quota.windows.map { .init(agentId: $0.id, timestamp: result.at, remainingPct: $0.remaining) }, now: now)
            }
            cached[result.credential.pool.id] = result
        }
        return results
    }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock(), since = clock().addingTimeInterval(-Double(max(168, historyHours)) * 3600)
        async let fetched = quotas(now: now)
        let local = await sessions(since)
        let quotas = await fetched
        let events = UsageAggregation.usageUnion(local.sessions.map(\.events)).filter { $0.timestamp >= since && $0.timestamp <= now }
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
            let agentID = last?.agentId ?? "\(item.client.rawValue)-model:Unknown"
            if consumers[agentID] == nil {
                consumers[agentID] = AgentDescriptor(id: agentID, vendor: item.client.name, model: "Unknown",
                    source: L10n.text("本地会话", "Local session"), enabled: true)
            }
            let running = item.turns.last.map { $0.state == .running && now.timeIntervalSince1970 - Double($0.observedAtMs) / 1000 < 120 } ?? false
            let unique = UsageAggregation.usageUnion([item.events])
            return LiveSession(id: item.id, agentId: agentID, task: item.title,
                terminal: item.workspace.map { URL(fileURLWithPath: $0).lastPathComponent }, startedAt: start, endedAt: running ? nil : end,
                pctOfWindow: nil, tokensIn: unique.reduce(0) { $0 + $1.tokensIn }, tokensOut: unique.reduce(0) { $0 + $1.tokensOut },
                client: item.client.name, transcriptPath: item.path, cacheReadTokens: unique.reduce(0) { $0 + $1.cacheReadTokens })
        }
        var snapshots: [UsageSnapshot] = [], descriptors: [AgentDescriptor] = [], samples: [HistorySample] = []
        var insights: [String: UsageInsights] = [:]
        var notices = local.notices, plans: [String: String] = [:], links: [String: Set<String>] = [:]
        var services = apiServices()
        for result in quotas {
            let pool = result.credential.pool
            if let error = result.notice {
                let key = pool.provider == "OpenCode Go" ? "OpenCode" : pool.provider
                notices[key] = [notices[key], "\(pool.label): \(error)"].compactMap { $0 }.joined(separator: " · ")
            }
            guard let quota = result.quota else { continue }
            if let plan = quota.plan {
                plans[pool.id] = plan
                services += result.credential.clients.sorted().map {
                    AgentService(client: $0, provider: pool.provider, product: .plan, accountID: pool.id)
                }
            }
            for window in quota.windows {
                snapshots.append(.init(agentId: window.id, remainingPct: window.remaining, resetAt: window.reset,
                    windowDuration: window.duration, updatedAt: result.at))
                descriptors.append(.init(id: window.id, vendor: pool.provider, model: window.label + (quota.plan.map { " · " + $0 } ?? ""),
                    source: result.credential.clients.sorted().joined(separator: ", "), enabled: true, billingPool: pool))
                // Historical records lacking a pool must not inherit the current credential's quota.
                links[window.id] = Set(events.filter { $0.attribution?.pool == pool }.map(\.agentId))
                let readings = await history.samples(agentId: window.id, since: since)
                let snapshot = snapshots.last!
                let burn = UsageAnalytics.burnRate(samples: readings, cycle: snapshot.cycle, now: now)
                let caps = UsageAnalytics.capStats(samples: readings, now: now)
                insights[window.id] = UsageInsights(burnRatePctPerHour: burn?.pctPerHour,
                    timeToExhaust: burn?.timeToExhaust(remainingPct: window.remaining), weeklyCapHits: caps.hits,
                    weeklyWaitTotal: caps.totalWait, weeklyWaitLongest: caps.longestWait, weeklyWaitLongestAt: caps.longestAt,
                    weeklyShare: [:], windowSessionCount: 0, windowUsedPct: 100 - window.remaining)
                if let first = readings.first {
                    samples += UsageAnalytics.hourlyHistory(agentId: window.id, quota: readings, usage: [],
                        hours: min(historyHours, max(1, Int(now.timeIntervalSince(first.timestamp) / 3600) + 1)), now: now,
                        calendar: .current, fallbackRemaining: nil)
                }
            }
        }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: live, history: samples,
            activity: UsageAnalytics.activityGrid(usage: events, since: now.addingTimeInterval(-7 * 86400), calendar: .current),
            insights: .empty, notice: notices.isEmpty ? nil : notices.keys.sorted().map { "\($0): \(notices[$0]!)" }.joined(separator: " · "),
            discoveredAgents: descriptors, consumers: consumers.values.sorted { $0.id < $1.id }, consumption: events,
            indexing: local.indexing, insightsByAgent: insights, subscriptions: plans, sourceNotices: notices, consumerIdsByQuota: links,
            completions: local.sessions.flatMap(\.completions), turns: local.sessions.flatMap(\.turns), services: services)
    }
}
