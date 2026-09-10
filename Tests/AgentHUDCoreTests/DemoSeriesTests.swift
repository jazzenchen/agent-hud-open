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
    func testSeriesMatchesPrototypeValues() {
        let s = DemoSeries.series(count: 24, seed: 11)
        XCTAssertEqual(s.count, 24)
        XCTAssertEqual(s[0].open, 100)
        let closes = s.prefix(6).map { ($0.close * 1e6).rounded() / 1e6 }
        XCTAssertEqual(closes, [86.204769, 71.660324, 56.402685, 52.681852, 100.0, 84.065972])
    }

    func testResetsEveryFifthBucket() {
        let s = DemoSeries.series(count: 48, seed: 5)
        XCTAssertEqual(s[4].close, 100)
        XCTAssertEqual(s[9].close, 100)
        XCTAssertEqual(s[0].close, 86.627778, accuracy: 1e-6)
        for candle in s {
            XCTAssertGreaterThanOrEqual(candle.close, 2)
            XCTAssertLessThanOrEqual(candle.close, 100)
            XCTAssertGreaterThanOrEqual(candle.high, max(candle.open, candle.close))
            XCTAssertLessThanOrEqual(candle.low, min(candle.open, candle.close))
        }
    }

    func testHourlyTokensShapeAndScale() {
        let t = DemoSeries.hourlyTokens(agentCount: 4, hours: 48)
        XCTAssertEqual(t.count, 48)
        XCTAssertEqual(t[0].count, 4)
        XCTAssertLessThanOrEqual(t.flatMap { $0 }.max() ?? 0, 40)
        XCTAssertGreaterThan(t.flatMap { $0 }.reduce(0, +), 0)
    }

    func testActivityGridIsSevenByTwentyFour() {
        let grid = DemoSeries.activity()
        XCTAssertEqual(grid.rows.count, 7)
        XCTAssertTrue(grid.rows.allSatisfy { $0.count == 24 })
        XCTAssertTrue(grid.rows.flatMap { $0 }.allSatisfy { $0 >= 0 && $0 <= 1 })
        XCTAssertEqual(grid.rows.flatMap { $0 }.max() ?? 0, 1, accuracy: 1e-9)
    }

    func testLineSeeds() {
        XCTAssertEqual((0..<4).map(DemoSeries.lineSeed), [5, 17, 29, 41])
        XCTAssertNotEqual(DemoSeries.lineSeed(index: 4), DemoSeries.lineSeed(index: 5))
    }
}

final class DemoUsageProviderTests: XCTestCase {
    func testReportCoversEnabledAgentsAndHours() async throws {
        let report = try await DemoUsageProvider().fetchUsage(agents: DemoData.agents, historyHours: 48)
        let enabled = DemoData.agents.filter(\.enabled)
        XCTAssertEqual(report.history.count, 48 * enabled.count)
        XCTAssertEqual(report.snapshots.count, DemoData.agents.count)
        XCTAssertEqual(report.sessions.filter(\.isLive).count, 2)
        XCTAssertEqual(report.snapshot(for: "codex")?.remainingPct, 7)
        XCTAssertEqual(report.insights.weeklyShare.values.reduce(0, +), 1, accuracy: 1e-9)
        let opus = report.history(for: "claude-opus")
        XCTAssertEqual(opus.count, 48)
        XCTAssertEqual(opus.first?.remainingStart, 100)
        XCTAssertTrue(zip(opus, opus.dropFirst()).allSatisfy { $0.hourStart < $1.hourStart })
    }

    func testDifferentRangesProduceDifferentLengths() async throws {
        let short = try await DemoUsageProvider().fetchUsage(agents: DemoData.agents, historyHours: 48)
        let long = try await DemoUsageProvider().fetchUsage(agents: DemoData.agents, historyHours: 168)
        XCTAssertEqual(long.history(for: "codex").count, 168)
        XCTAssertEqual(short.history(for: "codex").count, 48)
    }
}
