import Foundation

/// API list prices and context windows of the models agents call, for an estimate of what their calls would cost at the
/// vendor's API and how full a session's context is. Prices are per million tokens at the vendors' published rates, as
/// checked on `checkedOn`. A model missing here has no estimate; nothing is guessed from a name it does not carry exactly.
public enum ModelCatalog {
    public static let checkedOn = "2026-09-25"

    /// The platforms a vendor sells on: its international one, priced in US dollars, and its China-mainland one, priced in
    /// yuan, where the two lists differ. A vendor with one list for everyone has it as `international`.
    public enum Region: String, CaseIterable, Codable, Sendable {
        case international, china

        public var currency: String { self == .china ? "CNY" : "USD" }
    }

    public struct Rates: Hashable, Sendable {
        public let input: Decimal
        public let cacheWrite: Decimal
        public let cacheRead: Decimal
        /// Reasoning is billed as output everywhere.
        public let output: Decimal
    }

    /// One platform's list: the base rates and, for a call whose prompt reaches a tier, that tier's rates.
    public struct Price: Hashable, Sendable {
        public struct Tier: Hashable, Sendable {
            /// The shortest prompt, in tokens, the tier applies to.
            public let from: Int
            public let rates: Rates
        }

        public let rates: Rates
        /// Ascending by `from`.
        public let tiers: [Tier]

        public func rates(prompt: Int) -> Rates { tiers.last { prompt >= $0.from }?.rates ?? rates }
    }

    public struct Model: Hashable, Sendable {
        public let vendor: String
        public let prices: [Region: Price]
        /// One list for everyone, whichever platform a call went through.
        public let global: Bool
        /// The published window; nil where the client reports it with every call.
        public let contextWindow: Int?
        /// DeepSeek bills twice its off-peak rates in Beijing working hours.
        public let peakHours: Bool

        /// The list a call on `region` is priced by: that platform's, or the one list of a global vendor. A regional
        /// model a platform does not sell has no price there.
        public func price(in region: Region) -> (price: Price, region: Region)? {
            if let price = prices[region] { return (price, region) }
            return global ? prices[.international].map { ($0, .international) } : nil
        }
    }

    /// An amount of money and its currency; list prices are never converted.
    public struct Cost: Hashable, Sendable {
        public let amount: Decimal
        public let currency: String
    }

    /// What some usage would cost at list price, per currency, and the models that had tokens but no price.
    public struct ListCost: Hashable, Sendable {
        public var amounts: [String: Decimal] = [:]
        public var unpriced: [String] = []

        /// "≈$12.30 · ≈¥45.60": US dollars first, each currency as it was priced.
        public var text: String {
            amounts.keys.sorted { $0 == "USD" || ($1 != "USD" && $0 < $1) }
                .map { "≈" + MoneyFormat.amount(amounts[$0]!, currency: $0) }.joined(separator: " · ")
        }
    }

    /// The catalog name of a consumer id: the model its client called, after the client's `<source>-model:` prefix, without
    /// a dated snapshot suffix or Claude Code's 1M marker. Whichever client made the call, the same model has the same
    /// list price, so Claude Code pointed at DeepSeek's API prices its calls as DeepSeek's.
    static func name(of agentId: String) -> String? {
        guard let prefix = agentId.range(of: "-model:") else { return nil }
        var name = String(agentId[prefix.upperBound...]).lowercased()
        if name.hasSuffix("[1m]") { name.removeLast(4) }
        if let date = name.range(of: #"-[0-9]{8}$|-[0-9]{4}-[0-9]{2}-[0-9]{2}$"#, options: .regularExpression) { name.removeSubrange(date) }
        return name.isEmpty ? nil : name
    }

    public static func model(for agentId: String) -> Model? { name(of: agentId).flatMap { models[$0] } }

    /// What one call would cost at list price on the platform it went through, in that platform's currency: at the tier
    /// its prompt reaches, and at DeepSeek's peak rates when made in Beijing working hours. nil when the model has no
    /// price there.
    public static func cost(agentId: String, kinds: TokenKinds, region: Region = .international, at date: Date? = nil) -> Cost? {
        guard let model = model(for: agentId), let list = model.price(in: region) else { return nil }
        let rates = list.price.rates(prompt: kinds.input + kinds.cacheWrite + kinds.cacheRead)
        return Cost(amount: amount(kinds, at: rates) * factor(model, at: date), currency: list.region.currency)
    }

    /// What calls counted together would cost. A sum says nothing of any one call's prompt, so it is priced at the base
    /// rates; `date` is when the calls were made, for DeepSeek's peak hours.
    public static func cost(agentId: String, summed kinds: TokenKinds, region: Region = .international, at date: Date? = nil) -> Cost? {
        guard let model = model(for: agentId), let list = model.price(in: region) else { return nil }
        return Cost(amount: amount(kinds, at: list.price.rates) * factor(model, at: date), currency: list.region.currency)
    }

    /// Each model's counted tokens at list price, on the platform `region` names for it, added up per currency. `peak` is
    /// the part of those tokens counted in DeepSeek's peak hours. nil when no model with tokens has a price.
    public static func cost(of tokens: [String: TokenKinds], peak: [String: TokenKinds] = [:],
                            region: (String) -> Region = { _ in .international }) -> ListCost? {
        var cost = ListCost(), priced = false
        for (agentId, kinds) in tokens where !kinds.isEmpty {
            guard let model = model(for: agentId), let list = model.price(in: region(agentId)) else {
                cost.unpriced.append(agentId)
                continue
            }
            let busy = model.peakHours ? peak[agentId] ?? TokenKinds() : TokenKinds()
            cost.amounts[list.region.currency, default: 0] += amount(kinds - busy, at: list.price.rates) + amount(busy, at: list.price.rates) * 2
            priced = true
        }
        cost.unpriced.sort()
        return priced ? cost : nil
    }

    private static func amount(_ kinds: TokenKinds, at rates: Rates) -> Decimal {
        (Decimal(kinds.input) * rates.input + Decimal(kinds.cacheWrite) * rates.cacheWrite
            + Decimal(kinds.cacheRead) * rates.cacheRead + Decimal(kinds.output + kinds.reasoning) * rates.output) / 1_000_000
    }

    private static func factor(_ model: Model, at date: Date?) -> Decimal {
        model.peakHours && date.map(isPeak) == true ? 2 : 1
    }

    /// DeepSeek's peak hours: 9:00–12:00 and 14:00–18:00 Beijing time, Monday to Friday. Its Chinese public holidays are
    /// off-peak too; the catalog does not know them, so they count as peak.
    public static func isPeak(_ date: Date) -> Bool {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 8 * 3600)!
        let day = calendar.component(.weekday, from: date), hour = calendar.component(.hour, from: date)
        return (2...6).contains(day) && ((9..<12).contains(hour) || (14..<18).contains(hour))
    }

    /// The context window a call ran in: what the client reported, else the published window. Claude Code does not
    /// report it, and a Claude model runs with 200K or 1M, so a model this Mac has seen hold more than 200K counts as 1M.
    public static func contextWindow(agentId: String, reported: Int?, largestSeen: Int?) -> Int? {
        if let reported, reported > 0 { return reported }
        let published = model(for: agentId)?.contextWindow ?? (agentId.hasPrefix("claude-model:") ? 200_000 : nil)
        guard let published else { return nil }
        return (largestSeen ?? 0) > published ? max(published, 1_000_000) : published
    }
}
