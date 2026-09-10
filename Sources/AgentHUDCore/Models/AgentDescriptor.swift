import Foundation

/// One monitored agent/model row, e.g. "Claude · Opus 4.5". Order in the stored array is the glow order (left → right).
public struct AgentDescriptor: Hashable, Codable, Sendable, Identifiable {
    public let id: String
    public let vendor: String
    public let model: String
    /// Human-readable data-source label shown in settings ("Claude Code 会话", "codex app-server", "未连接").
    public let source: String
    public let enabled: Bool
    /// Whether a local data source was detected for this agent.
    public let connected: Bool
    public let billingPool: BillingPool?

    public init(
        id: String,
        vendor: String,
        model: String,
        source: String,
        enabled: Bool,
        connected: Bool = true,
        billingPool: BillingPool? = nil
    ) {
        self.id = id
        self.vendor = vendor
        self.model = model
        self.source = source
        self.enabled = enabled
        self.connected = connected
        self.billingPool = billingPool
    }

    public var displayVendor: String { L10n.vendorLabel(billingPool?.product == .api ? billingPool!.provider : vendor) }
    public var displayName: String { "\(displayVendor) · \(L10n.modelLabel(model))" }

    /// DeepSeek exposes API balance and costs instead of subscription quota windows.
    public var isAPIBilled: Bool {
        if let product = billingPool?.product, product != .unknown { return product == .api }
        return vendor == "DeepSeek"
    }

    /// Returns a copy with the given fields replaced (the core never mutates in place).
    public func with(
        enabled: Bool? = nil,
        connected: Bool? = nil
    ) -> AgentDescriptor {
        AgentDescriptor(
            id: id,
            vendor: vendor,
            model: model,
            source: source,
            enabled: enabled ?? self.enabled,
            connected: connected ?? self.connected,
            billingPool: billingPool
        )
    }

}

public extension Array where Element == AgentDescriptor {
    /// Moves the agent with `id` to `index`, returning a new array.
    func moving(id: String, to index: Int) -> [AgentDescriptor] {
        guard let from = firstIndex(where: { $0.id == id }), index >= 0, index < count else { return self }
        var copy = self
        let item = copy.remove(at: from)
        copy.insert(item, at: index)
        return copy
    }

    func replacing(_ agent: AgentDescriptor) -> [AgentDescriptor] {
        map { $0.id == agent.id ? agent : $0 }
    }
}
