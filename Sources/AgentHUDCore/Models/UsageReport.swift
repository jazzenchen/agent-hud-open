import Foundation

/// Metrics of one quota window, derived from its stored readings: burn rate, forecast and this week's cap hits.
public struct UsageInsights: Hashable, Codable, Sendable {
    public let burnRatePctPerHour: Double?
    /// Seconds until this quota window is exhausted at its recent burn rate.
    public let timeToExhaust: TimeInterval?
    public let weeklyCapHits: Int
    public let weeklyWaitTotal: TimeInterval
    public let weeklyWaitLongest: TimeInterval
    public let weeklyWaitLongestAt: Date?

    public init(
        burnRatePctPerHour: Double?,
        timeToExhaust: TimeInterval?,
        weeklyCapHits: Int,
        weeklyWaitTotal: TimeInterval,
        weeklyWaitLongest: TimeInterval,
        weeklyWaitLongestAt: Date?
    ) {
        self.burnRatePctPerHour = burnRatePctPerHour
        self.timeToExhaust = timeToExhaust
        self.weeklyCapHits = weeklyCapHits
        self.weeklyWaitTotal = weeklyWaitTotal
        self.weeklyWaitLongest = weeklyWaitLongest
        self.weeklyWaitLongestAt = weeklyWaitLongestAt
    }
}

/// Background indexing of local logs: how many sessions are ready out of the total to scan.
public struct IndexProgress: Hashable, Codable, Sendable {
    public let done: Int
    public let total: Int

    public init(done: Int, total: Int) {
        self.done = done
        self.total = total
    }

    public var label: String { L10n.text("索引中 \(done)/\(total)", "Indexing \(done)/\(total)") }
}

/// Everything one poll returns.
public struct UsageReport: Hashable, Codable, Sendable {
    public let generatedAt: Date
    public let snapshots: [UsageSnapshot]
    public let sessions: [LiveSession]
    /// Explicit local turn completions, kept separate from archived/inactive sessions.
    public let completions: [SessionCompletion]
    /// Explicit current/recent turn observations; archived session activity is not a substitute.
    public let turns: [SessionTurn]
    /// Short note for the UI when quota data is missing or degraded ("等待 Claude Code 上报额度").
    public let notice: String?
    /// Agent rows the provider found in local data (e.g. Claude model families); the settings store merges them.
    public let discoveredAgents: [AgentDescriptor]
    /// Things that spend tokens (model families), as opposed to quota windows. Sessions and token charts key on these.
    public let consumers: [AgentDescriptor]
    /// Token totals in 15-minute periods, ordered by start, after overlapping logs were resolved. Charts sum these periods.
    public let usage: [UsageBucket]
    /// Non-nil while local logs are still being indexed; sessions and token figures are partial until then.
    public let indexing: IndexProgress?
    /// Per-window metrics; a Codex window must never display Claude's burn rate.
    public let insightsByAgent: [String: UsageInsights]
    public let subscriptions: [String: String]
    /// Optional so previously saved reports remain readable.
    public let services: [AgentService]?
    /// Authoritative pool inventory for each reporting provider. An empty set means no usable credentials.
    /// Nil keeps older cached reports decodable; it does not assert that their credentials are still valid.
    public let activeQuotaPoolIDs: [String: Set<String>]?
    /// Account inventory by provider name. A present key is that provider's authoritative list, an empty list included.
    /// Nil keeps older cached reports decodable.
    public let accounts: [String: [AccountObservation]]?
    /// Providers whose accounts must be forgotten now, for example after the user withdrew consent to read them.
    /// Unlike an empty inventory, which keeps earlier accounts as last readings, their readings, rows and settings retire at once.
    public let forgottenAccountProviders: Set<String>?
    public let sourceNotices: [String: String]
    /// Consumer ids covered by each quota row. Providers own the relationship between model and quota ids.
    public let consumerIdsByQuota: [String: Set<String>]
    public let billing: [APIBilling]
    /// One account-wide balance, shared by all Codex quota windows.
    public let codexResetCredits: CodexResetCredits?
    /// Time of the successful account/rateLimits/read, including responses without quota windows.
    public let codexResetCreditsObservedAt: Date?
    /// Where each session's tokens went, by session id. Optional so previously saved reports remain readable.
    public let sessionUsage: [String: SessionUsage]?
    /// Every model's tokens today and over the last seven and thirty days; `usage` covers only the charts' week.
    public let periods: UsagePeriods?

    public init(
        generatedAt: Date,
        snapshots: [UsageSnapshot],
        sessions: [LiveSession],
        notice: String? = nil,
        discoveredAgents: [AgentDescriptor] = [],
        consumers: [AgentDescriptor]? = nil,
        usage: [UsageBucket] = [],
        indexing: IndexProgress? = nil,
        insightsByAgent: [String: UsageInsights] = [:],
        subscriptions: [String: String] = [:],
        sourceNotices: [String: String] = [:],
        consumerIdsByQuota: [String: Set<String>] = [:],
        billing: [APIBilling] = [],
        codexResetCredits: CodexResetCredits? = nil,
        codexResetCreditsObservedAt: Date? = nil,
        completions: [SessionCompletion] = [],
        turns: [SessionTurn] = [],
        services: [AgentService]? = nil,
        activeQuotaPoolIDs: [String: Set<String>]? = nil,
        accounts: [String: [AccountObservation]]? = nil,
        forgottenAccountProviders: Set<String>? = nil,
        sessionUsage: [String: SessionUsage]? = nil,
        periods: UsagePeriods? = nil
    ) {
        self.sessionUsage = sessionUsage
        self.periods = periods
        self.accounts = accounts
        self.forgottenAccountProviders = forgottenAccountProviders
        self.services = services
        self.activeQuotaPoolIDs = activeQuotaPoolIDs
        self.completions = completions
        self.turns = turns
        self.codexResetCredits = codexResetCredits
        self.codexResetCreditsObservedAt = codexResetCreditsObservedAt
        self.billing = billing
        self.insightsByAgent = insightsByAgent
        self.subscriptions = subscriptions
        self.sourceNotices = sourceNotices
        self.consumerIdsByQuota = consumerIdsByQuota
        self.indexing = indexing
        self.generatedAt = generatedAt
        self.snapshots = snapshots
        self.sessions = sessions
        self.notice = notice
        self.discoveredAgents = discoveredAgents
        self.consumers = consumers ?? discoveredAgents
        self.usage = usage
    }

    public func snapshot(for agentId: String) -> UsageSnapshot? {
        snapshots.first { $0.agentId == agentId }
    }
}
