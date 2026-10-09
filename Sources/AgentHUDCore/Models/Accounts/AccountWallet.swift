import Foundation

/// Reported money belongs to an account independently of its subscription allowance.
public struct AccountWallet: Hashable, Codable, Sendable, Identifiable {
    public enum Kind: String, Codable, Sendable { case prepaid, onDemand }

    public let kind: Kind
    public let currency: String
    public let balance: Decimal?
    public let used: Decimal?
    public let limit: Decimal?
    /// The source's observation time, which can differ from the account's subscription reading.
    public let observedAt: Date?
    public var id: String { kind.rawValue + ":" + currency }

    public init(kind: Kind, currency: String = "USD", balance: Decimal? = nil, used: Decimal? = nil,
                limit: Decimal? = nil, observedAt: Date? = nil) {
        self.kind = kind
        self.currency = currency
        self.balance = balance
        self.used = used
        self.limit = limit
        self.observedAt = observedAt
    }

    /// Only a known usage and positive spending limit define a usage percentage.
    public var usedPercent: Double? {
        guard let used, used >= 0, let limit, limit > 0 else { return nil }
        let value = NSDecimalNumber(decimal: used / limit * 100).doubleValue
        return value.isFinite ? value : nil
    }

    public func observed(at date: Date) -> AccountWallet {
        AccountWallet(kind: kind, currency: currency, balance: balance, used: used, limit: limit, observedAt: date)
    }
}
