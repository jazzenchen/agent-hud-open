import Foundation

/// The statistics window's selections and what they chart: the range, the page and its focus, token columns, usage by
/// agent and list prices.
@MainActor
public extension UsageStore {
    // MARK: Selections

    func setStatsRange(_ range: StatsRange) {
        guard range != statsRange else { return }
        statsRange = range
        if !range.bucketSizes.contains(tokenBucketSize) { tokenBucketSize = range.bucketSizes.last ?? .day1 }
    }

    // MARK: Range and sessions

    var statsInterval: DateInterval { statsRange.interval(endingAt: dataDate) }

    /// `view.recentSessions`: the sessions whose last event is at most seven days old, whatever range the charts show.
    var statsSessions: [LiveSession] { view.recentSessions.map(\.session) }

    /// The focused session while this Mac still reports it.
    var focusedSession: LiveSession? { focusedSessionID.flatMap { view.session($0)?.session } }

    func sessionUsage(_ session: LiveSession) -> SessionUsage? { report?.sessionUsage?[session.id] }

    /// Reads the focused turn's calls from the ledger, and the tools they asked for from their logs, once per focus.
    func loadFocusedTurnCalls() async {
        guard focusedTurnCalls == nil, let session = focusedSession, let index = focusedTurn,
              let turns = sessionUsage(session)?.turns, turns.indices.contains(index) else { return }
        let turn = turns[index]
        let calls: [TurnCall]
        if let ledger {
            let read = (try? await ledger.turnCalls(SessionUsageRequest(session), from: turn.start, through: turn.end)) ?? []
            calls = await Task.detached(priority: .userInitiated) { CallTools.attach(to: read) }.value
        } else {
            calls = sampleTurnCalls?(session, turn) ?? []
        }
        // The focus may have moved while the calls were read.
        guard focusedSession?.id == session.id, focusedTurn == index else { return }
        focusedTurnCalls = calls
    }

    /// Every token the session and its sub-agents spent, by kind: its breakdown, or its log's counts, which do not split
    /// cache writes and reasoning apart.
    func sessionTokens(_ session: LiveSession) -> TokenKinds {
        sessionUsage(session)?.total.kinds
            ?? TokenKinds(tokensIn: session.tokensIn, tokensOut: session.tokensOut, cacheRead: session.cacheReadTokens)
    }

    /// Sessions under the local day they were last active on, in the order given; the newest day first. A session in
    /// flight is active now, so it sits under today however long ago its last event was, and a session moves to today
    /// as soon as it does something again.
    func sessionsByDay(_ sessions: [LiveSession], calendar: Calendar = .current) -> [(day: Date, sessions: [LiveSession])] {
        let view = self.view, now = self.now
        var days: [(day: Date, sessions: [LiveSession])] = []
        for session in sessions {
            let shown = view.session(for: session)
            let day = calendar.startOfDay(for: shown.phase.isInFlight ? now : min(shown.lastEventAt, now))
            if let index = days.firstIndex(where: { $0.day == day }) { days[index].sessions.append(session) }
            else { days.append((day, [session])) }
        }
        return days.sorted { $0.day > $1.day }
    }

    // MARK: Consumers (token spenders, e.g. model families)

    /// Token spend is independent of which remaining-quota windows the user monitors.
    var consumers: [AgentDescriptor] { report?.consumers ?? [] }

    /// Both surfaces show every model's token spend.
    var tokenColumns: [TokenColumn] {
        ChartData.tokenBars(usage: report?.usage ?? [], agentIds: consumers.map(\.id), range: statsRange, bucketSize: tokenBucketSize,
                            now: dataDate, dimensions: tokenDimensions)
    }

    var statsActivity: ActivityGrid {
        UsageAnalytics.activityGrid(usage: (report?.usage ?? []).filter { $0.start < dataDate },
            since: dataDate.addingTimeInterval(-7 * 86400), calendar: .current, dimensions: tokenDimensions)
    }

    /// The platform each model's calls are priced on: the one its client reaches.
    var priceRegions: PriceRegions { PriceRegions(report: report) }

    /// What each agent spent in the charted range, the most tokens of the selected kinds first.
    var agentUsage: [AgentUsage] {
        AgentUsage.build(usage: report?.usage ?? [], consumers: consumers, sessions: sessions, breakdowns: report?.sessionUsage ?? [:],
                         vendor: { self.sessionSource($0).vendor }, interval: statsInterval, dimensions: tokenDimensions,
                         region: priceRegions.region(for:))
    }

    /// An agent's tokens of the selected kinds in each of the chart's buckets.
    func agentSeries(_ vendor: String) -> [Int] {
        ChartData.tokenBars(usage: report?.usage ?? [], agentIds: consumers.filter { $0.vendor == vendor }.map(\.id), range: statsRange,
                            bucketSize: tokenBucketSize, now: dataDate, dimensions: tokenDimensions).map(\.total)
    }

    /// Token cards name execution clients, independently of the services whose quota windows they share.
    /// Until picked explicitly, show recorded clients, configured API clients and clients of the enabled quota rows.
    var shownAgents: Set<String> {
        pickedAgents ?? Set(consumers.map(\.vendor)
            + (report?.services ?? []).filter { $0.product == .api }.map(\.client)
            + enabledAgents.filter(\.connected).flatMap { tokenCardVendors(for: $0) })
    }

    /// A quota event reveals the clients connected to that exact billing pool, without assigning its historical tokens.
    func tokenCardVendors(for quotaID: String) -> [String] {
        guard let agent = visibleAgents.first(where: { $0.id == quotaID }) else { return [] }
        return tokenCardVendors(for: agent)
    }

    private func tokenCardVendors(for agent: AgentDescriptor) -> [String] {
        guard let pool = agent.billingPool else { return [agent.vendor] }
        let clients = (report?.services ?? []).filter { $0.accountID == pool.id && $0.product == pool.product }.map(\.client)
        return clients.isEmpty ? [agent.vendor] : Set(clients).sorted()
    }

    /// What the charted tokens of the selected kinds would cost at list price, counted as the chart counts them, each
    /// model on its client's platform and DeepSeek's peak hours at its peak rates.
    var statsListCost: ModelCatalog.ListCost? {
        let interval = statsInterval, ids = Set(consumers.map(\.id)), dimensions = tokenDimensions
        var tokens: [String: TokenKinds] = [:], peak: [String: TokenKinds] = [:], peakRated: [String: Bool] = [:]
        for bucket in report?.usage ?? [] where ids.contains(bucket.agentId) && bucket.overlaps(interval) {
            let kinds = dimensions.masking(bucket.kinds)
            tokens[bucket.agentId, default: TokenKinds()] += kinds
            let rated = peakRated[bucket.agentId] ?? (ModelCatalog.model(for: bucket.agentId)?.peakHours == true)
            peakRated[bucket.agentId] = rated
            if rated, ModelCatalog.isPeak(bucket.start) { peak[bucket.agentId, default: TokenKinds()] += kinds }
        }
        return ModelCatalog.cost(of: tokens, peak: peak, region: priceRegions.region)
    }

    /// What a period's tokens of the selected kinds would cost at list price.
    func periodListCost(_ period: UsagePeriods.Period) -> ModelCatalog.ListCost? {
        let dimensions = tokenDimensions
        return ModelCatalog.cost(of: periodTokens(period).mapValues(dimensions.masking),
                                 peak: (report?.periods?.peak[period] ?? [:]).mapValues(dimensions.masking), region: priceRegions.region)
    }

    /// Each model's tokens in a period, for the models the charts show.
    func periodTokens(_ period: UsagePeriods.Period) -> [String: TokenKinds] {
        let ids = Set(consumers.map(\.id))
        return (report?.periods?.tokens[period] ?? [:]).filter { ids.contains($0.key) && !$0.value.isEmpty }
    }

    /// Quota switches do not change token-chart colors.
    func consumerPaletteIndex(_ id: String) -> Int {
        consumers.firstIndex { $0.id == id } ?? 0
    }

    func consumerName(_ id: String) -> String {
        guard let consumer = consumers.first(where: { $0.id == id }) else {
            guard let row = rows.first(where: { $0.id == id }) else { return ModelCatalog.consumerName(of: id) }
            return row.agent.name
        }
        return consumer.name
    }
}
