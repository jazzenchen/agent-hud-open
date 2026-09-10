import Foundation

/// A confirmed quota event, also used by Settings to preview the production presentation.
public struct QuotaAlert: Identifiable, Hashable, Sendable {
    public enum Kind: String, CaseIterable, Sendable {
        case exhaustion, reset
    }

    public let id: UUID
    public let kind: Kind
    public let agent: AgentDescriptor
    public let snapshot: UsageSnapshot
    public let timeToExhaust: TimeInterval?
    public let otherExhaustedWindows: [String]
    public let isPreview: Bool

    public init(kind: Kind, agent: AgentDescriptor, snapshot: UsageSnapshot,
                timeToExhaust: TimeInterval? = nil, otherExhaustedWindows: [String] = [], isPreview: Bool = false) {
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

/// One observation history owns both island events and the existing critical system notification.
public struct QuotaAlertTracker: Sendable {
    public struct Update: Sendable {
        public var alerts: [QuotaAlert] = []
        public var criticalAgentIDs: Set<String> = []
    }

    private struct Observation: Sendable {
        let snapshot: UsageSnapshot
        let atRisk: Bool
        let critical: Bool
    }
    private var previous: [String: Observation] = [:]

    public init() {}

    public mutating func update(report: UsageReport, agents: [AgentDescriptor], now: Date) -> Update {
        let quotaAgents = agents.filter { !$0.isAPIBilled }
        let ids = Set(quotaAgents.map(\.id))
        previous = previous.filter { ids.contains($0.key) }
        var result = Update()
        for agent in quotaAgents {
            guard let snapshot = report.snapshot(for: agent.id),
                  report.sourceNotices[agent.vendor] == nil,
                  snapshot.updatedAt <= now,
                  now.timeIntervalSince(snapshot.updatedAt) < QuotaForecast.maximumReadingAge else { continue }
            // A passed deadline is pending confirmation. An observed full idle window can have no deadline.
            if let resetAt = snapshot.resetAt, resetAt <= now { continue }
            guard snapshot.resetAt != nil || snapshot.remainingPct == 100 else { continue }
            let old = previous[agent.id]
            let criticalThreshold = 100 - AlertPolicy.criticalUsed
            guard old == nil || snapshot.updatedAt > old!.snapshot.updatedAt else { continue }
            let forecast = report.insightsByAgent[agent.id]?.timeToExhaust
            let predictsCap = forecast.map { interval in
                interval.isFinite && interval > 0 && snapshot.resetAt.map { interval < $0.timeIntervalSince(now) } == true
            } ?? false
            let critical = snapshot.remainingPct <= criticalThreshold
            let atRisk = snapshot.remainingPct <= 0 || critical || predictsCap
            previous[agent.id] = Observation(snapshot: snapshot, atRisk: atRisk, critical: critical)
            guard let old else { continue } // First observation establishes a baseline without notifying.

            if critical && !old.critical { result.criticalAgentIDs.insert(agent.id) }
            let cycleAdvanced = old.snapshot.resetAt.map { oldReset in
                snapshot.resetAt.map { $0 > oldReset && snapshot.updatedAt >= oldReset } == true
            } ?? false
            // An early/manual reset may retain the deadline but restores the full window.
            let restoredEarly = snapshot.remainingPct == 100 && old.snapshot.remainingPct < 100
            if cycleAdvanced || restoredEarly {
                let exhausted = quotaAgents.filter {
                    $0.vendor == agent.vendor && $0.id != agent.id &&
                    report.snapshot(for: $0.id).map { $0.remainingPct <= 0 && ($0.resetAt ?? .distantPast) > now } == true
                }.map(\.model)
                result.alerts.append(QuotaAlert(kind: .reset, agent: agent, snapshot: snapshot, otherExhaustedWindows: exhausted))
            } else if atRisk && !old.atRisk {
                result.alerts.append(QuotaAlert(kind: .exhaustion, agent: agent, snapshot: snapshot,
                                               timeToExhaust: predictsCap ? forecast : nil))
            }
        }
        return result
    }
}
