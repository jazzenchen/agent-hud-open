import Foundation
import Observation

/// Persists `Settings`, the ordered agent list and onboarding state in UserDefaults as JSON.
@MainActor
@Observable
public final class SettingsStore {
    public private(set) var settings: Settings
    public private(set) var agents: [AgentDescriptor]
    public private(set) var hasCompletedOnboarding: Bool

    public enum Change { case settings, agents, discovery }
    /// The sync service observes persisted mutations, after translations and local state are updated.
    @ObservationIgnored public var onChange: ((Change) -> Void)?

    private let defaults: UserDefaults
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    public enum Keys {
        public static let settings = "settings.v1"
        public static let agents = "agents.v5"
        public static let onboarding = "onboarding.completed.v1"
    }

    public init(defaults: UserDefaults = .standard, defaultAgents: [AgentDescriptor] = DefaultAgents.list) {
        self.defaults = defaults
        let decoder = JSONDecoder()
        if let data = defaults.data(forKey: Keys.settings), let stored = try? decoder.decode(Settings.self, from: data) {
            settings = stored
        } else {
            settings = Settings()
        }
        if let data = defaults.data(forKey: Keys.agents), let stored = try? decoder.decode([AgentDescriptor].self, from: data), !stored.isEmpty {
            // Only observed subscriptions belong in the agent catalog.
            agents = stored.map { agent in
                guard agent.id == "chatgpt", agent.vendor == "ChatGPT", agent.model == "Plus",
                      agent.source == L10n.sourceBrowserAuth else { return agent }
                return AgentDescriptor(id: agent.id, vendor: agent.vendor, model: "ChatGPT", source: agent.source,
                    enabled: agent.enabled, connected: agent.connected, billingPool: agent.billingPool)
            }.groupedAgentOrder
        } else {
            agents = defaultAgents.groupedAgentOrder
        }
        hasCompletedOnboarding = defaults.bool(forKey: Keys.onboarding)
    }

    public var enabledAgents: [AgentDescriptor] { agents.filter(\.enabled) }

    public func update(_ change: (inout Settings) -> Void) {
        let next = settings.with(change)
        guard next != settings else { return }
        // Views can render as soon as settings change; translations must already match.
        if next.language != settings.language {
            L10n.setLanguage(next.language)
        }
        settings = next
        persist(next, key: Keys.settings)
        onChange?(.settings)
    }

    public func updateAgents(_ transform: ([AgentDescriptor]) -> [AgentDescriptor]) {
        saveAgents(transform(agents), change: .agents)
    }

    private func saveAgents(_ list: [AgentDescriptor], change: Change) {
        let next = list.groupedAgentOrder
        guard next != agents else { return }
        agents = next
        persist(next, key: Keys.agents)
        onChange?(change)
    }

    public func setAgent(id: String, enabled: Bool) {
        updateAgents { list in list.map { $0.id == id ? $0.with(enabled: enabled) : $0 } }
    }

    public func moveAgent(id: String, to index: Int) {
        updateAgents { $0.moving(id: id, to: index) }
    }

    public func moveAgentGroup(id: String, to targetID: String) {
        updateAgents { $0.movingGroup(id: id, to: targetID) }
    }

    /// Adds rows a provider discovered (in the order given) and refreshes model names of known rows.
    /// New rows for a vendor go right after that vendor's last existing row, or at the top when the vendor is new,
    /// so the user's manual order is preserved.
    public func mergeDiscovered(_ discovered: [AgentDescriptor]) {
        guard !discovered.isEmpty else { return }
        let merged: [AgentDescriptor] = {
            var list = agents
            if discovered.contains(where: { $0.vendor == "DeepSeek" && $0.id.hasPrefix("deepseek-model:") }),
               let index = list.firstIndex(where: { $0.id == "deepseek" }) {
                let placeholder = list.remove(at: index)
                let replacements = discovered.filter { found in found.vendor == "DeepSeek" && !list.contains(where: { $0.id == found.id }) }
                    .map { $0.with(enabled: placeholder.enabled) }
                list.insert(contentsOf: replacements, at: index)
            }
            // Replace the old disconnected Antigravity placeholder when real quota buckets arrive.
            if discovered.contains(where: { $0.vendor == "Antigravity" && $0.id.hasPrefix("antigravity:") }),
               let index = list.firstIndex(where: { $0.id == "antigravity" && $0.source == L10n.sourceNotConnected }) {
                let placeholder = list.remove(at: index)
                let replacements = discovered.filter { found in found.vendor == "Antigravity" && !list.contains(where: { $0.id == found.id }) }.map {
                    $0.with(enabled: placeholder.enabled)
                }
                list.insert(contentsOf: replacements, at: index)
            }
            for found in discovered {
                if let index = list.firstIndex(where: { $0.id == found.id }) {
                    let existing = list[index]
                    if existing.model != found.model || existing.source != found.source || existing.connected != found.connected || existing.billingPool != found.billingPool {
                        list[index] = AgentDescriptor(
                            id: existing.id, vendor: existing.vendor, model: found.model, source: found.source,
                            enabled: existing.enabled, connected: found.connected, billingPool: found.billingPool
                        )
                    }
                    continue
                }
                let anchor = list.lastIndex { $0.vendor == found.vendor }
                list.insert(found, at: anchor.map { $0 + 1 } ?? 0)
            }
            return list
        }()
        saveAgents(merged, change: .discovery)
    }

    public func markOnboardingComplete() {
        hasCompletedOnboarding = true
        defaults.set(true, forKey: Keys.onboarding)
    }

    public func resetOnboarding() {
        hasCompletedOnboarding = false
        defaults.set(false, forKey: Keys.onboarding)
    }

    private func persist<T: Encodable>(_ value: T, key: String) {
        guard let data = try? encoder.encode(value) else { return }
        defaults.set(data, forKey: key)
    }
}
