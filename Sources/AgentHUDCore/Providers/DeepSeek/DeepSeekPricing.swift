import Foundation

/// The price of a DeepSeek Harness call on the account it was billed to, from the model catalog: yuan on a Chinese
/// account, US dollars on an international one. Estimates are local Harness usage, not account invoices.
public enum DeepSeekPricing {
    public static let checkedOn = ModelCatalog.checkedOn
    public static let sourceURL = URL(string: "https://api-docs.deepseek.com/quick_start/pricing/")!

    public static func isPeak(_ date: Date) -> Bool { ModelCatalog.isPeak(date) }

    public static func estimate(_ usage: DeepSeekTranscript.Usage, currency: String) -> Decimal? {
        guard usage.provider == "deepseek-official",
              let region = ModelCatalog.Region.allCases.first(where: { $0.currency == currency }) else { return nil }
        let kinds = TokenKinds(tokensIn: usage.input, tokensOut: usage.output, cacheRead: usage.cachedInput,
                               cacheWrite: usage.cacheWrite, reasoning: usage.reasoning)
        return ModelCatalog.cost(agentId: "deepseek-model:" + usage.model, kinds: kinds, region: region, at: usage.requestedAt)?.amount
    }
}
