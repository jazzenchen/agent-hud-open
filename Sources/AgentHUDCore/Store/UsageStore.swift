import Foundation
import Observation

/// Keeps a change handler registered with `UsageStore.observeChanges(_:)`; releasing it unregisters the handler.
public final class UsageChangeObservation {
    private var cancellation: (() -> Void)?
    init(_ cancellation: @escaping () -> Void) { self.cancellation = cancellation }
    public func cancel() { cancellation?(); cancellation = nil }
    deinit { cancellation?() }
}

/// Observable app state: the collected report, derived rows, glow appearance and stats selections.
@MainActor
@Observable
public final class UsageStore {
    public internal(set) var report: UsageReport? { didSet { reportGeneration += 1 } }
    public internal(set) var lastError: String?
    public internal(set) var pausedUntil: Date?
    public internal(set) var isRefreshing = false
    public internal(set) var statsRange: StatsRange = .hours24
    public var tokenBucketSize: TokenBucketSize = .hour1
    public var tokenDimensions: TokenDimensions = .fresh
    /// Explicitly picked token cards; nil shows recorded clients and clients connected to monitored services.
    public var pickedAgents: Set<String>?
    /// Keep every selectable range ready, including the partial hour at the start of the rolling window.
    public static var historyHours: Int { StatsRange.days7.hours + 1 }
    /// The statistics window's page. Pointing out a quota window turns to Tokens, focusing a session to Sessions.
    public var statsTab: StatsTab = .tokens
    /// The quota window the statistics window points out, set by whatever opened it.
    public var selectedQuotaId: String? { didSet { if selectedQuotaId != nil { statsTab = .tokens } } }
    /// The session the Sessions page shows in place of its list; nil shows the list.
    public var focusedSessionID: String? {
        didSet {
            if focusedSessionID != nil { statsTab = .sessions }
            if focusedSessionID != oldValue { focusedTurn = nil }
        }
    }
    /// The focused session's turn whose calls its page lays out, by its place in the session's breakdown.
    public var focusedTurn: Int? { didSet { if focusedTurn != oldValue { focusedTurnCalls = nil } } }
    /// The focused turn's calls once read, with the tools their logs name.
    public internal(set) var focusedTurnCalls: [TurnCall]?
    /// Where turns' calls are read from: the ledger a host's providers write.
    @ObservationIgnored public var ledger: UsageLedger?
    /// The calls a turn shows where there is no ledger to read them from, such as the demo's
    /// (`DemoData.turnCalls(session:turn:)`); without either, a turn shows none.
    @ObservationIgnored public var sampleTurnCalls: (@MainActor (LiveSession, SessionUsage.Turn) -> [TurnCall])?
    public var glowHidden = false
    /// Advances every few seconds so countdowns re-render.
    public internal(set) var now = Date()
    /// The turns clients' prompt and Stop hooks saw, by session id, which a host that receives those hooks hands in. A hook
    /// turn takes the place of what a session's log says wherever the hooks saw more
    /// (`SessionPhase.hookPrevails(_:over:lastEventAt:)`), so the panel follows a Stop hook at once.
    public var hookTurns: [String: SessionPhase.HookTurn] = [:]

    public let settings: SettingsStore
    private let accessAllowed: () -> Bool
    private let collector: UsageCollector
    /// A later poll that found nothing changed extends the report's coverage.
    var checkedAt: Date?
    @ObservationIgnored private var changeObservers: [UUID: @MainActor (UsageChanges) -> Void] = [:]
    /// Counts the reports shown, so that a new one is told from the last without comparing them.
    @ObservationIgnored private var reportGeneration = 0
    /// The last view built, and what it was built from.
    @ObservationIgnored private var built: (generation: Int, agents: [AgentDescriptor], settings: Settings, approvals: [String],
                                            hookTurns: [String: SessionPhase.HookTurn], failure: String?, view: ReportView)?

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
        collector.start()
    }

    public func stop() {
        collector.stop()
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

    // MARK: Derived

    /// What the Mac shows of the report now: its rows, balances, levels and sessions, with the sessions whose clients wait
    /// for an answer to a permission request (`PermissionRequests.shared`) waiting for approval and the hooks' turns in
    /// their place, and every reading failed while the last pass failed to read any source. It is built again only when
    /// the report, the agent list, the settings, the waiting requests, the hooks' turns, a failed pass or the time changed,
    /// and reading it tracks all seven.
    public var view: ReportView {
        let report = self.report, agents = settings.agents, preferences = settings.settings, now = self.now, failure = lastError
        let approvals = PermissionRequests.shared.pending, requests = approvals.map(\.id), hookTurns = self.hookTurns
        if let built, built.generation == reportGeneration, built.view.now == now, built.agents == agents,
           built.settings == preferences, built.approvals == requests, built.hookTurns == hookTurns, built.failure == failure {
            return built.view
        }
        let view = ReportView(report: report, agents: agents, settings: preferences, approvals: approvals, hookTurns: hookTurns, now: now,
                              failure: failure)
        built = (reportGeneration, agents, preferences, requests, hookTurns, failure, view)
        return view
    }

    /// `view.visibleAgents`.
    public var visibleAgents: [AgentDescriptor] { view.visibleAgents }

    /// `view.enabledAgents`.
    public var enabledAgents: [AgentDescriptor] { view.enabledAgents }

    /// `view.rows`.
    public var rows: [AgentRow] { view.rows }

    public func row(for agentId: String) -> AgentRow? { view.rows.first { $0.id == agentId } }

    /// `view.forecastHint(for:)`.
    public func quotaForecastHint(for agentId: String) -> String? { view.forecastHint(for: agentId) }

    /// `view.tokensPerHour(for:)`.
    public func quotaTokensPerHour(for agentId: String) -> Double? { view.tokensPerHour(for: agentId) }

    /// `view.billing`.
    public var enabledBilling: [APIBilling] { view.billing }

    /// One of an account's balances' level, which follows `view.level(of:)`: none while the account's reading shows none.
    public func balanceLevel(_ balance: AccountBalance, billing: APIBilling) -> StatusLevel? {
        guard !balance.total.isNaN, view.assessment(of: billing).showsLevel else { return nil }
        return AlertPolicy.balanceLevel([balance], isAvailable: billing.isAvailable)
    }

    /// `view.levels`.
    public var levels: [StatusLevel] { view.levels }

    /// `view.alertPulseVendors`.
    public var alertPulseVendors: [String] { view.alertPulseVendors }

    public var isIndexing: Bool { report?.indexing != nil }

    /// Includes the brief interval before the first refresh starts; a failed fetch ends loading.
    public var isLoading: Bool { isAccessAllowed && report == nil && !isPaused && (isRefreshing || lastError == nil) }

    /// `view.maxUsedPct`.
    public var maxUsedPct: Double? { view.maxUsedPct }

    /// `view.sessions`, newest first.
    public var sessions: [LiveSession] { view.sessions.map(\.session) }

    /// How long after its last event a vendor still belongs in a logo queue.
    public static let queueRecency: TimeInterval = ReportView.queueRecency

    /// `view.queueVendors`.
    public var queueVendors: [(vendor: String, isWorking: Bool)] { view.queueVendors }

    /// `view.workingVendors`.
    public var workingVendors: Set<String> { view.workingVendors }

    /// The session's source as the view reads it.
    public func sessionSource(_ session: LiveSession) -> SessionSource { view.session(for: session).source }

    /// `view.subscriptions`.
    public var subscriptions: [String: String] { view.subscriptions }

    /// The end of the data on screen: the report's time, or the latest poll that confirmed nothing changed.
    public var dataDate: Date { max(report?.generatedAt ?? now, checkedAt ?? .distantPast) }


    /// Whether live status is on for the session's vendor.
    public func liveStatusEnabled(for session: LiveSession) -> Bool { view.session(for: session).liveStatus }

    /// Running, including a turn blocked on the user: both are work in flight, and the panel tells them apart by colour.
    public func isSessionLive(_ session: LiveSession) -> Bool { view.phase(of: session).isInFlight }

    /// What the newest turn of this session is doing, when its source reported one.
    public func sessionState(_ session: LiveSession) -> SessionTurn.State? { view.session(for: session).turn?.state }

    public func isSessionWaiting(_ session: LiveSession) -> Bool { view.phase(of: session).state == .waitingForApproval }

    public func sessionStatusLabel(_ session: LiveSession) -> String {
        let shown = view.session(for: session)
        guard shown.liveStatus else { return L10n.text("状态显示已关闭", "Live status off") }
        switch shown.phase.state {
        case .unverified: return L10n.text("状态待更新", "Status out of date")
        case .waitingForApproval: return L10n.text("等待批准", "Needs approval")
        case .running, .idle: return Countdown.sessionLabel(shown.phase, now: now)
        }
    }

    /// `view.liveSessions`.
    public var liveSessions: [LiveSession] { view.liveSessions.map(\.session) }

    /// What the agent last said in the session's newest turn that carries a message; nothing while live status is off.
    public func sessionMessage(_ session: LiveSession) -> String? { view.session(for: session).message }

    public var hasLiveSession: Bool { view.sessions.contains { $0.phase.isInFlight } }

    public var updatedAt: Date? { report?.generatedAt }

    /// `view.rowGroups`.
    public var rowGroups: [(vendor: String, rows: [AgentRow])] { view.rowGroups }

    /// `view.accountSections(_:)`.
    public func accountSections(_ rows: [AgentRow]) -> [AccountSection] { view.accountSections(rows) }

    /// `view.accountNotice(for:)`.
    public func accountNotice(for section: AccountSection) -> String? { view.accountNotice(for: section) }

    /// `view.assessment(of:)` put into its header label.
    public func accountLabel(for account: AccountObservation) -> String { view.assessment(of: account).accountLabel(now: now) }
}
