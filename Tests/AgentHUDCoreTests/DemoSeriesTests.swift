import XCTest
@testable import AgentHUDCore

final class SeededRandomTests: XCTestCase {
    func testMatchesPrototypeLCG() {
        var r = SeededRandom(seed: 11)
        XCTAssertEqual(r.next(), 0.6498971193415638, accuracy: 1e-12)
        XCTAssertEqual(r.next(), 0.9044281550068587, accuracy: 1e-12)
        XCTAssertEqual(r.next(), 0.297590877914952, accuracy: 1e-12)
    }

    func testIsDeterministic() {
        var a = SeededRandom(seed: 5), b = SeededRandom(seed: 5)
        for _ in 0..<50 { XCTAssertEqual(a.next(), b.next()) }
    }
}

final class DemoSeriesTests: XCTestCase {
    func testHourlyTokensShapeAndScale() {
        let t = DemoSeries.hourlyTokens(agentCount: 4, hours: 48)
        XCTAssertEqual(t.count, 48)
        XCTAssertEqual(t[0].count, 4)
        XCTAssertLessThanOrEqual(t.flatMap { $0 }.max() ?? 0, 40)
        XCTAssertGreaterThan(t.flatMap { $0 }.reduce(0, +), 0)
    }
}

final class DemoUsageProviderTests: XCTestCase {
    func testReportCoversEnabledAgentsAndHours() async throws {
        let report = try await DemoUsageProvider().fetchUsage(agents: DemoData.agents, historyHours: 48)
        XCTAssertEqual(report.snapshots.count, DemoData.agents.count)
        XCTAssertEqual(report.sessions.filter(\.isLive).count, 2)
        XCTAssertEqual(report.snapshot(for: "codex")?.remainingPct, 7)
    }

    func testTheDemoFillsTheMonthTheChartsCanShow() async throws {
        let short = try await DemoUsageProvider().fetchUsage(agents: DemoData.agents, historyHours: 48)
        let long = try await DemoUsageProvider().fetchUsage(agents: DemoData.agents, historyHours: 168)
        func earliest(_ report: UsageReport) -> Date? { report.usage.filter { $0.agentId == "codex" }.map(\.start).min() }
        let month = try XCTUnwrap(earliest(short))
        XCTAssertEqual(month, try XCTUnwrap(earliest(long)), "whatever the history, the demo reaches back a month")
        XCTAssertEqual(short.generatedAt.timeIntervalSince(month) / 3600, Double(StatsRange.days30.hours), accuracy: 2)
    }
}
