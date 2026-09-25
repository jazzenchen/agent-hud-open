import XCTest
@testable import AgentHUDCore

final class AgentUsageTests: XCTestCase {
    private let base = Date(timeIntervalSince1970: 1_800_000_000 - 1_800_000_000.truncatingRemainder(dividingBy: 900))

    func testAgentsModelsAndSessionsRankByTokensWithWhatTheyCostAndWhereTheMoneyWent() {
        let consumers = [("claude-model:claude-opus-5", "Claude"), ("claude-model:claude-haiku-4-5", "Claude"), ("codex-model:gpt-6-sol", "Codex"),
                         ("cursor-model:auto", "Cursor")].map { AgentDescriptor(id: $0.0, vendor: $0.1, model: $0.0, source: "", enabled: true) }
        let usage = [
            // Opus 5: $5 input, $0.50 cache reads, $25 output; Haiku 4.5 $1 input; GPT-6 Sol $2 input.
            UsageBucket(start: base, agentId: "claude-model:claude-opus-5", tokensIn: 1_000_000, tokensOut: 100_000, cacheReadTokens: 10_000_000),
            UsageBucket(start: base, agentId: "claude-model:claude-haiku-4-5", tokensIn: 1_000_000, tokensOut: 0),
            UsageBucket(start: base, agentId: "codex-model:gpt-6-sol", tokensIn: 1_000_000, tokensOut: 0),
            UsageBucket(start: base, agentId: "cursor-model:auto", tokensIn: 5_000_000, tokensOut: 0),
            UsageBucket(start: base.addingTimeInterval(-86_400), agentId: "codex-model:gpt-6-sol", tokensIn: 90_000_000, tokensOut: 0),
        ]
        func session(_ id: String, input: Int, recached: Int? = nil) -> (LiveSession, SessionUsage) {
            let tokens = SessionUsage.Tokens(tokensIn: input, tokensOut: 0, cacheReadTokens: 0, cacheWriteTokens: input)
            return (LiveSession(id: id, agentId: "claude-model:claude-opus-5", task: id, terminal: nil, startedAt: base, pctOfWindow: nil,
                                tokensIn: input, tokensOut: 0),
                    SessionUsage(models: [.init(agentId: "claude-model:claude-opus-5", tokens: tokens)],
                                 periods: [.init(start: base, agentId: "claude-model:claude-opus-5", tokens: tokens)], calls: 1,
                                 turns: [.init(start: base, end: base, tokens: tokens, calls: 1, recachedTokens: recached)]))
        }
        let sessions = [session("small", input: 100_000, recached: 200_000), session("large", input: 300_000)]
        let agents = AgentUsage.build(usage: usage, consumers: consumers, sessions: sessions.map(\.0),
                                      breakdowns: Dictionary(uniqueKeysWithValues: sessions.map { ($0.0.id, $0.1) }), vendor: { _ in "Claude" },
                                      interval: DateInterval(start: base, duration: 5 * 3600), dimensions: .fresh, region: { _ in .international })
        XCTAssertEqual(agents.map(\.vendor), ["Cursor", "Claude", "Codex"], "tokens rank, whether or not they have a price")
        let claude = agents[1]
        XCTAssertEqual(claude.models.map(\.agentId), ["claude-model:claude-opus-5", "claude-model:claude-haiku-4-5"])
        XCTAssertEqual(claude.cost?.amounts["USD"], Decimal(string: "13.5"))
        XCTAssertEqual(try XCTUnwrap(claude.cacheReadShare), 5 / 13.5, accuracy: 0.0001, "$5 of $13.50 read context back")
        XCTAssertEqual(claude.sessions.map(\.id), ["large", "small"])
        XCTAssertEqual(claude.recachedCost, 2, "200K written to the cache again at $10 a million")
        XCTAssertEqual(agents[2].tokens.input, 1_000_000, "a bucket before the range is left out")
        XCTAssertNil(agents[0].cost)
    }
}
