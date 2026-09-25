import XCTest
@testable import AgentHUDCore

final class ModelCatalogTests: XCTestCase {
    func testListPricesFollowTheModelAndThePromptLength() {
        XCTAssertEqual(ModelCatalog.name(of: "claude-model:claude-haiku-4-5-20251001"), "claude-haiku-4-5", "a dated snapshot prices like its alias")
        XCTAssertEqual(ModelCatalog.name(of: "cursor-model:auto"), "auto")
        XCTAssertNil(ModelCatalog.model(for: "cursor-model:auto"), "a client's own routing name has no list price")
        XCTAssertNil(ModelCatalog.cost(agentId: "codex-model:codex-auto-review", kinds: TokenKinds(input: 1)))
        // Fresh input, cache reads and output at $10, $1 and $50 per million, reasoning billed as output.
        XCTAssertEqual(ModelCatalog.cost(agentId: "codex-model:gpt-6-astra",
                                         kinds: TokenKinds(input: 100_000, reasoning: 400, output: 600, cacheRead: 100_000)),
                       .init(amount: Decimal(string: "1.15")!, currency: "USD"))
        // A prompt over 272K tokens costs twice the input and cache rates and one and a half times the output rate.
        XCTAssertEqual(ModelCatalog.cost(agentId: "codex-model:gpt-6-astra",
                                         kinds: TokenKinds(input: 100_000, output: 1_000, cacheRead: 200_000))?.amount, Decimal(string: "2.475"))
        // Claude Code's one-hour cache writes cost twice the input rate.
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:claude-fable-5-1", kinds: TokenKinds(cacheWrite: 1_000_000, cacheRead: 1_000_000))?.amount,
                       Decimal(string: "20.25"))
    }

    func testSummedCountsArePricedAtBaseRatesWithTheUnpricedModelsNamed() throws {
        // Two 200K prompts add up past 272K, yet neither was a long one.
        let astra = TokenKinds(input: 400_000, output: 1_000)
        XCTAssertEqual(ModelCatalog.cost(agentId: "codex-model:gpt-6-astra", summed: astra)?.amount, Decimal(string: "4.05"))
        let cost = try XCTUnwrap(ModelCatalog.cost(of: ["codex-model:gpt-6-astra": astra, "cursor-model:auto": TokenKinds(input: 5),
                                                        "claude-model:claude-next": TokenKinds()]))
        XCTAssertEqual(cost.amounts, ["USD": Decimal(string: "4.05")!])
        XCTAssertEqual(cost.unpriced, ["cursor-model:auto"], "a model without tokens is not named")
        XCTAssertNil(ModelCatalog.cost(of: ["cursor-model:auto": TokenKinds(input: 5)]))
        XCTAssertEqual(TokenDimensions.fresh.masking(TokenKinds(input: 1, cacheRead: 9)), TokenKinds(input: 1))
        XCTAssertEqual(TokenKinds(cacheWrite: 10, input: 10, cacheRead: 80).cacheHitRate, 0.8)
        XCTAssertNil(TokenKinds(input: 10, output: 5).cacheHitRate, "a log that counts no cache says nothing about hits")
    }

    func testEachPlatformPricesInItsOwnCurrencyAndTiers() {
        let short = TokenKinds(input: 10_000, output: 1_000), long = TokenKinds(input: 40_000, output: 1_000)
        // BigModel charges more from 32K prompt tokens; Z.ai has one rate.
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:glm-5.1", kinds: short, region: .china),
                       .init(amount: Decimal(string: "0.084")!, currency: "CNY"))
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:glm-5.1", kinds: long, region: .china)?.amount, Decimal(string: "0.348"))
        XCTAssertEqual(ModelCatalog.cost(agentId: "opencode-model:glm-5.1", kinds: short),
                       .init(amount: Decimal(string: "0.0184")!, currency: "USD"))
        XCTAssertNil(ModelCatalog.cost(agentId: "claude-model:glm-5-turbo", kinds: short), "a model one platform does not sell has no price there")
        XCTAssertNil(ModelCatalog.cost(agentId: "claude-model:glm-4.6", kinds: short, region: .china))
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:claude-haiku-4-5", kinds: TokenKinds(input: 1_000_000), region: .china),
                       .init(amount: 1, currency: "USD"), "a vendor with one list charges it everywhere")
        // Alibaba's tiers follow the prompt; a dated snapshot prices like its model.
        XCTAssertEqual(ModelCatalog.cost(agentId: "qwen-model:qwen3-coder-plus-2025-09-23", kinds: TokenKinds(input: 50_000, output: 1_000))?.amount,
                       Decimal(string: "0.099"))
        // xAI doubles every token of a call from 200K prompt tokens.
        XCTAssertEqual(ModelCatalog.cost(agentId: "grok-model:grok-4.6", kinds: TokenKinds(input: 200_000))?.amount, Decimal(string: "0.8"))
        XCTAssertEqual(ModelCatalog.cost(agentId: "grok-model:grok-code-fast-1", summed: TokenKinds(input: 1_000_000))?.amount, 1,
                       "a retired name prices as the model that serves it")
    }

    func testDeepSeekChargesTwiceInBeijingWorkingHours() throws {
        let formatter = ISO8601DateFormatter()
        for (instant, peak) in [("2026-09-07T00:59:59Z", false), ("2026-09-07T01:00:00Z", true), ("2026-09-07T04:00:00Z", false),
                                ("2026-09-07T06:00:00Z", true), ("2026-09-07T10:00:00Z", false), ("2026-09-12T02:00:00Z", false)] {
            XCTAssertEqual(ModelCatalog.isPeak(formatter.date(from: instant)!), peak, instant)
        }
        let friday = formatter.date(from: "2026-09-25T02:00:00Z")!, saturday = formatter.date(from: "2026-09-26T02:00:00Z")!
        let million = TokenKinds(input: 1_000_000)
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:deepseek-v4-pro", kinds: million, region: .china, at: saturday)?.amount, Decimal(string: "4.5"))
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:deepseek-v4-pro", kinds: million, region: .china, at: friday)?.amount, 9)
        XCTAssertEqual(ModelCatalog.cost(agentId: "deepseek-model:deepseek-v4-flash", kinds: million, at: saturday),
                       .init(amount: Decimal(string: "0.15")!, currency: "USD"), "the retired Flash name bills at Flash's rate")
        // Counted usage splits its peak part out; each currency adds up on its own.
        let cost = try XCTUnwrap(ModelCatalog.cost(of: ["claude-model:deepseek-v4-pro": TokenKinds(input: 2_000_000),
                                                        "codex-model:gpt-6-sol": TokenKinds(input: 1_000_000)],
                                                   peak: ["claude-model:deepseek-v4-pro": million, "codex-model:gpt-6-sol": million],
                                                   region: { $0.hasPrefix("claude") ? .china : .international }))
        XCTAssertEqual(cost.amounts, ["CNY": Decimal(string: "13.5")!, "USD": 2])
        XCTAssertTrue(cost.text.hasPrefix("≈$"), cost.text)
    }

    func testContextWindowsComeFromTheLogThenTheCatalogThenWhatTheModelHeld() {
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "codex-model:gpt-6-astra", reported: 258_400, largestSeen: nil), 258_400)
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "claude-model:claude-haiku-4-5", reported: nil, largestSeen: 150_000), 200_000)
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "claude-model:claude-next-1", reported: nil, largestSeen: 150_000), 200_000)
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "claude-model:claude-next-1", reported: nil, largestSeen: 420_000), 1_000_000)
        XCTAssertNil(ModelCatalog.contextWindow(agentId: "codex-model:gpt-6-astra", reported: nil, largestSeen: 420_000))
        XCTAssertNil(ModelCatalog.contextWindow(agentId: "cursor-model:auto", reported: nil, largestSeen: 420_000))
    }
}
