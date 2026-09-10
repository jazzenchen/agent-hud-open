import XCTest
@testable import AgentHUDCore

final class RetainedUsageProviderTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testPartialFailureKeepsQuotaBalanceCreditsAndObservationTimes() async throws {
        let good = report(at: now, remaining: 64, balance: 12, credits: 2)
        let partial = report(at: now.addingTimeInterval(3600), remaining: nil, balance: nil, credits: nil)
        let provider = RetainedUsageProvider(provider: SequenceProvider([good, partial]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let result = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(result.snapshots, good.snapshots)
        XCTAssertEqual(result.billing.first?.balances, good.billing.first?.balances)
        XCTAssertEqual(result.billing.first?.updatedAt, now)
        XCTAssertEqual(result.codexResetCredits?.availableCount, 2)
        XCTAssertEqual(result.codexResetCreditsObservedAt, now)
        XCTAssertEqual(result.subscriptions["Codex"], "Pro")
    }

    func testSuccessfulZeroReplacesPreviousValues() async throws {
        let provider = RetainedUsageProvider(provider: SequenceProvider([
            report(at: now, remaining: 64, balance: 12, credits: 2),
            report(at: now.addingTimeInterval(3600), remaining: 0, balance: 0, credits: 0)
        ]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let result = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(result.snapshots.first?.remainingPct, 0)
        XCTAssertEqual(result.snapshots.first?.updatedAt, now.addingTimeInterval(3600))
        XCTAssertEqual(result.billing.first?.balances.first?.total, 0)
        XCTAssertEqual(result.codexResetCredits?.availableCount, 0)
    }

    func testOfflineRestartRestoresTheLastSuccessfulReport() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("report.json")
        let good = report(at: now, remaining: 64, balance: 12, credits: 2)
        let first = RetainedUsageProvider(provider: SequenceProvider([good]), cacheURL: file)
        _ = try await first.fetchUsage(agents: [], historyHours: 24)
        let restarted = RetainedUsageProvider(provider: SequenceProvider([]), cacheURL: file)
        XCTAssertEqual(restarted.initialReport, good)
        let offline = try await restarted.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(offline, good)
    }

    private func report(at date: Date, remaining: Double?, balance: Decimal?, credits: Int?) -> UsageReport {
        let descriptor = AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: "", enabled: true)
        return UsageReport(generatedAt: date,
            snapshots: remaining.map { [.init(agentId: "codex", remainingPct: $0, resetAt: now.addingTimeInterval(60), updatedAt: date)] } ?? [],
            sessions: [], history: [], activity: UsageAnalytics.activityGrid(usage: [], since: date, calendar: .current),
            insights: .empty, discoveredAgents: [descriptor], subscriptions: remaining == nil ? [:] : ["Codex": "Pro"],
            sourceNotices: remaining == nil ? ["Codex": "offline"] : [:],
            billing: [.init(vendor: "DeepSeek", balances: balance.map { [.init(currency: "CNY", total: $0, granted: 0, toppedUp: $0)] } ?? [],
                isAvailable: balance.map { $0 > 0 }, updatedAt: balance == nil ? nil : date, costs: [], notice: nil)],
            codexResetCredits: credits.map { .init(availableCount: $0, credits: nil) },
            codexResetCreditsObservedAt: credits == nil ? nil : date)
    }
}

private actor SequenceProvider: UsageProvider {
    var reports: [UsageReport]
    init(_ reports: [UsageReport]) { self.reports = reports }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) throws -> UsageReport {
        guard !reports.isEmpty else { throw UsageProviderError("offline") }
        return reports.removeFirst()
    }
}
