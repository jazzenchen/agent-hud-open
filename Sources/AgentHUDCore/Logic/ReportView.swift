import AgentHUDSupport
import Foundation

/// What the Mac shows of a report at one time: the quota rows it lists and how they group, the balances, the glow's
/// levels, and each session with its source, newest turn, last event, message and phase. Rows, balances, levels and
/// sessions are worked out when the view is built, a window's metrics only when asked for. A session's phase comes from
/// the report, the turns its client's hooks saw and the permission requests waiting for an answer.
public struct ReportView: Sendable {
    /// One session as the Mac shows it.
    public struct Session: Hashable, Sendable, Identifiable {
        public let session: LiveSession
        /// The session's vendor, from the consumer its model is, a settings row, a report row or its id, and its client.
        public let source: SessionSource
        /// The newest turn from the session's vendor, or from any provider without a vendor: the one that started last, a
        /// turn without a start counting from when it was observed, then the one observed last, the later listed of equals.
        public let turn: SessionTurn?
        /// When the session last did something: its newest turn event from any provider, or its end when that is later;
        /// without turns, its end, or while in flight when its log last recorded anything, else the reading that last saw
        /// it. A permission request waiting for an answer is an event too, and so are the prompt and the Stop hook of a
        /// hook turn that takes the reading's place.
        public let lastEventAt: Date
        /// What the agent last said: the message of the newest turn that carries one, from the providers `turn` comes
        /// from, or what it said at the Stop hook of a hook turn that takes the reading's place. Nil while live status is
        /// off.
        public let message: String?
        /// Whether live status is on for the session's execution agent; a session without one answers to no switch.
        public let liveStatus: Bool
        public let phase: SessionPhase

        public var id: String { session.id }
    }

    public let now: Date
    /// The rows present, as `isPresent(_:in:)` decides, in the settings' order. Settings keep the switch and place of a
    /// row that stops being present, so it comes back as it was; until then it is not shown anywhere.
    public let visibleAgents: [AgentDescriptor]
    /// The visible rows switched on.
    public let enabledAgents: [AgentDescriptor]
    /// A quota row for each enabled row of a shown account that is not billed through an API account.
    public let rows: [AgentRow]
    /// Rows grouped by displayed vendor, in row order.
    public let rowGroups: [(vendor: String, rows: [AgentRow])]
    /// Account cards follow the agent switches: a billing pool's card its pool's rows, any other its vendor's.
    public let billing: [APIBilling]
    /// One part of the glow: a quota row's or an API account's level, and the vendor whose alerts light it.
    public struct GlowSegment: Hashable, Sendable {
        public let vendor: String
        public let level: StatusLevel
    }

    /// The glow's parts in glow order: each enabled row whose reading shows a level, each balance once. The glow's colours
    /// and an alert's pulse both come from this one list.
    public let glowSegments: [GlowSegment]
    /// Newest first, by the last event each source reported: a prompt, a reply, a tool result or an approval request. A
    /// running session nothing has been heard from for half an hour sits below one that just answered. Sessions with the
    /// same last event are ordered by vendor, then id.
    public let sessions: [Session]

    private let report: UsageReport?
    private let settings: Settings
    private let index: Index
    /// Why the last pass failed when every source failed to read; every reading then counts as failed.
    private let failure: String?

    /// - agents: the settings' rows, in their order.
    /// - approvals: the permission requests waiting for an answer. A session whose client waits for one is waiting for
    ///   approval, however its source reads, for as long as the client waits.
    /// - hookTurns: the turns clients' prompt and Stop hooks saw, by session id. A hook turn takes the place of the phase
    ///   the report gives wherever the hooks saw more (`SessionPhase.hookPrevails(_:over:lastEventAt:)`), unless it is out
    ///   of date while the report has the session in flight.
    /// - failure: the error of the last pass when every source failed to read it.
    public init(report: UsageReport?, agents: [AgentDescriptor], settings: Settings, approvals: [PermissionRequest] = [],
                hookTurns: [String: SessionPhase.HookTurn] = [:], now: Date, failure: String? = nil) {
        let visible = agents.filter { Self.isPresent($0, in: report) }
        let enabled = visible.filter(\.enabled)
        let shown = enabled.filter { settings.accountVisible($0.displayAccountID) }
        let rows = enabled.filter { !$0.isAPIBilled }.enumerated()
            .filter { settings.accountVisible($0.element.displayAccountID) }.map { index, agent in
            let snapshot = report?.snapshot(for: agent.id)
            // Without a report, a row has no reading and counts as its account's current one.
            let reading = report?.assess(.window(agent), now: now)
                ?? ReadingAssessment(status: .normal, isCurrentAccount: true, observedAt: nil, now: now)
            let assessment = failure.map(reading.failing) ?? reading
            return AgentRow(
                agent: agent,
                remainingPct: snapshot?.remainingPct,
                level: assessment.showsLevel ? snapshot.flatMap(\.remainingPct).map { AlertPolicy.quotaLevel(remaining: $0) } : nil,
                resetAt: snapshot?.resetAt,
                weeklyRemainingPct: snapshot?.weeklyRemainingPct,
                paletteIndex: index,
                account: agent.account.flatMap { report?.observation(accountID: $0.id) },
                assessment: assessment
            )
        }
        let vendors = Set(shown.map(\.vendor))
        let billing = (report?.billing ?? []).filter { billing in
            settings.accountVisible(billing.id)
                && (billing.billingPool.map { pool in shown.contains { $0.billingPool?.id == pool.id } } ?? vendors.contains(billing.vendor))
        }
        let quota = Dictionary(uniqueKeysWithValues: rows.compactMap { row in row.level.map { (row.id, $0) } })
        var seenAccounts: Set<String> = []
        let segments = shown.flatMap { model -> [GlowSegment] in
            if !model.isAPIBilled { return quota[model.id].map { [GlowSegment(vendor: model.vendor, level: $0)] } ?? [] }
            return billing.filter { $0.contains(model) && seenAccounts.insert($0.id).inserted }.compactMap {
                Self.level(of: $0, assessment: Self.assessment(of: $0, in: report, failure: failure, now: now))
                    .map { GlowSegment(vendor: model.vendor, level: $0) }
            }
        }
        var order: [String] = []
        var groups: [String: [AgentRow]] = [:]
        for row in rows {
            let vendor = row.agent.displayVendor
            if groups[vendor] == nil { order.append(vendor) }
            groups[vendor, default: []].append(row)
        }
        let index = Index(report: report, agents: agents, approvals: approvals, hookTurns: hookTurns)
        self.failure = failure
        self.report = report
        self.settings = settings
        self.index = index
        self.now = now
        visibleAgents = visible
        enabledAgents = enabled
        self.rows = rows
        rowGroups = order.map { ($0, groups[$0] ?? []) }
        self.billing = billing
        glowSegments = segments
        sessions = (report?.sessions ?? []).filter { !$0.isBillingOnlyGrokBotSubagent }
            .map { Self.read($0, index: index, settings: settings, now: now) }.sorted {
            if $0.lastEventAt != $1.lastEventAt { return $0.lastEventAt > $1.lastEventAt }
            let lhs = $0.source.vendor ?? "", rhs = $1.source.vendor ?? ""
            return lhs == rhs ? $0.id < $1.id : lhs < rhs
        }
    }

    // MARK: Quota

    /// Whether a row is present, the one rule of the panel, the menu, Settings and the report kept across passes: a provider
    /// reported it within the retention period, its account is in its provider's inventory, and its plan pool is active.
    /// What a report does not say counts as present: rows it keeps no sighting times for, a provider without an inventory, a
    /// pool its provider's inventory does not cover; without a report every row is. A row without an account is no longer
    /// present once its provider identifies accounts.
    public static func isPresent(_ agent: AgentDescriptor, in report: UsageReport?) -> Bool {
        guard let report else { return true }
        return isPresent(agent, seenAt: report.rowSeenAt, accounts: report.accounts, activePools: report.activeQuotaPoolIDs)
    }

    /// `isPresent(_:in:)` over the sighting times, the account inventory and the active plan pools themselves.
    static func isPresent(_ agent: AgentDescriptor, seenAt: [String: Date]?, accounts: [String: [AccountObservation]]?,
                          activePools: [String: Set<String>]?) -> Bool {
        if let seenAt, seenAt[agent.id] == nil { return false }
        if let pool = agent.billingPool {
            guard pool.product == .plan, let active = activePools?[pool.provider] else { return true }
            return active.contains(pool.id)
        }
        guard let inventory = accounts?[agent.account?.provider ?? agent.vendor] else { return true }
        guard let account = agent.account else { return inventory.isEmpty }
        return inventory.contains { $0.account.id == account.id }
    }

    /// Each glow segment's level, in glow order.
    public var levels: [StatusLevel] { glowSegments.map(\.level) }

    /// Each glow segment's vendor, in glow order. An alert's pulse lights the part of the glow its vendor's segments take,
    /// the same part their colours take.
    public var alertPulseVendors: [String] { glowSegments.map(\.vendor) }

    /// The most consumed window among those whose readings show a level, shown in the menu bar.
    public var maxUsedPct: Double? { rows.filter(\.assessment.showsLevel).compactMap(\.usedPct).max() }

    /// Whether a window's reading enters calculations: only the reading of a row that shows a level does. Any other is
    /// shown greyed, without a forecast.
    private func counts(_ agentId: String) -> Bool { rows.contains { $0.id == agentId && $0.assessment.showsLevel } }

    /// Plans of the vendors that have an enabled row.
    public var subscriptions: [String: String] {
        let vendors = Set(enabledAgents.map(\.vendor))
        return (report?.subscriptions ?? [:]).filter { vendors.contains($0.key) }
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

    /// A balance's reading as its card and the menu weigh it at the view's time.
    public func assessment(of billing: APIBilling) -> ReadingAssessment {
        Self.assessment(of: billing, in: report, failure: failure, now: now)
    }

    /// A balance's level, which follows the rules of a quota window's: none while its reading shows none, such as after a
    /// failed read, else the lowest rung of its balances, critical while its service marks the account unavailable.
    public func level(of billing: APIBilling) -> StatusLevel? { Self.level(of: billing, assessment: assessment(of: billing)) }

    private static func assessment(of billing: APIBilling, in report: UsageReport?, failure: String?, now: Date) -> ReadingAssessment {
        let reading = report?.assess(.balance(billing), now: now)
            ?? ReadingAssessment(status: billing.ownStatus, isCurrentAccount: true, observedAt: billing.updatedAt, now: now)
        return failure.map(reading.failing) ?? reading
    }

    private static func level(of billing: APIBilling, assessment: ReadingAssessment) -> StatusLevel? {
        assessment.showsLevel ? AlertPolicy.balanceLevel(billing.balances, isAvailable: billing.isAvailable) : nil
    }

    /// An account's reading as its section header weighs it at the view's time.
    public func assessment(of account: AccountObservation) -> ReadingAssessment {
        let reading = report?.assess(.account(account), now: now)
            ?? ReadingAssessment(status: account.ownStatus, isCurrentAccount: account.isCurrent, observedAt: account.observedAt, now: now)
        return failure.map(reading.failing) ?? reading
    }

    /// What the header of an account's section says about its readings, on the island and in the menu alike: the reason
    /// of the account's status, then its client's notices, which also name what holds nothing back, such as a notice about
    /// local logs or hooks. A vendor's notices belong to its current accounts; retained accounts still show notices
    /// scoped to their client home. A client's notices that already carry the reason do not repeat it. A billing pool
    /// speaks only for itself: its vendor's notices can be about another of its pools.
    public func accountNotice(for section: AccountSection) -> String? {
        guard let account = section.account else { return nil }
        let reason = assessment(of: account).status.reason
        let client = accountSourceNotice(for: section)
        var parts: [String] = []
        if let reason, !(client?.contains(reason) ?? false) { parts.append(reason) }
        if let client { parts.append(client) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Source details are independent of the account's failed quota reading and can be displayed with their own style.
    public func accountSourceNotice(for section: AccountSection) -> String? {
        guard let account = section.account, !account.account.isBillingPool else { return nil }
        return report?.sourceNotices[ClientHome.sourceKey(provider: account.account.provider, home: account.home)]
            ?? (account.isCurrent ? report?.sourceNotices[account.account.provider] : nil)
    }

    /// Tokens per hour over the observed part of this quota window's current cycle. Token history before the local ledger
    /// begins is not guessed at.
    public func tokensPerHour(for agentId: String) -> Double? {
        guard counts(agentId), let report, let snapshot = report.snapshot(for: agentId),
              let consumers = report.consumerIdsByQuota[agentId] else { return nil }
        return QuotaMath.tokensPerHour(snapshot: snapshot, consumers: consumers, usage: report.usage, now: now)
    }

    /// What the window's reading and recent pace say about the rest of its cycle.
    public func outlook(for agentId: String) -> QuotaOutlook? {
        guard counts(agentId), let report, let snapshot = report.snapshot(for: agentId) else { return nil }
        return QuotaMath.outlook(snapshot: snapshot, insights: report.insightsByAgent[agentId], now: now)
    }

    /// The window's outlook in words.
    public func forecastHint(for agentId: String) -> String? {
        guard counts(agentId), let report, let snapshot = report.snapshot(for: agentId) else { return nil }
        return QuotaForecast.hint(snapshot: snapshot, insights: report.insightsByAgent[agentId], now: now)
    }

    // MARK: Sessions

    /// The session with this id, including agents below the listed root sessions.
    public func session(_ id: String) -> Session? {
        if let root = sessions.first(where: { $0.id == id }) { return root }
        return sessions.lazy.flatMap { $0.session.descendantSessions }.first { $0.id == id }.map { session(for: $0) }
    }

    /// The session that directly started an agent; nil for a root session or one no longer reported.
    public func parentSession(of id: String) -> LiveSession? {
        sessions.lazy.flatMap { [$0.session] + $0.session.descendantSessions }.first {
            $0.subagentSessions?.contains { $0.id == id } == true
        }
    }

    /// A session as this view reads it, one that is not among `sessions` included, such as a copy from an earlier report.
    public func session(for session: LiveSession) -> Session {
        Self.read(session, index: index, settings: settings, now: now)
    }

    /// The phase of a session, one that is not among `sessions` included.
    public func phase(of session: LiveSession) -> SessionPhase { self.session(for: session).phase }

    /// The sessions in flight, newest first.
    public var liveSessions: [Session] { sessions.filter(\.phase.isInFlight) }

    /// How long after its last event a session stays on the Sessions page.
    static let listRecency: TimeInterval = 7 * 86400

    /// The sessions whose last event is at most `listRecency` old, newest first: those the Sessions page lists.
    public var recentSessions: [Session] {
        let cutoff = now.addingTimeInterval(-Self.listRecency)
        return sessions.filter { $0.lastEventAt >= cutoff }
    }

    /// The vendors with work in flight. A session names the model it spends rather than the quota row it belongs to, so
    /// its vendor is resolved instead of its id being compared with a row's.
    public var workingVendors: Set<String> { Set(liveSessions.compactMap(\.source.vendor)) }

    /// How long after its last event a vendor still belongs in a logo queue.
    static let queueRecency: TimeInterval = 24 * 3600

    /// What a logo queue shows: every row's vendor, in row order, then any agent with a session whose last event is at
    /// most `queueRecency` old and has no row on that list, most recently used first. An agent used this morning belongs
    /// in the queue whether or not its quota is followed; one nobody has run for a day and nobody watches does not. A
    /// vendor whose live status is off is not counted as having run, since that switch is what says its runs may be
    /// reported at all. Grok's shared account rows use its recent execution clients' marks, or an idle account mark
    /// when there is no client to show; each client's own sessions decide whether its mark is working.
    public var queueVendors: [(vendor: String, isWorking: Bool)] {
        let working = Set(liveSessions.compactMap(\.source.agentVendor))
        let cutoff = now.addingTimeInterval(-Self.queueRecency)
        let recent = sessions.filter { $0.liveStatus && $0.lastEventAt >= cutoff }
        var grokSeen = Set<String>()
        let grokAgents = recent.compactMap { session -> String? in
            guard session.source.vendor == "Grok", let agent = session.source.agentVendor,
                  grokSeen.insert(agent).inserted else { return nil }
            return agent
        }
        var order: [String] = []
        var grokAdded = false
        for row in rows {
            if row.agent.vendor == "Grok" {
                guard !grokAdded else { continue }
                grokAdded = true
                order += grokAgents.isEmpty ? ["Grok"] : grokAgents
            } else {
                order.append(row.agent.vendor)
            }
        }
        var seen = Set(order)
        for session in recent {
            guard let vendor = session.source.agentVendor, seen.insert(vendor).inserted else { continue }
            order.append(vendor)
        }
        return order.map { (vendor: $0, isWorking: working.contains($0)) }
    }

    /// What sessions are read against: the vendor of each agent id, the report's turns and newest turn event by session,
    /// when each session's client last asked for approval among the requests still waiting, and the hooks' turns.
    private struct Index: Sendable {
        var vendors: [String: String] = [:]
        var turns: [String: [SessionTurn]] = [:]
        var lastTurnEvents: [String: Date] = [:]
        var requests: [String: Date] = [:]
        let hooks: [String: SessionPhase.HookTurn]

        /// A vendor comes from the consumers, then the settings rows, then the report's own rows.
        init(report: UsageReport?, agents: [AgentDescriptor], approvals: [PermissionRequest], hookTurns: [String: SessionPhase.HookTurn]) {
            hooks = hookTurns
            for agent in (report?.consumers ?? []) + agents + (report?.discoveredAgents ?? []) where vendors[agent.id] == nil {
                vendors[agent.id] = agent.vendor
            }
            for turn in report?.turns ?? [] {
                turns[turn.sessionID, default: []].append(turn)
                let at = RecordCoding.date(turn.observedAtMs)
                if at > lastTurnEvents[turn.sessionID] ?? .distantPast { lastTurnEvents[turn.sessionID] = at }
            }
            for request in approvals where request.at > requests[request.sessionID] ?? .distantPast {
                requests[request.sessionID] = request.at
            }
        }
    }

    /// The newest of a session's turns: the one that started last, a turn without a start counting from when it was
    /// observed, then the one observed last; of equals, the later listed.
    static func newest(_ turns: [SessionTurn]) -> SessionTurn? {
        turns.reduce(nil) { newest, turn in
            guard let newest else { return turn }
            return (turn.startedAtMs ?? turn.observedAtMs, turn.observedAtMs) >= (newest.startedAtMs ?? newest.observedAtMs, newest.observedAtMs)
                ? turn : newest
        }
    }

    private static func read(_ session: LiveSession, index: Index, settings: Settings, now: Date) -> Session {
        let vendor = index.vendors[session.agentId] ?? SessionSource.vendor(impliedBy: session.agentId)
        let source = SessionSource(vendor: vendor, client: session.client)
        let provider = source.vendor?.lowercased()
        let turns = (index.turns[session.id] ?? []).filter { provider == nil || $0.provider.lowercased() == provider }
        let turn = newest(turns)
        let asked = index.requests[session.id]
        var lastEventAt = max(session.lastEvent(turnAt: index.lastTurnEvents[session.id]), asked ?? .distantPast)
        let liveStatus = settings.liveStatusEnabled(for: source.agentVendor ?? "")
        var phase = SessionPhase(session: session, turn: turn, lastEventAt: lastEventAt, liveStatus: liveStatus, now: now)
        var message = newest(turns.filter { $0.message != nil })?.message
        if liveStatus, let hook = index.hooks[session.id],
           let hooked = SessionPhase.hooked(hook, over: phase, lastEventAt: lastEventAt, now: now) {
            phase = hooked
            lastEventAt = max(lastEventAt, hook.endedAt ?? hook.startedAt)
            message = hook.message ?? message
        }
        if liveStatus, asked != nil { phase = phase.awaitingApproval(session, turn: turn) }
        return Session(session: session, source: source, turn: turn,
                       lastEventAt: lastEventAt, message: liveStatus ? message : nil, liveStatus: liveStatus, phase: phase)
    }
}
