import Foundation

/// Fixed application policy. No threshold values are stored or synchronized.
public enum AlertPolicy {
    public static let warningUsed: Double = 70
    public static let criticalUsed: Double = 90

    public static func quotaLevel(remaining: Double) -> StatusLevel {
        StatusLevel.resolve(remainingPct: remaining, warnPct: 100 - warningUsed, critPct: 100 - criticalUsed)
    }

    public static func balanceLevel(remaining: Decimal, currency: String) -> StatusLevel? {
        guard !remaining.isNaN else { return nil }
        let warning: Decimal
        switch currency {
        case "CNY": warning = 10
        case "USD": warning = 2
        default: return remaining <= 0 ? .critical : nil
        }
        return remaining <= 0 ? .critical : remaining <= warning ? .warning : .ok
    }
}

public extension APIBilling {
    func contains(_ model: AgentDescriptor) -> Bool {
        guard model.isAPIBilled else { return false }
        if let billingPool { return model.billingPool?.id == billingPool.id }
        return model.billingPool == nil && model.vendor == vendor
    }
}
