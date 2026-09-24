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

    func testContextWindowsComeFromTheLogThenTheCatalogThenWhatTheModelHeld() {
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "codex-model:gpt-6-astra", reported: 258_400, largestSeen: nil), 258_400)
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "claude-model:claude-haiku-4-5", reported: nil, largestSeen: 150_000), 200_000)
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "claude-model:claude-next-1", reported: nil, largestSeen: 150_000), 200_000)
        XCTAssertEqual(ModelCatalog.contextWindow(agentId: "claude-model:claude-next-1", reported: nil, largestSeen: 420_000), 1_000_000)
        XCTAssertNil(ModelCatalog.contextWindow(agentId: "codex-model:gpt-6-astra", reported: nil, largestSeen: 420_000))
        XCTAssertNil(ModelCatalog.contextWindow(agentId: "cursor-model:auto", reported: nil, largestSeen: 420_000))
    }
}
