import Foundation

/// A display group derived from the ordered model list; it has no separate saved order.
public struct AgentGroup: Identifiable, Equatable, Sendable {
    public let id: String
    public let agents: [AgentDescriptor]
}

public extension Array where Element == AgentDescriptor {
    /// Preserve first appearance and model order, combining rows with the same displayed agent name.
    var agentGroups: [AgentGroup] {
        var order: [String] = []
        var groups: [String: [AgentDescriptor]] = [:]
        for agent in self {
            let group = agent.displayVendor
            if groups[group] == nil { order.append(group) }
            groups[group, default: []].append(agent)
        }
        return order.map { AgentGroup(id: $0, agents: groups[$0]!) }
    }

    /// Keep every agent's models together in the same flat order used by the glow.
    var groupedAgentOrder: [AgentDescriptor] { agentGroups.flatMap(\.agents) }

    func movingGroup(id: String, to targetID: String) -> [AgentDescriptor] {
        var groups = agentGroups
        guard let source = groups.firstIndex(where: { $0.id == id }),
              let target = groups.firstIndex(where: { $0.id == targetID }) else { return self }
        let group = groups.remove(at: source)
        groups.insert(group, at: target)
        return groups.flatMap(\.agents)
    }
}
