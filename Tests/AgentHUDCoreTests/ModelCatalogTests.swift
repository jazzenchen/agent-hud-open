import XCTest
@testable import AgentHUDCore

final class ModelCatalogTests: XCTestCase {
    func testListPricesFollowTheModelAndThePromptLength() {
        XCTAssertEqual(ModelCatalog.name(of: "claude-model:claude-haiku-4-5-20251001"), "claude-haiku-4-5", "a dated snapshot prices like its alias")
        XCTAssertNil(ModelCatalog.name(of: "cursor-model:auto"), "only Claude Code's and Codex's models have list prices")
        XCTAssertNil(ModelCatalog.cost(agentId: "codex-model:codex-auto-review", kinds: TokenKinds(input: 1)))
        // Fresh input, cache reads and output at $10, $1 and $50 per million, reasoning billed as output.
        XCTAssertEqual(ModelCatalog.cost(agentId: "codex-model:gpt-6-astra",
                                         kinds: TokenKinds(input: 100_000, reasoning: 400, output: 600, cacheRead: 100_000)), Decimal(string: "1.15"))
        // A prompt over 272K tokens costs twice the input and cache rates and one and a half times the output rate.
        XCTAssertEqual(ModelCatalog.cost(agentId: "codex-model:gpt-6-astra",
                                         kinds: TokenKinds(input: 100_000, output: 1_000, cacheRead: 200_000)), Decimal(string: "2.475"))
        // Claude Code's one-hour cache writes cost twice the input rate.
        XCTAssertEqual(ModelCatalog.cost(agentId: "claude-model:claude-fable-5-1", kinds: TokenKinds(cacheWrite: 1_000_000, cacheRead: 1_000_000)),
                       Decimal(string: "20.25"))
    }

    func testSummedCountsArePricedAtBaseRatesWithTheUnpricedModelsNamed() throws {
        // Two 200K prompts add up past 272K, yet neither was a long one.
        let astra = TokenKinds(input: 400_000, output: 1_000)
        XCTAssertEqual(ModelCatalog.cost(agentId: "codex-model:gpt-6-astra", summed: astra), Decimal(string: "4.05"))
        let cost = try XCTUnwrap(ModelCatalog.cost(of: ["codex-model:gpt-6-astra": astra, "cursor-model:auto": TokenKinds(input: 5),
                                                        "claude-model:claude-next": TokenKinds()]))
        XCTAssertEqual(cost.amount, Decimal(string: "4.05"))
        XCTAssertEqual(cost.unpriced, ["cursor-model:auto"], "a model without tokens is not named")
        XCTAssertNil(ModelCatalog.cost(of: ["cursor-model:auto": TokenKinds(input: 5)]))
        XCTAssertEqual(TokenDimensions.fresh.masking(TokenKinds(input: 1, cacheRead: 9)), TokenKinds(input: 1))
        XCTAssertEqual(TokenKinds(cacheWrite: 10, input: 10, cacheRead: 80).cacheHitRate, 0.8)
        XCTAssertNil(TokenKinds(input: 10, output: 5).cacheHitRate, "a log that counts no cache says nothing about hits")
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
