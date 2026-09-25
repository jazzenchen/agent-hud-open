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
    /// The account whose quota this row shows. Nil for token consumers, API rows and placeholders.
    public let account: ProviderAccount?

    public init(
        id: String,
        vendor: String,
        model: String,
        source: String,
        enabled: Bool,
        connected: Bool = true,
        billingPool: BillingPool? = nil,
        account: ProviderAccount? = nil
    ) {
        self.id = id
        self.vendor = vendor
        self.model = model
        self.source = source
        self.enabled = enabled
        self.connected = connected
        self.billingPool = billingPool
        self.account = account
    }

    /// The provider's own window name inside the account-scoped id (`codex`, `claude-session`, `weekly`).
    public var windowKey: String {
        if let account, id.hasPrefix(account.id + "/") { return String(id.dropFirst(account.id.count + 1)) }
        if let billingPool, id.hasPrefix(billingPool.id + ":") { return String(id.dropFirst(billingPool.id.count + 1)) }
        return id
    }

    /// The vendor this row is grouped under: the API provider for API-billed rows. An id, not a name to show.
    public var displayVendor: String { billingPool?.product == .api ? billingPool!.provider : vendor }
    /// The group's name as shown, from the vendor catalog.
    public var vendorName: String { VendorCatalog.name(displayVendor) }
    public var displayName: String { "\(vendorName) · \(L10n.modelLabel(model))" }

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
            billingPool: billingPool,
            account: account
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
