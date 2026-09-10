import Foundation

public struct AccountBalance: Hashable, Codable, Sendable, Identifiable {
    public let currency: String
    public let total: Decimal
    public let granted: Decimal
    public let toppedUp: Decimal
    public var id: String { currency }

    public init(currency: String, total: Decimal, granted: Decimal, toppedUp: Decimal) {
        self.currency = currency; self.total = total; self.granted = granted; self.toppedUp = toppedUp
    }
}

/// Money remains independent of subscription percentages: a balance has no fixed denominator.
public struct APIBilling: Hashable, Codable, Sendable, Identifiable {
    public struct CostSample: Hashable, Codable, Sendable {
        public let timestamp: Date
        public let sessionId: String
        public let model: String
        /// Estimates in each explicitly priced currency; no currency conversion is implied.
        public let amounts: [String: Decimal]
        public var eventID: String? = nil
        public var billingPool: BillingPool? = nil

        public init(timestamp: Date, sessionId: String, model: String, amounts: [String: Decimal], eventID: String? = nil, billingPool: BillingPool? = nil) {
            self.timestamp = timestamp; self.sessionId = sessionId; self.model = model
            self.amounts = amounts; self.eventID = eventID; self.billingPool = billingPool
        }
    }

    public let vendor: String
    public let balances: [AccountBalance]
    public let isAvailable: Bool?
    public let updatedAt: Date?
    public let costs: [CostSample]
    public let notice: String?
    public var billingPool: BillingPool? = nil
    public var id: String { billingPool?.id ?? vendor }
    public var displayName: String { (billingPool?.provider ?? vendor) + " · API" }
    public var currency: String { balances.first?.currency ?? "CNY" }

    public init(vendor: String, balances: [AccountBalance], isAvailable: Bool?, updatedAt: Date?, costs: [CostSample], notice: String?, billingPool: BillingPool? = nil) {
        self.vendor = vendor; self.balances = balances; self.isAvailable = isAvailable
        self.updatedAt = updatedAt; self.costs = costs; self.notice = notice; self.billingPool = billingPool
    }

    public func estimatedCost(currency: String, during interval: DateInterval? = nil, sessionId: String? = nil) -> Decimal? {
        return Self.sum(costs.filter { sample in
            (interval == nil || (sample.timestamp >= interval!.start && sample.timestamp <= interval!.end))
                && (sessionId == nil || sample.sessionId == sessionId)
        }, currency: currency)
    }

    private static func sum(_ samples: [CostSample], currency: String) -> Decimal? {
        var total: Decimal = 0
        for sample in samples {
            guard let amount = sample.amounts[currency] else { return nil }
            total += amount
        }
        return total
    }
}

public enum MoneyFormat {
    public static func amount(_ amount: Decimal, currency: String, estimated: Bool = false) -> String {
        let format = Decimal.FormatStyle.Currency(code: currency)
            .presentation(.narrow)
            .locale(Locale(identifier: L10n.resolved == .zhHans ? "zh_CN" : "en_US"))
            .precision(.fractionLength(2...(estimated ? 6 : 2)))
        let minimum = Decimal(string: "0.000001")!
        if estimated, amount > 0, amount < minimum {
            return "<" + minimum.formatted(format)
        }
        return amount.formatted(format)
    }
}
