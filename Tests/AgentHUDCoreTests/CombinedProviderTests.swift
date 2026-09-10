import XCTest
@testable import AgentHUDCore

final class CombinedProviderTests: XCTestCase {
    struct Source: UsageProvider {
        let report: UsageReport?
        func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
            guard let report else { throw UsageProviderError("signed out") }
            return report
        }
    }

    private let now = Date(timeIntervalSince1970: 1788768000)

    private func report(_ id: String, tokens: Int) -> UsageReport {
        let sample = TranscriptSession.UsageEvent(timestamp: now.addingTimeInterval(-3600), agentId: id, tokensIn: tokens, tokensOut: 0)
        return UsageReport(generatedAt: now, snapshots: [UsageSnapshot(agentId: id, remainingPct: 80, updatedAt: now)],
                           sessions: [], history: [], activity: .empty, insights: .empty, consumption: [sample],
                           consumerIdsByQuota: [id: ["\(id)-model"]])
    }

    func testOneUnavailableVendorDoesNotHideTheOther() async throws {
        let codex = report("codex", tokens: 300)
        let provider = CombinedUsageProvider([.init("Claude", Source(report: nil)), .init("Codex", Source(report: codex))])
        let combined = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(combined.snapshots, codex.snapshots)
        XCTAssertEqual(combined.sourceNotices, ["Claude": "signed out"])
        XCTAssertEqual(combined.insights.weeklyShare["codex"], 1)
    }

    func testWeeklyShareCombinesRawTokensAcrossVendors() async throws {
        let provider = CombinedUsageProvider([.init("Claude", Source(report: report("claude", tokens: 100))),
                                              .init("Codex", Source(report: report("codex", tokens: 300)))])
        let combined = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(combined.insights.weeklyShare["claude"], 0.25)
        XCTAssertEqual(combined.insights.weeklyShare["codex"], 0.75)
        XCTAssertEqual(combined.consumerIdsByQuota, ["claude": ["claude-model"], "codex": ["codex-model"]])
        XCTAssertEqual(combined.activity.rows.flatMap { $0 }.filter { $0 > 0 }, [1])
        XCTAssertEqual(combined.activity.tokens.flatMap { $0 }.filter { $0 > 0 }, [400], "hover totals include every vendor")
        XCTAssertEqual(combined.activity.tokensByModel.flatMap { $0 }.filter { !$0.isEmpty }, [["claude": 100, "codex": 300]])
    }

    func testClaudeCoverageSurvivesVendorAggregation() async throws {
        let since = now.addingTimeInterval(-30 * 86400)
        let claude = UsageReport(generatedAt: now, snapshots: [], sessions: [], history: [], activity: .empty,
            insights: .empty, claudeConsumptionSince: since)
        let combined = CombinedUsageProvider([.init("Claude", Source(report: claude)), .init("Codex", Source(report: report("codex", tokens: 300)))])
        let result = try await combined.fetchUsage(agents: [], historyHours: 721)
        XCTAssertEqual(result.claudeConsumptionSince, since)
    }

    func testCodexCachedPollDoesNotDuplicateQuotaHistory() async throws {
        let limits = try JSONDecoder().decode(CodexRateLimits.self, from: Data(CodexProviderTests.limits.utf8))
        let history = QuotaHistoryStore(fileURL: nil)
        let now = now
        let provider = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: []),
                                          history: history, clock: { now })
        _ = try await provider.fetchUsage(agents: [], historyHours: 48)
        _ = try await provider.fetchUsage(agents: [], historyHours: 48)
        let count = await history.count
        XCTAssertEqual(count, 3, "one observed sample per real quota window, regardless of session polling")
    }

    func testResetBalanceSurvivesCodexPollingAndVendorAggregation() async throws {
        let limits = try JSONDecoder().decode(CodexRateLimits.self, from: Data(CodexProviderTests.limitsWithResets.utf8))
        let history = QuotaHistoryStore(fileURL: nil)
        let now = now
        let codex = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: []),
                                       history: history, clock: { now })
        let combined = CombinedUsageProvider([.init("Claude", Source(report: nil)), .init("Codex", codex)])
        let first = try await combined.fetchUsage(agents: [], historyHours: 48)
        let cached = try await combined.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(first.codexResetCredits, limits.rateLimitResetCredits)
        XCTAssertEqual(cached.codexResetCredits, first.codexResetCredits)
        XCTAssertEqual(cached.snapshots.count, 3)
        let count = await history.count
        XCTAssertEqual(count, 3, "the reset balance uses the existing quota query and cache")
    }
}
