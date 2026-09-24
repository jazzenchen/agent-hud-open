import Foundation

/// API list prices and context windows of the models Claude Code and Codex call, for an estimate of what a session
/// would cost at the vendor's API and how full its context is. Prices are US dollars per million tokens. A model
/// missing here has no estimate; nothing is guessed from its name.
public enum ModelCatalog {
    public struct Rates: Hashable, Sendable {
        public let input: Decimal
        public let cacheWrite: Decimal
        public let cacheRead: Decimal
        public let output: Decimal
    }

    public struct Model: Hashable, Sendable {
        public let rates: Rates
        /// Rates of a call whose prompt is longer than `longContextAbove` tokens.
        public let longRates: Rates?
        public let longContextAbove: Int?
        /// The published window; nil where the client reports it with every call.
        public let contextWindow: Int?
    }

    /// Claude Code writes its prompt cache with the one-hour lifetime, billed at twice the input rate. Cache reads cost a
    /// tenth of input unless the model has its own rate.
    private static func claude(_ input: String, _ output: String, read: String? = nil, window: Int) -> Model {
        let input = decimal(input)
        return Model(rates: Rates(input: input, cacheWrite: input * 2, cacheRead: read.map(decimal) ?? input / 10, output: decimal(output)),
                     longRates: nil, longContextAbove: nil, contextWindow: window)
    }

    /// OpenAI bills cache writes as input. Prompts over 272K tokens cost twice the input and cache rates and one and a half
    /// times the output rate on the models that accept them.
    private static func openAI(_ input: String, _ read: String, _ output: String, long: Bool = true) -> Model {
        let rates = Rates(input: decimal(input), cacheWrite: decimal(input), cacheRead: decimal(read), output: decimal(output))
        let longRates = Rates(input: rates.input * 2, cacheWrite: rates.cacheWrite * 2, cacheRead: rates.cacheRead * 2,
                              output: rates.output * 3 / 2)
        return Model(rates: rates, longRates: long ? longRates : nil, longContextAbove: long ? 272_000 : nil, contextWindow: nil)
    }

    private static func decimal(_ text: String) -> Decimal { Decimal(string: text, locale: Locale(identifier: "en_US_POSIX"))! }

    static let models: [String: Model] = [
        "claude-fable-5-1": claude("10", "50", read: "0.25", window: 1_000_000),
        "claude-mythos-5-1": claude("10", "50", read: "0.25", window: 1_000_000),
        "claude-fable-5": claude("10", "50", window: 1_000_000),
        "claude-mythos-5": claude("10", "50", window: 1_000_000),
        "claude-opus-5-5": claude("4", "20", read: "0.20", window: 1_000_000),
        "claude-opus-5": claude("5", "25", window: 1_000_000),
        "claude-opus-4-8": claude("5", "25", window: 1_000_000),
        "claude-opus-4-7": claude("5", "25", window: 1_000_000),
        "claude-opus-4-6": claude("5", "25", window: 1_000_000),
        "claude-opus-4-5": claude("5", "25", window: 200_000),
        "claude-opus-4-1": claude("15", "75", window: 200_000),
        "claude-opus-4": claude("15", "75", window: 200_000),
        "claude-opus-4-0": claude("15", "75", window: 200_000),
        "claude-sonnet-5": claude("2", "10", window: 1_000_000),
        "claude-sonnet-4-6": claude("3", "15", window: 1_000_000),
        "claude-sonnet-4-5": claude("3", "15", window: 200_000),
        "claude-sonnet-4": claude("3", "15", window: 200_000),
        "claude-sonnet-4-0": claude("3", "15", window: 200_000),
        "claude-haiku-4-5": claude("1", "5", window: 200_000),
        "claude-3-5-haiku": claude("0.80", "4", window: 200_000),
        "gpt-6-astra": openAI("10", "1", "50"),
        "gpt-6-sol": openAI("2", "0.20", "10"),
        "gpt-6-luna": openAI("0.10", "0.01", "0.50"),
        "gpt-5.6-sol": openAI("4", "0.40", "20"),
        "gpt-5.6-terra": openAI("2", "0.20", "12"),
        "gpt-5.6-luna": openAI("0.20", "0.02", "1.20"),
        "gpt-5.5": openAI("5", "0.50", "30"),
        "gpt-5.4": openAI("2.50", "0.25", "15"),
        "gpt-5.3-codex": openAI("1.75", "0.175", "14", long: false),
    ]

    /// The catalog name of a consumer id: Claude Code's and Codex's models, without a dated snapshot suffix.
    static func name(of agentId: String) -> String? {
        for prefix in ["claude-model:", "codex-model:"] where agentId.hasPrefix(prefix) {
            var name = String(agentId.dropFirst(prefix.count)).lowercased()
            if name.hasSuffix("[1m]") { name.removeLast(4) }
            if let date = name.range(of: #"-[0-9]{8}$"#, options: .regularExpression) { name.removeSubrange(date) }
            return name
        }
        return nil
    }

    public static func model(for agentId: String) -> Model? { name(of: agentId).flatMap { models[$0] } }

    /// What one call would cost at list price; nil when its model has none.
    public static func cost(agentId: String, kinds: TokenKinds) -> Decimal? {
        guard let model = model(for: agentId) else { return nil }
        let prompt = kinds.input + kinds.cacheWrite + kinds.cacheRead
        let rates = model.longContextAbove.map { prompt > $0 } == true ? model.longRates ?? model.rates : model.rates
        let micro = Decimal(kinds.input) * rates.input + Decimal(kinds.cacheWrite) * rates.cacheWrite
            + Decimal(kinds.cacheRead) * rates.cacheRead + Decimal(kinds.output + kinds.reasoning) * rates.output
        return micro / 1_000_000
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
