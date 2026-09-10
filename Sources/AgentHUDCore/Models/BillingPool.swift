import AgentHUDSupport
import Foundation

/// The payer is independent of the program making a request. Only identical, explicit scopes merge.
/// Scope values are hashes of provider-owned IDs or credentials, never credentials themselves.
public struct BillingPool: Hashable, Codable, Sendable, Identifiable {
    public enum Product: String, Codable, Sendable { case plan, api, unknown }
    public enum Evidence: String, Codable, Sendable { case account, credential, unresolved }
    public let provider: String
    public let realm: String
    public let product: Product
    public let scope: String
    public let evidence: Evidence
    public let organization: String?
    public let project: String?
    public let entitlement: String

    public init(provider: String, realm: String, product: Product, scope: String, evidence: Evidence,
                organization: String? = nil, project: String? = nil, entitlement: String) {
        self.provider = provider; self.realm = realm; self.product = product; self.scope = scope
        self.evidence = evidence; self.organization = organization; self.project = project; self.entitlement = entitlement
    }

    public var id: String {
        "pool:" + RecordCoding.hash([provider, realm, product.rawValue, evidence.rawValue, scope,
                                  organization ?? "", project ?? "", entitlement])
    }
    public func windowID(_ window: String) -> String { id + ":" + window }
    public var label: String {
        let kind = product == .unknown ? L10n.text("计费未确认", "Billing unconfirmed") : product.rawValue.uppercased()
        let identity = evidence == .account ? L10n.text("账户", "Account")
            : evidence == .credential ? L10n.text("凭据", "Credential") : L10n.text("归属未确认", "Unconfirmed")
        return "\(realm) · \(kind) · \(identity) \(id.dropFirst(5).prefix(6))"
    }
}

/// Historical attribution is recorded with the observation, never reconstructed from today's login.
public struct UsageAttribution: Hashable, Codable, Sendable {
    public let client: String
    public let providerID: String
    public let pool: BillingPool?
    /// The client may price a subscription request at API list prices. This is an estimate, not a bill.
    public let estimatedUSD: Decimal?
    public init(client: String, providerID: String, pool: BillingPool? = nil, estimatedUSD: Decimal? = nil) {
        self.client = client; self.providerID = providerID; self.pool = pool; self.estimatedUSD = estimatedUSD
    }
}
