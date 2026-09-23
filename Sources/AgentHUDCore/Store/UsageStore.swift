import AgentHUDSupport
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
    /// The account this window belongs to, when the provider identifies accounts.
    public let account: AccountObservation?
    /// Other accounts show their last reading without a status level, so they stay out of the glow and alerts.
    public let isCurrentAccount: Bool

    public var id: String { agent.id }

    /// Share of the window already consumed; the UI shows usage, not what is left.
    public var usedPct: Double? { remainingPct.map { max(0, min(100, 100 - $0)) } }

    public var missingQuotaLabel: String {
        "—"
    }

    public func resetLabel(now: Date, compact: Bool = false) -> String {
        guard isCurrentAccount else { return "—" }
        if let resetAt {
            if resetAt <= now { return L10n.text("等待更新", "Pending update") }
            if resetAt.timeIntervalSince(now) < 60 { return "<1m" }
        }
        return compact ? Countdown.resetLabelCompact(resetAt, now: now) : Countdown.resetLabel(resetAt, now: now)
    }
}

extension UsageReport {
    /// The times at which this report's activity changes with time alone: when a running turn reaches the age at which
    /// it no longer counts as current, and when a session whose source never said what its turn is doing reaches the
    /// age at which a quiet log ends it. A source is read again at these times instead of being polled.
    var activityChecks: [Date] {
        let margin: TimeInterval = 1
        var times = sessions.filter(\.isLive).map { $0.observedAt.addingTimeInterval(UsageRefresh.liveThreshold + margin) }
        for turn in turns where turn.state == .running {
            let observed = RecordCoding.date(turn.observedAtMs)
            times.append(observed.addingTimeInterval(UsageRefresh.liveThreshold + margin))
            times.append(observed.addingTimeInterval(UsageRefresh.activeTurnFreshness + margin))
            times.append(observed.addingTimeInterval(UsageRefresh.abandonedTurnTimeout + margin))
        }
        return times
    }

    /// When an account reading of this source is next worth taking, counted from `since`, when its steps last ran.
    /// A window moves only while work runs, so a running turn is read often, a session between turns slowly, and work
    /// that finished after the last reading once more. An idle source's windows change only when they reset, and a
    /// source that cannot see this Mac's work keeps the account interval.
    func accountCheck(since: Date, now: Date, seesLocalWork: Bool) -> Date {
        // A deadline remains due until an account request has actually run at or after it.
        // Comparing with `now` loses the scheduled refresh as soon as the deadline arrives.
        let reset = snapshots.filter { snapshot in
            discoveredAgents.first(where: { $0.id == snapshot.agentId }).map(isCurrent) ?? true
        }.compactMap(\.resetAt).filter { $0 > since }.min()
        let regular: Date
        let stale = now.addingTimeInterval(-UsageRefresh.activeTurnFreshness)
        if turns.contains(where: { $0.state == .running && RecordCoding.date($0.observedAtMs) > stale }) {
            regular = since.addingTimeInterval(UsageRefresh.runningAccountInterval)
        } else if sessions.contains(where: { $0.isLive(at: now) }) {
            regular = since.addingTimeInterval(UsageRefresh.liveAccountInterval)
        } else if !seesLocalWork {
            regular = since.addingTimeInterval(UsageRefresh.accountInterval)
        } else if sessions.contains(where: { ($0.endedAt ?? .distantPast) > since }) {
            regular = now
        } else {
            regular = reset == nil || snapshots.contains(where: { ($0.resetAt ?? .distantFuture) <= since })
                ? since.addingTimeInterval(UsageRefresh.accountInterval) : .distantFuture
        }
        return min(regular, reset ?? .distantFuture)
    }
}

/// Keeps a change handler registered with `UsageStore.observeChanges(_:)`; releasing it unregisters the handler.
public final class UsageChangeObservation {
    private var cancellation: (() -> Void)?
    init(_ cancellation: @escaping () -> Void) { self.cancellation = cancellation }
    public func cancel() { cancellation?(); cancellation = nil }
    deinit { cancellation?() }
}

/// Rows of one account inside a vendor group.
public struct AccountSection: Identifiable, Sendable {
    public let id: String
    public let account: AccountObservation?
    public let isCurrent: Bool
    public let rows: [AgentRow]
}

/// Observable app state: the collected report, derived rows, glow appearance and stats selections.
@MainActor
@Observable
public final class UsageStore {
    public internal(set) var report: UsageReport?
    public internal(set) var lastError: String?
    public internal(set) var pausedUntil: Date?
    public internal(set) var isRefreshing = false
    public private(set) var statsRange: StatsRange = .hours24
    public var tokenBucketSize: TokenBucketSize = .hour1
    public var tokenDimensions: TokenDimensions = .fresh
    /// Keep every selectable range ready, including the partial hour at the start of the rolling window.
    public static var historyHours: Int { StatsRange.days7.hours + 1 }
    /// The quota window the statistics window points out, set by whatever opened it.
    public var selectedQuotaId: String?
    /// The session the statistics window shows in place of its overview; nil shows the overview.
    public var focusedSessionID: String?
    public var glowHidden = false
    /// Advances every few seconds so countdowns re-render.
    public internal(set) var now = Date()

    public let settings: SettingsStore
    private let accessAllowed: () -> Bool
    private let collector: UsageCollector
    private var clockTask: Task<Void, Never>?
    /// A later poll that found nothing changed extends the report's coverage.
    var checkedAt: Date?
    @ObservationIgnored private var changeObservers: [UUID: @MainActor (UsageChanges) -> Void] = [:]

    /// `hooks` let a host choose the history window, publish each provider report and merge it into the displayed report.
    public init(provider: any UsageProvider, settings: SettingsStore, accessAllowed: @escaping () -> Bool = { true },
                hooks: UsageCollectionHooks = UsageCollectionHooks()) {
        self.settings = settings
        self.accessAllowed = accessAllowed
        collector = UsageCollector(provider: provider, settings: settings, hooks: hooks)
        collector.store = self
    }

    public var isAccessAllowed: Bool { accessAllowed() }

    // MARK: Lifecycle

    public func start() {
        stop()
        collector.start()
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(10))
                guard let self else { return }
                self.now = Date()
                self.collector.confirmQuiet(at: self.now)
            }
        }
    }

    public func stop() {
        collector.stop()
        clockTask?.cancel()
        clockTask = nil
    }

    /// Reads local data now unless a pass is already running, in which case the next pass reads it.
    public func refresh() async {
        await collector.refresh()
    }

    /// Reads the accounts again as soon as the next pass can, for when someone looks at the numbers instead of waiting
    /// for work to move them. Every provider's own request spacing still holds, so looking twice in a minute reads once.
    public func refreshAccounts() async {
        await collector.refreshAccounts()
    }

    /// Runs only the merge hook again on the provider's last report, for data the merge adds that changed since the pass.
    /// Never overlaps a local poll: one in progress merges for it.
    public func remerge() async {
        await collector.remerge()
    }

    /// Installs a report directly (snapshots, tests) without going through the provider.
    public func replace(report: UsageReport) {
        show(report)
        collector.forgetLocalReport()
        checkedAt = nil
        lastError = nil
        now = Date()
    }

    /// Calls `handler` with what each newly displayed report changed, until the returned observation is released or cancelled.
    public func observeChanges(_ handler: @escaping @MainActor (UsageChanges) -> Void) -> UsageChangeObservation {
        let id = UUID()
        changeObservers[id] = handler
        return UsageChangeObservation { [weak self] in
            // An observation can be released off the main thread; the handler is then removed on it.
            guard Thread.isMainThread else {
                Task { @MainActor [weak self] in _ = self?.changeObservers.removeValue(forKey: id) }
                return
            }
            MainActor.assumeIsolated { _ = self?.changeObservers.removeValue(forKey: id) }
        }
    }

    /// A pass's merged report replaces the displayed one.
    func collected(_ report: UsageReport) {
        show(report)
        checkedAt = nil
        lastError = nil
    }

    /// A merge run again on the provider's last report.
    func merged(_ report: UsageReport) {
        show(report)
    }

    private func show(_ report: UsageReport) {
        let changes = UsageChanges(from: self.report, to: report)
        self.report = report
        guard !changes.isEmpty else { return }
        for handler in changeObservers.values { handler(changes) }
    }

    public func pause(for interval: TimeInterval) {
        pausedUntil = Date().addingTimeInterval(interval)
    }

    public func resume() {
        pausedUntil = nil
        Task { await refreshAccounts() }
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
            let isCurrent = report?.isCurrent(agent) ?? true
            return AgentRow(
                agent: agent,
                remainingPct: snapshot?.remainingPct,
                level: isCurrent && report?.quotaNotice(for: agent) == nil ? snapshot.flatMap {
                    ($0.resetAt ?? .distantFuture) > now && now.timeIntervalSince($0.updatedAt) < QuotaForecast.maximumReadingAge
                        ? AlertPolicy.quotaLevel(remaining: $0.remainingPct) : nil
                } : nil,
                resetAt: snapshot?.resetAt,
                weeklyRemainingPct: snapshot?.weeklyRemainingPct,
                paletteIndex: index,
                account: agent.account.flatMap { report?.observation(accountID: $0.id) },
                isCurrentAccount: isCurrent
            )
        }
    }

    public func row(for agentId: String) -> AgentRow? { rows.first { $0.id == agentId } }

    public func quotaForecastHint(for agentId: String) -> String? {
        guard let report, let snapshot = report.snapshot(for: agentId) else { return nil }
        return QuotaForecast.hint(snapshot: snapshot, insights: report.insightsByAgent[agentId], now: now)
    }

    /// Tokens per hour over the observed part of this quota window's current cycle. This is the same reading sent to
    /// the phone: token history before the local ledger begins is not guessed at.
    public func quotaTokensPerHour(for agentId: String) -> Double? {
        guard let report, let snapshot = report.snapshot(for: agentId),
              let consumers = report.consumerIdsByQuota[agentId], !consumers.isEmpty,
              let cycle = snapshot.cycle, let oldest = report.usage.first?.start else { return nil }
        let start = max(cycle.start, oldest)
        let hours = now.timeIntervalSince(start) / 3600
        guard hours > 0 else { return nil }
        let tokens = report.usage.reduce(0) { total, bucket in
            consumers.contains(bucket.agentId) && bucket.start >= start && bucket.start < now
                ? total + bucket.total : total
        }
        return (Double(tokens) / hours).rounded()
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

    public var isIndexing: Bool { report?.indexing != nil }

    /// Includes the brief interval before the first refresh starts; a failed fetch ends loading.
    public var isLoading: Bool { isAccessAllowed && report == nil && !isPaused && (isRefreshing || lastError == nil) }

    /// The most consumed window of a signed-in account, shown in the menu bar.
    public var maxUsedPct: Double? { rows.filter(\.isCurrentAccount).compactMap(\.usedPct).max() }

    /// Newest first, by the last event each source reported: a prompt, a reply, a tool result or an approval request.
    /// A running session nothing has been heard from for half an hour sits below one that just answered.
    public var sessions: [LiveSession] {
        let events = lastTurnEvents
        return (report?.sessions ?? []).sorted {
            let left = $0.lastEvent(turnAt: events[$0.id]), right = $1.lastEvent(turnAt: events[$1.id])
            return left == right ? $0.id < $1.id : left > right
        }
    }

    /// The newest turn event per session, read once instead of once per session.
    private var lastTurnEvents: [String: Date] {
        (report?.turns ?? []).reduce(into: [:]) { events, turn in
            let at = RecordCoding.date(turn.observedAtMs)
            if at > events[turn.sessionID] ?? .distantPast { events[turn.sessionID] = at }
        }
    }

    /// How long after its last turn a vendor still belongs in a logo queue.
    public static let queueRecency: TimeInterval = 24 * 3600

    /// What a logo queue shows: every watched window's vendor, in the order they are watched, then any
    /// vendor that ran within the last day and has no window on that list, most recently used first. An
    /// agent used this morning belongs in the row whether or not its quota is being followed; one nobody
    /// has run for a day and nobody watches does not. A vendor whose Live status is off is not counted as
    /// having run, since that switch is what says its runs may be reported at all.
    public var queueVendors: [(vendor: String, isWorking: Bool)] {
        let working = workingVendors
        var order = rows.map(\.agent.vendor)
        var seen = Set(order)
        let cutoff = now.addingTimeInterval(-Self.queueRecency)
        let events = lastTurnEvents
        for session in sessions where session.lastEvent(turnAt: events[session.id]) >= cutoff {
            guard liveStatusEnabled(for: session), let vendor = sessionSource(session).vendor,
                  seen.insert(vendor).inserted else { continue }
            order.append(vendor)
        }
        return order.map { (vendor: $0, isWorking: working.contains($0)) }
    }

    /// The vendors with work in flight, by the same liveness the panel ranks sessions with. A session names
    /// the model it spends rather than the quota row it belongs to, so its vendor is resolved instead of its
    /// id being compared with an agent's — which matched only in the demo, where the two happen to be equal.
    public var workingVendors: Set<String> {
        Set(sessions.filter { isSessionLive($0) }.compactMap { sessionSource($0).vendor })
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

    /// The end of the data on screen: the report's time, or the latest poll that confirmed nothing changed.
    public var dataDate: Date { max(report?.generatedAt ?? now, checkedAt ?? .distantPast) }
    public var statsInterval: DateInterval { statsRange.interval(endingAt: dataDate) }

    /// Include sessions active during the selected range, including ones that started before it.
    public var statsSessions: [LiveSession] {
        let interval = statsInterval
        return sessions.filter { $0.startedAt <= interval.end && ($0.endedAt ?? now) >= interval.start }
    }

    public func liveStatusEnabled(for session: LiveSession) -> Bool {
        settings.settings.liveStatusEnabled(for: sessionSource(session).vendor ?? "")
    }

    /// Running, including a turn blocked on the user: both are work in flight, and the panel tells them apart by colour.
    public func isSessionLive(_ session: LiveSession) -> Bool {
        session.isLive(at: now) && liveStatusEnabled(for: session)
    }

    /// What the newest turn of this session is doing, when its source reported one.
    public func sessionState(_ session: LiveSession) -> SessionTurn.State? {
        let vendor = sessionSource(session).vendor?.lowercased()
        return report?.turns.last {
            $0.sessionID == session.id && (vendor == nil || $0.provider.lowercased() == vendor)
        }?.state
    }

    public func isSessionWaiting(_ session: LiveSession) -> Bool {
        isSessionLive(session) && sessionState(session) == .waitingForApproval
    }

    public func sessionStatusLabel(_ session: LiveSession) -> String {
        guard liveStatusEnabled(for: session) else { return L10n.text("状态显示已关闭", "Live status off") }
        if session.isLive && !session.isLive(at: now) { return L10n.text("状态待更新", "Status out of date") }
        if isSessionWaiting(session) { return L10n.text("等待批准", "Needs approval") }
        return Countdown.sessionLabel(session, now: now)
    }

    public var liveSessions: [LiveSession] { sessions.filter(isSessionLive) }

    /// The focused session while this Mac still reports it.
    public var focusedSession: LiveSession? {
        focusedSessionID.flatMap { id in sessions.first { $0.id == id } }
    }

    public func sessionUsage(_ session: LiveSession) -> SessionUsage? { report?.sessionUsage?[session.id] }

    /// What the agent last said in the session's newest turn that carries a message.
    public func sessionMessage(_ session: LiveSession) -> String? {
        let vendor = sessionSource(session).vendor?.lowercased()
        return report?.turns.last {
            $0.sessionID == session.id && $0.message != nil && (vendor == nil || $0.provider.lowercased() == vendor)
        }?.message
    }

    /// The session's tokens on a time axis from its first period to its last activity, with one stack per model.
    public func sessionColumns(_ session: LiveSession, usage: SessionUsage) -> (columns: [TokenColumn], interval: DateInterval, agentIds: [String]) {
        let agentIds = usage.models.map(\.agentId)
        let start = min(session.startedAt, usage.periods.first?.start ?? session.startedAt)
        let end = max(usage.periods.last.map { $0.start.addingTimeInterval(UsageBucket.duration) } ?? start, session.endedAt ?? dataDate)
        let interval = DateInterval(start: start, end: max(end, start.addingTimeInterval(UsageBucket.duration)))
        let columns = ChartData.tokenBars(usage: usage.periods.map(\.bucket), agentIds: agentIds, interval: interval,
                                          bucketSize: ChartData.bucketSize(spanning: interval.duration), dimensions: tokenDimensions)
        return (columns, interval, agentIds)
    }

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

    /// - screen: which display's glow to resolve; nothing asks for the default one.
    public func glowAppearance(light: Bool, on screen: String? = nil) -> GlowAppearance {
        let appearance = GlowAppearance.resolve(
            levels: levels,
            paused: isPaused || !isAccessAllowed,
            anyAgentActive: hasLiveSession,
            glow: settings.settings.glow(on: screen),
            light: light
        )
        guard glowHidden else { return appearance }
        return GlowAppearance(
            stops: appearance.stops, peakOpacity: appearance.peakOpacity, troughOpacity: appearance.troughOpacity,
            breathing: appearance.breathing, breathSeconds: appearance.breathSeconds, hidden: true
        )
    }

    // MARK: Consumers (token spenders, e.g. model families)

    /// Token spend is independent of which remaining-quota windows the user monitors.
    public var consumers: [AgentDescriptor] { report?.consumers ?? [] }

    /// Both surfaces show every model's token spend.
    public var tokenColumns: [TokenColumn] {
        ChartData.tokenBars(usage: report?.usage ?? [], agentIds: consumers.map(\.id), range: statsRange, bucketSize: tokenBucketSize,
                            now: dataDate, dimensions: tokenDimensions)
    }

    public var statsActivity: ActivityGrid {
        UsageAnalytics.activityGrid(usage: (report?.usage ?? []).filter { $0.start < dataDate },
            since: dataDate.addingTimeInterval(-7 * 86400), calendar: .current, dimensions: tokenDimensions)
    }

    public var weeklyTokenShare: [String: Double] {
        var totals: [String: Int] = [:]
        let week = DateInterval(start: dataDate.addingTimeInterval(-7 * 86400), end: dataDate)
        for bucket in report?.usage ?? [] where bucket.overlaps(week) {
            totals[bucket.agentId, default: 0] += tokenDimensions.count(bucket)
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

    /// A vendor group's rows split by account, in row order. One section without an account when nothing is identified.
    public func accountSections(_ rows: [AgentRow]) -> [AccountSection] {
        var order: [String] = []
        var sections: [String: [AgentRow]] = [:]
        for row in rows {
            let key = row.agent.account?.id ?? ""
            if sections[key] == nil { order.append(key) }
            sections[key, default: []].append(row)
        }
        return order.map { key in
            let rows = sections[key] ?? []
            return AccountSection(id: key, account: rows.first?.account, isCurrent: rows.first?.isCurrentAccount ?? true, rows: rows)
        }
    }
}
