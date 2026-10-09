import Foundation

/// A confirmed quota event; `preview` builds sample events for snapshots.
public struct QuotaAlert: Identifiable, Hashable, Sendable {
    public enum Kind: String, CaseIterable, Sendable {
        case exhaustion, reset
    }

    public let id: UUID
    public let kind: Kind
    public let agent: AgentDescriptor
    public let snapshot: UsageSnapshot
    public let timeToExhaust: TimeInterval?
    /// The account's other windows still exhausted when this one reset, named through their descriptors.
    public let otherExhaustedWindows: [AgentDescriptor]
    public let isPreview: Bool

    public init(kind: Kind, agent: AgentDescriptor, snapshot: UsageSnapshot,
                timeToExhaust: TimeInterval? = nil, otherExhaustedWindows: [AgentDescriptor] = [], isPreview: Bool = false) {
        id = UUID()
        self.kind = kind
        self.agent = agent
        self.snapshot = snapshot
        self.timeToExhaust = timeToExhaust
        self.otherExhaustedWindows = otherExhaustedWindows
        self.isPreview = isPreview
    }

    public static func preview(_ kind: Kind, agent: AgentDescriptor, now: Date = Date()) -> QuotaAlert {
        QuotaAlert(kind: kind, agent: agent,
                   snapshot: UsageSnapshot(agentId: agent.id, remainingPct: kind == .reset ? 100 : 8,
                                           resetAt: now.addingTimeInterval(kind == .reset ? 5 * 3600 : 2 * 3600), updatedAt: now),
                   timeToExhaust: kind == .exhaustion ? 40 * 60 : nil, isPreview: true)
    }
}

/// One observation history per window decides island events and the threshold crossings a host may relay.
public struct QuotaAlertTracker: Sendable {
    public struct Update: Sendable {
        public var alerts: [QuotaAlert] = []
        /// Windows that crossed the critical threshold in this update.
        public var criticalAgentIDs: Set<String> = []
        /// Windows that reached zero in this update; a window already at zero on first sight is a silent baseline.
        public var exhaustedAgentIDs: Set<String> = []
    }

    private struct Observation: Sendable {
        let snapshot: UsageSnapshot
        let atRisk: Bool
        let critical: Bool
        let exhausted: Bool
        /// The reset of the cycle a forecast already warned in. The estimate moves with every reading, so it warns once
        /// per cycle, not whenever it dips below the reset and back.
        var forecastWarnedUntil: Date? = nil
    }
    private var previous: [String: Observation] = [:]

    public init() {}

    public mutating func update(report: UsageReport, agents: [AgentDescriptor], now: Date) -> Update {
        let quotaAgents = agents.filter { !$0.isAPIBilled }
        let ids = Set(quotaAgents.map(\.id))
        previous = previous.filter { ids.contains($0.key) }
        var result = Update()
        for agent in quotaAgents {
            let reading = report.assess(.window(agent), now: now)
            // Signing back in to an account is a new baseline, not a reset observed while it was away.
            guard reading.isCurrentAccount else { previous[agent.id] = nil; continue }
            guard let snapshot = report.snapshot(for: agent.id) else { continue }
            // An explicit unknown value breaks the numeric baseline. A failed or absent read still keeps it.
            guard let remaining = snapshot.remainingPct else {
                if reading.status.isNormal, !reading.isFromFuture,
                   previous[agent.id].map({ snapshot.updatedAt > $0.snapshot.updatedAt }) ?? true {
                    previous[agent.id] = nil
                }
                continue
            }
            // A passed deadline is pending confirmation.
            guard reading.confirmsEvents else { continue }
            // An observed full idle window can have no deadline.
            guard snapshot.resetAt != nil || remaining == 100 else { continue }
            let old = previous[agent.id]
            let criticalThreshold = 100 - AlertPolicy.criticalUsed
            guard old == nil || snapshot.updatedAt > old!.snapshot.updatedAt else { continue }
            let forecast = QuotaMath.exhaustion(insights: report.insightsByAgent[agent.id], resetAt: snapshot.resetAt, now: now)
            // Running out counts only before a known reset.
            let predictsCap = snapshot.resetAt != nil && forecast?.beforeReset == true
            let critical = remaining <= criticalThreshold
            let exhausted = remaining <= AlertPolicy.exhaustedRemaining
            let atRisk = exhausted || critical || predictsCap
            let warnedUntil = old?.forecastWarnedUntil.flatMap { $0 > snapshot.updatedAt ? $0 : nil }
            previous[agent.id] = Observation(snapshot: snapshot, atRisk: atRisk, critical: critical, exhausted: exhausted,
                                             forecastWarnedUntil: warnedUntil)
            guard let old, let oldRemaining = old.snapshot.remainingPct else { continue } // First observation is a silent baseline.

            if critical && !old.critical { result.criticalAgentIDs.insert(agent.id) }
            if exhausted && !old.exhausted { result.exhaustedAgentIDs.insert(agent.id) }
            let cycleAdvanced = old.snapshot.resetAt.map { oldReset in
                snapshot.resetAt.map { $0 > oldReset && snapshot.updatedAt >= oldReset } == true
            } ?? false
            // An early/manual reset may retain the deadline but restores the full window; a reading that wobbles up to
            // full by a point or two is not one.
            let restoredEarly = remaining == 100 && remaining - oldRemaining >= AlertPolicy.resetRise
            if cycleAdvanced || restoredEarly {
                // The account's other windows whose readings show a level and are exhausted until a later reset.
                let otherExhausted = quotaAgents.filter {
                    $0.vendor == agent.vendor && $0.account?.id == agent.account?.id && $0.id != agent.id &&
                    report.assess(.window($0), now: now).showsLevel && report.snapshot(for: $0.id).map {
                        $0.remainingPct.map { $0 <= AlertPolicy.exhaustedRemaining } == true && ($0.resetAt ?? .distantPast) > now
                    } == true
                }
                result.alerts.append(QuotaAlert(kind: .reset, agent: agent, snapshot: snapshot, otherExhaustedWindows: otherExhausted))
            } else if exhausted && !old.exhausted {
                // Running out is its own event even after the earlier at-risk warning.
                result.alerts.append(QuotaAlert(kind: .exhaustion, agent: agent, snapshot: snapshot))
            } else if atRisk && !old.atRisk && (critical || warnedUntil == nil) {
                result.alerts.append(QuotaAlert(kind: .exhaustion, agent: agent, snapshot: snapshot,
                                               timeToExhaust: predictsCap ? forecast?.interval : nil))
                if !critical { previous[agent.id]?.forecastWarnedUntil = snapshot.resetAt }
            }
        }
        return result
    }
}
