import Foundation
import Observation

/// One agent as shown in the hover panel / menu: descriptor + latest reading.
public struct AgentRow: Hashable, Sendable, Identifiable {
    public let agent: AgentDescriptor
    public let remainingPct: Double?
    public let level: StatusLevel?
    public let resetAt: Date?
    public let weeklyRemainingPct: Double?
    /// Index into `AgentPalette` (position among enabled agents).
    public let paletteIndex: Int

    public var id: String { agent.id }

    /// Share of the window already consumed; the UI shows usage, not what is left.
    public var usedPct: Double? { remainingPct.map { max(0, min(100, 100 - $0)) } }

    public var missingQuotaLabel: String {
        "—"
    }
}

/// Observable app state: polls the provider, exposes derived rows, glow appearance and stats selections.
@MainActor
@Observable
public final class UsageStore {
    public private(set) var report: UsageReport?
    public private(set) var lastError: String?
    public private(set) var pausedUntil: Date?
    public private(set) var isRefreshing = false
    public private(set) var statsRange: StatsRange = .hours24
    public var tokenBucketSize: TokenBucketSize = .hour1
    public var tokenDimensions: TokenDimensions = .fresh
    /// Keep every selectable range ready, including the partial hour at the start of the rolling window.
    public static var historyHours: Int { StatsRange.days7.hours + 1 }
    public var selectedQuotaId: String?
    public var glowHidden = false
    /// Advances every few seconds so countdowns re-render.
    public private(set) var now = Date()

    public let settings: SettingsStore
    private let provider: any UsageProvider
    private let accessAllowed: () -> Bool
    private var pollTask: Task<Void, Never>?
    private var clockTask: Task<Void, Never>?
    private var accountTask: Task<Void, Never>?

    public init(provider: any UsageProvider, settings: SettingsStore, accessAllowed: @escaping () -> Bool = { true }) {
        self.provider = provider
        self.settings = settings
        self.accessAllowed = accessAllowed
    }

    public var isAccessAllowed: Bool { accessAllowed() }

    // MARK: Lifecycle

    public func start() {
        stop()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                await self.refresh()
                // While local logs are still being indexed, poll quickly so each step lands on screen.
                let settings = self.settings.settings
                // Completion reminders tail local logs promptly; provider quota/balance queries retain their own caches.
                let interval = min(5, settings.pollInterval.rawValue)
                let seconds = self.isIndexing ? 2 : interval
                try? await Task.sleep(for: .seconds(seconds))
            }
        }
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(10))
                self?.now = Date()
            }
        }
    }

    public func stop() {
        pollTask?.cancel()
        clockTask?.cancel()
        accountTask?.cancel()
        pollTask = nil
        clockTask = nil
    }

    public func refresh() async {
        guard isAccessAllowed, !isRefreshing else { return }
        if let pausedUntil, pausedUntil > Date() { return }
        pausedUntil = nil
        if accountTask == nil {
            accountTask = Task { [weak self, provider] in
                await provider.refreshAccountUsage(historyHours: Self.historyHours)
                self?.accountTask = nil
            }
        }
        isRefreshing = true
        defer { isRefreshing = false }
        do {
            let fetched = try await provider.fetchUsage(agents: settings.agents, historyHours: Self.historyHours)
            guard isAccessAllowed, !Task.isCancelled else { return }
            settings.mergeDiscovered(fetched.discoveredAgents, activeQuotaPoolIDs: fetched.activeQuotaPoolIDs)
            report = fetched
            lastError = nil
        } catch {
            guard isAccessAllowed, !Task.isCancelled else { return }
            lastError = error.localizedDescription
        }
        now = Date()
    }

    /// Installs a report directly (snapshots, tests) without going through the provider.
    public func replace(report: UsageReport) {
        self.report = report
        lastError = nil
        now = Date()
    }

    public func pause(for interval: TimeInterval) {
        pausedUntil = Date().addingTimeInterval(interval)
    }

    public func resume() {
        pausedUntil = nil
        Task { await refresh() }
    }

    public var isPaused: Bool {
        guard let pausedUntil else { return false }
        return pausedUntil > now
    }

    // MARK: Stats selections

    public func setStatsRange(_ range: StatsRange) {
        guard range != statsRange else { return }
        statsRange = range
    }

    // MARK: Derived

    public var enabledAgents: [AgentDescriptor] {
        settings.enabledAgents.filter { agent in
            guard let pool = agent.billingPool, pool.product == .plan else { return true }
            guard let report else { return false }
            return report.activeQuotaPoolIDs?[pool.provider]?.contains(pool.id) ?? true
        }
    }

    public var rows: [AgentRow] {
        enabledAgents.filter { !$0.isAPIBilled }.enumerated().map { index, agent in
            let snapshot = report?.snapshot(for: agent.id)
            return AgentRow(
                agent: agent,
                remainingPct: snapshot?.remainingPct,
                level: snapshot.map { AlertPolicy.quotaLevel(remaining: $0.remainingPct) },
                resetAt: snapshot?.resetAt,
                weeklyRemainingPct: snapshot?.weeklyRemainingPct,
                paletteIndex: index
            )
        }
    }

    public func row(for agentId: String) -> AgentRow? { rows.first { $0.id == agentId } }

    public func quotaForecastHint(for agentId: String) -> String? {
        guard let report, let snapshot = report.snapshot(for: agentId) else { return nil }
        return QuotaForecast.hint(snapshot: snapshot, insights: report.insightsByAgent[agentId], now: now)
    }

    /// Account cards shown in the island and menu follow the agent switches.
    public var enabledBilling: [APIBilling] {
        let vendors = Set(enabledAgents.map(\.vendor))
        return (report?.billing ?? []).filter { billing in
            billing.billingPool.map { pool in enabledAgents.contains { $0.billingPool?.id == pool.id } } ?? vendors.contains(billing.vendor)
        }
    }

    public func balanceLevel(_ balance: AccountBalance, billing: APIBilling) -> StatusLevel? {
        guard !balance.total.isNaN else { return nil }
        if billing.isAvailable == false { return .critical }
        return AlertPolicy.balanceLevel(remaining: balance.total, currency: balance.currency)
    }

    public var primaryRow: AgentRow? {
        rows.first { $0.id == selectedQuotaId } ?? rows.first { $0.remainingPct != nil } ?? rows.first
    }

    public var primaryInsights: UsageInsights {
        guard let report else { return .empty }
        if let id = primaryRow?.id, let scoped = report.insightsByAgent[id] { return scoped }
        return report.insightsByAgent.isEmpty ? report.insights : .empty
    }

    /// Status per enabled agent that has data, in glow order. Agents without a reading stay out of the glow.
    public var levels: [StatusLevel] {
        let quota = Dictionary(uniqueKeysWithValues: rows.compactMap { row in row.level.map { (row.id, $0) } })
        let accounts = enabledBilling
        var seenAccounts: Set<String> = []
        return enabledAgents.flatMap { model -> [StatusLevel] in
            if !model.isAPIBilled { return quota[model.id].map { [$0] } ?? [] }
            return accounts.filter { $0.contains(model) && seenAccounts.insert($0.id).inserted }.compactMap { billing in
                if billing.isAvailable == false { return .critical }
                let levels = billing.balances.compactMap { balanceLevel($0, billing: billing) }
                return levels.contains(.critical) ? .critical : levels.contains(.warning) ? .warning : levels.first
            }
        }
    }

    /// When the quota numbers were last reported (engine fetch time), falling back to the poll time.
    public var quotaUpdatedAt: Date? { report?.snapshots.first?.updatedAt ?? report?.generatedAt }

    public var isIndexing: Bool { report?.indexing != nil }

    /// Includes the brief interval before the first refresh starts; a failed fetch ends loading.
    public var isLoading: Bool { isAccessAllowed && report == nil && !isPaused && (isRefreshing || lastError == nil) }

    public var minRemainingPct: Double? { rows.compactMap(\.remainingPct).min() }

    /// The most consumed window, shown in the menu bar.
    public var maxUsedPct: Double? { rows.compactMap(\.usedPct).max() }

    public var sessions: [LiveSession] {
        let sessions = report?.sessions ?? []
        return sessions.sorted {
            let left = isSessionLive($0), right = isSessionLive($1)
            if left != right { return left }
            return ($0.endedAt ?? $0.startedAt) > ($1.endedAt ?? $1.startedAt)
        }
    }

    public func sessionSource(_ session: LiveSession) -> SessionSource {
        let vendor = consumers.first { $0.id == session.agentId }?.vendor
            ?? settings.agents.first { $0.id == session.agentId }?.vendor
            ?? SessionSource.vendor(impliedBy: session.agentId)
        return SessionSource(vendor: vendor, client: session.client)
    }

    public var subscriptions: [String: String] {
        let vendors = Set(enabledAgents.map(\.vendor))
        return (report?.subscriptions ?? [:]).filter { vendors.contains($0.key) }
    }

    public var dataDate: Date { report?.generatedAt ?? now }
    public var statsInterval: DateInterval { statsRange.interval(endingAt: dataDate) }

    /// Include sessions active during the selected range, including ones that started before it.
    public var statsSessions: [LiveSession] {
        let interval = statsInterval
        return sessions.filter { $0.startedAt <= interval.end && ($0.endedAt ?? now) >= interval.start }
    }

    public func liveStatusEnabled(for session: LiveSession) -> Bool {
        settings.settings.liveStatusEnabled(for: sessionSource(session).vendor ?? "")
    }

    public func isSessionLive(_ session: LiveSession) -> Bool {
        session.isLive(at: now) && liveStatusEnabled(for: session)
    }

    public func sessionStatusLabel(_ session: LiveSession) -> String {
        guard liveStatusEnabled(for: session) else { return L10n.text("状态同步已关闭", "Live status off") }
        if session.isLive && !session.isLive(at: now) { return L10n.text("状态待更新", "Status out of date") }
        return Countdown.sessionLabel(session, now: now)
    }

    public var liveSessions: [LiveSession] { sessions.filter(isSessionLive) }

    /// Keep every running session and the most recent session from each client visible.
    public func sessionPreview(from sessions: [LiveSession]) -> [LiveSession] {
        var seen = Set<SessionSource>()
        return sessions.filter { session in
            let first = seen.insert(sessionSource(session)).inserted
            return isSessionLive(session) || first
        }
    }

    public var hasLiveSession: Bool { !liveSessions.isEmpty }

    public var updatedAt: Date? { report?.generatedAt }

    public func glowAppearance(light: Bool) -> GlowAppearance {
        let appearance = GlowAppearance.resolve(
            levels: levels,
            paused: isPaused || !isAccessAllowed,
            anyAgentActive: hasLiveSession,
            settings: settings.settings,
            light: light
        )
        guard glowHidden else { return appearance }
        return GlowAppearance(
            stops: appearance.stops, peakOpacity: appearance.peakOpacity, troughOpacity: appearance.troughOpacity,
            breathing: appearance.breathing, breathSeconds: appearance.breathSeconds, hidden: true
        )
    }

    /// Last `hours` buckets of one agent's history.
    public func history(for agentId: String, lastHours hours: Int? = nil) -> [HistorySample] {
        let all = report?.history(for: agentId) ?? []
        guard let hours, all.count > hours else { return all }
        return Array(all.suffix(hours))
    }

    // MARK: Consumers (token spenders, e.g. model families)

    /// Token spend is independent of which remaining-quota windows the user monitors.
    public var consumers: [AgentDescriptor] { report?.consumers ?? [] }

    /// Both surfaces show every model's token spend.
    public var tokenColumns: [TokenColumn] {
        ChartData.tokenBars(usage: report?.consumption ?? [], agentIds: consumers.map(\.id), range: statsRange, bucketSize: tokenBucketSize,
                            now: dataDate, dimensions: tokenDimensions)
    }

    public var statsActivity: ActivityGrid {
        UsageAnalytics.activityGrid(usage: (report?.consumption ?? []).filter { $0.timestamp <= dataDate },
            since: dataDate.addingTimeInterval(-7 * 86400), calendar: .current, dimensions: tokenDimensions)
    }

    public var weeklyTokenShare: [String: Double] {
        var totals: [String: Int] = [:]
        for event in report?.consumption ?? [] where event.timestamp >= dataDate.addingTimeInterval(-7 * 86400) && event.timestamp <= dataDate {
            totals[event.agentId, default: 0] += tokenDimensions.count(event)
        }
        let total = totals.values.reduce(0, +)
        return total > 0 ? totals.mapValues { Double($0) / Double(total) } : [:]
    }

    /// Quota switches do not change token-chart colors.
    public func consumerPaletteIndex(_ id: String) -> Int {
        consumers.firstIndex { $0.id == id } ?? 0
    }

    public func consumerName(_ id: String) -> String {
        guard let consumer = consumers.first(where: { $0.id == id }) else {
            guard let row = rows.first(where: { $0.id == id }) else { return id }
            return L10n.modelLabel(row.agent.model)
        }
        return L10n.modelLabel(consumer.model)
    }

    /// Rows grouped by vendor, preserving order.
    public var rowGroups: [(vendor: String, rows: [AgentRow])] {
        var order: [String] = []
        var groups: [String: [AgentRow]] = [:]
        for row in rows {
            let vendor = row.agent.displayVendor
            if groups[vendor] == nil { order.append(vendor) }
            groups[vendor, default: []].append(row)
        }
        return order.map { ($0, groups[$0] ?? []) }
    }

    /// "周额度 · Claude 61% · ChatGPT 80%" source: first weekly reading per vendor.
    public var weeklyByVendor: [(vendor: String, pct: Double)] {
        var seen: Set<String> = []
        return rows.compactMap { row in
            guard let pct = row.weeklyRemainingPct, !seen.contains(row.agent.vendor) else { return nil }
            seen.insert(row.agent.vendor)
            return (row.agent.vendor, pct)
        }
    }
}
