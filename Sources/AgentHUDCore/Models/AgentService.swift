import Foundation

/// An observed relationship between a client and its service. Balances and costs stay with APIBilling.
public struct AgentService: Hashable, Codable, Sendable, Identifiable {
    public let client: String
    public let provider: String
    public let product: BillingPool.Product
    /// References the existing subscription or billing account; account values are not copied here.
    public let accountID: String?
    /// The platform the client reaches the provider on, where its endpoint or plan says; it picks the price list.
    public let region: ModelCatalog.Region?

    public init(client: String, provider: String, product: BillingPool.Product, accountID: String? = nil,
                region: ModelCatalog.Region? = nil) {
        self.client = client; self.provider = provider; self.product = product
        self.accountID = accountID; self.region = region
    }

    public var id: String { [client, provider, product.rawValue, accountID ?? "", region?.rawValue ?? ""].joined(separator: ":") }

    public static func merge(_ groups: [[Self]]) -> [Self] {
        groups.reduce(into: [String: Self]()) { result, group in
            for service in group { result[service.id] = service }
        }.values.sorted { $0.id < $1.id }
    }
}
