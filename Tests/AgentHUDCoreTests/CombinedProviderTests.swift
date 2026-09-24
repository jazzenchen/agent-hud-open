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
        let bucket = UsageBucket(start: now.addingTimeInterval(-3600), agentId: id, tokensIn: tokens, tokensOut: 0)
        return UsageReport(generatedAt: now, snapshots: [UsageSnapshot(agentId: id, remainingPct: 80, updatedAt: now)],
                           sessions: [], usage: [bucket],
                           consumerIdsByQuota: [id: ["\(id)-model"]])
    }

    func testVendorsAreReadOneAtATime() async throws {
        actor Tracker {
            private var active = 0
            private(set) var peak = 0
            func run() async {
                active += 1
                peak = max(peak, active)
                try? await Task.sleep(for: .milliseconds(20))
                active -= 1
            }
        }
        struct Slow: UsageProvider {
            let tracker: Tracker
            let report: UsageReport
            func refreshAccountUsage(historyHours: Int) async { await tracker.run() }
            func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
                await tracker.run()
                return report
            }
        }
        let tracker = Tracker()
        let provider = CombinedUsageProvider((0..<4).map { .init("Vendor \($0)", Slow(tracker: tracker, report: report("v\($0)", tokens: 1))) })
        XCTAssertEqual(provider.accountRefreshSteps.count, 4, "each vendor's account refresh is its own step")
        XCTAssertNil(provider.watchedDirectories, "a vendor that cannot name its directories keeps every poll reading")
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let peak = await tracker.peak
        XCTAssertEqual(peak, 1)
    }

    func testOneUnavailableVendorDoesNotHideTheOther() async throws {
        let codex = report("codex", tokens: 300)
        let provider = CombinedUsageProvider([.init("Claude", Source(report: nil)), .init("Codex", Source(report: codex))])
        let combined = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        XCTAssertEqual(combined.snapshots, codex.snapshots)
        XCTAssertEqual(combined.sourceNotices, ["Claude": "signed out"])
        XCTAssertEqual(combined.usage, codex.usage)
    }

    @MainActor
    func testPeriodsAndActivityCombineRawTokensAcrossVendors() async throws {
        let provider = CombinedUsageProvider([.init("Claude", Source(report: report("claude", tokens: 100))),
                                              .init("Codex", Source(report: report("codex", tokens: 300)))])
        let combined = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        XCTAssertEqual(combined.consumerIdsByQuota, ["claude": ["claude-model"], "codex": ["codex-model"]])
        let suite = "CombinedProviderTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: provider, settings: SettingsStore(defaults: defaults))
        store.replace(report: combined)
        XCTAssertEqual(combined.periods?.tokens[.days7]?.mapValues(\.total), ["claude": 100, "codex": 300],
                       "usage a source reports itself joins the ledger's periods")
        XCTAssertEqual(combined.periods?.tokens[.days30], combined.periods?.tokens[.days7])
        XCTAssertEqual(store.statsActivity.rows.flatMap { $0 }.filter { $0 > 0 }, [1])
        XCTAssertEqual(store.statsActivity.tokens.flatMap { $0 }.filter { $0 > 0 }, [400], "hover totals include every vendor")
        XCTAssertEqual(store.statsActivity.tokensByModel.flatMap { $0 }.filter { !$0.isEmpty }, [["claude": 100, "codex": 300]])
        store.stop()
    }

    func testRecordedUsageSurvivesAFailedRefresh() async throws {
        let ledger = UsageLedger.inMemory(), now = Date()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-combined-\(UUID().uuidString)/-p", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root.deletingLastPathComponent()) }
        let stamp = ISO8601DateFormatter().string(from: now.addingTimeInterval(-60))
        try (#"{"sessionId":"s","type":"assistant","message":{"id":"m","role":"assistant","model":"claude-opus-5","usage":{"input_tokens":7,"output_tokens":3}},"timestamp":"\#(stamp)"}"# + "\n")
            .write(to: root.appendingPathComponent("s.jsonl"), atomically: true, encoding: .utf8)
        let claude = ClaudeCodeProvider(engine: nil, transcripts: ClaudeTranscriptStore(roots: [root.deletingLastPathComponent()], ledger: ledger),
                                        history: QuotaHistoryStore(), clock: { now })
        let first = try await CombinedUsageProvider([.init("Claude", claude)], ledger: ledger).fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(first.usage.map(\.tokensIn), [7])
        let failing = CombinedUsageProvider([.init("Claude", Source(report: nil)), .init("Codex", Source(report: report("codex", tokens: 300)))], ledger: ledger)
        let second = try await failing.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(Set(second.usage.map(\.agentId)), ["claude-model:claude-opus-5", "codex"], "the ledger keeps what a failing source recorded")
    }

    func testCodexCachedPollDoesNotDuplicateQuotaHistory() async throws {
        let limits = try JSONDecoder().decode(CodexRateLimits.self, from: Data(CodexProviderTests.limits.utf8))
        let history = QuotaHistoryStore()
        let now = now
        let provider = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: []),
                                          history: history, clock: { now })
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let count = await history.count
        XCTAssertEqual(count, 3, "one observed sample per real quota window, regardless of session polling")
    }

    func testResetBalanceSurvivesCodexPollingAndVendorAggregation() async throws {
        let limits = try JSONDecoder().decode(CodexRateLimits.self, from: Data(CodexProviderTests.limitsWithResets.utf8))
        let history = QuotaHistoryStore()
        let now = now
        let codex = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: []),
                                       history: history, clock: { now })
        let combined = CombinedUsageProvider([.init("Claude", Source(report: nil)), .init("Codex", codex)])
        let first = try await combined.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let cached = try await combined.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        XCTAssertEqual(first.codexResetCredits, limits.rateLimitResetCredits)
        XCTAssertEqual(cached.codexResetCredits, first.codexResetCredits)
        XCTAssertEqual(cached.snapshots.count, 3)
        let count = await history.count
        XCTAssertEqual(count, 3, "the reset balance uses the existing quota query and cache")
    }

    func testCodexProviderAndCombinedReportPreserveResetObservationWithoutWindows() async throws {
        let limits = try JSONDecoder().decode(CodexRateLimits.self,
            from: Data(#"{"rateLimitsByLimitId":{},"rateLimitResetCredits":{"availableCount":3}}"#.utf8))
        let observed = now.addingTimeInterval(-120)
        let source = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: []),
                                        history: QuotaHistoryStore(), clock: { observed })
        let combined = CombinedUsageProvider([.init("Codex", source)])
        let result = try await combined.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertTrue(result.snapshots.isEmpty)
        XCTAssertEqual(result.codexResetCreditsObservedAt, observed)
    }
}
