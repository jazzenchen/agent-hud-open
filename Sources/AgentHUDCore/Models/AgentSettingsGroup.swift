import Foundation

/// Joins observed account information to the existing ordered display windows.
public struct AgentSettingsGroup: Identifiable, Equatable, Sendable {
    public let id: String
    public let source: SourceStatus?
    public let agents: [AgentDescriptor]
    public let plans: [String]
    public let apiProviders: [String]

    public var displayedCount: Int { agents.filter(\.enabled).count }
    public var hasLiveStatus: Bool { SessionSource.agentVendors.contains(id) }

    public static func make(sources: [SourceStatus], agents: [AgentDescriptor], report: UsageReport? = nil) -> [Self] {
        let agents = agents.filter { agent in
            guard let pool = agent.billingPool, pool.product == .plan,
                  let active = report?.activeQuotaPoolIDs?[pool.provider] else { return true }
            return active.contains(pool.id)
        }
        func vendor(_ source: SourceStatus) -> String {
            source.id == "chatgpt" ? "ChatGPT" : source.name
        }
        let existing = agents.agentGroups
        var ids = existing.map(\.id)
        for id in sources.map(vendor) + agents.map(\.vendor) + (report?.services ?? []).map(\.client)
            where !ids.contains(id) { ids.append(id) }
        return ids.map { id in
            let source = sources.first { vendor($0) == id }
            let windows = existing.first { $0.id == id }?.agents ?? []
            let services = (report?.services ?? []).filter { $0.client == id }
            var plans = source?.planLabel.map { [$0] } ?? []
            for service in services where service.product == .plan {
                guard let plan = report?.subscriptions[service.accountID ?? service.provider], !plan.isEmpty else { continue }
                plans.append(service.provider == id ? plan.capitalized : service.provider + " · " + plan.capitalized)
            }
            for window in windows {
                guard let pool = window.billingPool, pool.product == .plan,
                      let plan = report?.subscriptions[pool.id], !plan.isEmpty else { continue }
                plans.append(plan.capitalized)
            }
            var api = services.filter { $0.product == .api }.map(\.provider)
            api += agents.filter { $0.vendor == id && $0.billingPool?.product == .api }.compactMap { $0.billingPool?.provider }
            api += windows.compactMap { $0.billingPool?.product == .api ? $0.billingPool?.provider : nil }
            api += (report?.billing ?? []).filter {
                ($0.billingPool?.provider ?? $0.vendor) == id && (!$0.balances.isEmpty || !$0.costs.isEmpty)
            }.map { $0.billingPool?.provider ?? $0.vendor }
            return Self(id: id, source: source, agents: windows, plans: Array(Set(plans)).sorted(),
                        apiProviders: Array(Set(api)).sorted())
        }
    }
}
