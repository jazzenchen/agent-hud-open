import Foundation

/// Aggregates shown on the stats window. A real provider derives these from history; the demo supplies them directly.
public struct UsageInsights: Hashable, Codable, Sendable {
    public let burnRatePctPerHour: Double?
    /// Seconds until this quota window is exhausted at its current-cycle average rate.
    public let timeToExhaust: TimeInterval?
    public let weeklyCapHits: Int
    public let weeklyWaitTotal: TimeInterval
    public let weeklyWaitLongest: TimeInterval
    public let weeklyWaitLongestAt: Date?
    /// agentId → fraction of this week's consumption (sums to ~1).
    public let weeklyShare: [String: Double]
    public let windowSessionCount: Int
    public let windowUsedPct: Double

    public init(
        burnRatePctPerHour: Double?,
        timeToExhaust: TimeInterval?,
        weeklyCapHits: Int,
        weeklyWaitTotal: TimeInterval,
        weeklyWaitLongest: TimeInterval,
        weeklyWaitLongestAt: Date?,
        weeklyShare: [String: Double],
        windowSessionCount: Int,
        windowUsedPct: Double
    ) {
        self.burnRatePctPerHour = burnRatePctPerHour
        self.timeToExhaust = timeToExhaust
        self.weeklyCapHits = weeklyCapHits
        self.weeklyWaitTotal = weeklyWaitTotal
        self.weeklyWaitLongest = weeklyWaitLongest
        self.weeklyWaitLongestAt = weeklyWaitLongestAt
        self.weeklyShare = weeklyShare
        self.windowSessionCount = windowSessionCount
        self.windowUsedPct = windowUsedPct
    }

    public static let empty = UsageInsights(
        burnRatePctPerHour: nil, timeToExhaust: nil, weeklyCapHits: 0, weeklyWaitTotal: 0,
        weeklyWaitLongest: 0, weeklyWaitLongestAt: nil, weeklyShare: [:], windowSessionCount: 0, windowUsedPct: 0
    )
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
    public let history: [HistorySample]
    public let activity: ActivityGrid
    public let insights: UsageInsights
    /// Short note for the UI when quota data is missing or degraded ("等待 Claude Code 上报额度").
    public let notice: String?
    /// Agent rows the provider found in local data (e.g. Claude model families); the settings store merges them.
    public let discoveredAgents: [AgentDescriptor]
    /// "max", "pro", … when the engine reported it.
    public let subscriptionType: String?
    /// Things that spend tokens (model families), as opposed to quota windows. Sessions and token charts key on these.
    public let consumers: [AgentDescriptor]
    /// Original token events keyed by consumer id, retaining timestamps for chart aggregation.
    public let consumption: [TranscriptSession.UsageEvent]
    /// A complete Claude transcript scan owns usage in this range through `generatedAt`.
    /// Nil for partial scans; only this Mac's archived Claude events in the covered range may be replaced.
    public let claudeConsumptionSince: Date?
    /// Non-nil while local logs are still being indexed; sessions and token figures are partial until then.
    public let indexing: IndexProgress?
    /// Per-window metrics; a Codex window must never display Claude's burn rate.
    public let insightsByAgent: [String: UsageInsights]
    public let subscriptions: [String: String]
    /// Optional so previously saved reports remain readable.
    public let services: [AgentService]?
    public let sourceNotices: [String: String]
    /// Consumer ids covered by each quota row. Providers own the relationship between model and quota ids.
    public let consumerIdsByQuota: [String: Set<String>]
    public let billing: [APIBilling]
    /// One account-wide balance, shared by all Codex quota windows.
    public let codexResetCredits: CodexResetCredits?
    /// Time of the successful account/rateLimits/read, including responses without quota windows.
    public let codexResetCreditsObservedAt: Date?

    public init(
        generatedAt: Date,
        snapshots: [UsageSnapshot],
        sessions: [LiveSession],
        history: [HistorySample],
        activity: ActivityGrid,
        insights: UsageInsights,
        notice: String? = nil,
        discoveredAgents: [AgentDescriptor] = [],
        subscriptionType: String? = nil,
        consumers: [AgentDescriptor]? = nil,
        consumption: [TranscriptSession.UsageEvent] = [],
        indexing: IndexProgress? = nil,
        insightsByAgent: [String: UsageInsights] = [:],
        subscriptions: [String: String] = [:],
        sourceNotices: [String: String] = [:],
        consumerIdsByQuota: [String: Set<String>] = [:],
        billing: [APIBilling] = [],
        codexResetCredits: CodexResetCredits? = nil,
        codexResetCreditsObservedAt: Date? = nil,
        completions: [SessionCompletion] = [],
        claudeConsumptionSince: Date? = nil,
        turns: [SessionTurn] = [],
        services: [AgentService]? = nil
    ) {
        self.services = services
        self.claudeConsumptionSince = claudeConsumptionSince
        self.completions = completions
        self.turns = turns
        self.codexResetCredits = codexResetCredits
        self.codexResetCreditsObservedAt = codexResetCreditsObservedAt
        self.billing = billing
        self.insightsByAgent = insightsByAgent
        self.subscriptions = subscriptions.isEmpty ? subscriptionType.map { ["Claude": $0] } ?? [:] : subscriptions
        self.sourceNotices = sourceNotices
        self.consumerIdsByQuota = consumerIdsByQuota
        self.indexing = indexing
        self.generatedAt = generatedAt
        self.snapshots = snapshots
        self.sessions = sessions
        self.history = history
        self.activity = activity
        self.insights = insights
        self.notice = notice
        self.discoveredAgents = discoveredAgents
        self.subscriptionType = subscriptionType
        self.consumers = consumers ?? discoveredAgents
        self.consumption = consumption
    }

    public func snapshot(for agentId: String) -> UsageSnapshot? {
        snapshots.first { $0.agentId == agentId }
    }

    public func history(for agentId: String) -> [HistorySample] {
        history.filter { $0.agentId == agentId }.sorted { $0.hourStart < $1.hourStart }
    }
}
