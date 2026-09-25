import Foundation

/// Which platform a client's calls to a vendor went through, as far as this Mac can tell: the endpoint the client is
/// configured with or the plan it signed in to, and for DeepSeek, which bills one platform in either currency, the
/// currency of its account. A client whose endpoints disagree, or one nothing names, is priced on the international list.
public struct PriceRegions: Hashable, Sendable {
    /// By the client's source id (a consumer id's prefix) and the vendor.
    private var byClient: [String: [String: ModelCatalog.Region]] = [:]
    private var byVendor: [String: ModelCatalog.Region] = [:]

    public init(services: [AgentService], billing: [APIBilling]) {
        var clients: [String: [String: Set<ModelCatalog.Region>]] = [:], vendors: [String: Set<ModelCatalog.Region>] = [:]
        for service in services {
            guard let region = service.region else { continue }
            let vendor = Self.vendor(service.provider)
            clients[service.client.lowercased(), default: [:]][vendor, default: []].insert(region)
            vendors[vendor, default: []].insert(region)
        }
        for account in billing where account.vendor == "DeepSeek" {
            for balance in account.balances {
                if let region = ModelCatalog.Region.allCases.first(where: { $0.currency == balance.currency }) {
                    vendors["DeepSeek", default: []].insert(region)
                }
            }
        }
        byClient = clients.mapValues { $0.compactMapValues { $0.count == 1 ? $0.first : nil } }
        byVendor = vendors.compactMapValues { $0.count == 1 ? $0.first : nil }
    }

    public init(report: UsageReport?) {
        self.init(services: report?.services ?? [], billing: report?.billing ?? [])
    }

    public func region(for agentId: String) -> ModelCatalog.Region {
        guard let vendor = ModelCatalog.model(for: agentId)?.vendor else { return .international }
        let client = agentId.range(of: "-model:").map { String(agentId[..<$0.lowerBound]) } ?? ""
        return byClient[client]?[vendor] ?? byVendor[vendor] ?? .international
    }

    /// Plans name their vendor by product; the catalog by the company that prices the model.
    private static func vendor(_ provider: String) -> String { provider == "Kimi" ? "Moonshot" : provider }
}
