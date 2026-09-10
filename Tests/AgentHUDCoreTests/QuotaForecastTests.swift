import XCTest
@testable import AgentHUDCore

final class QuotaForecastTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    override func setUp() {
        super.setUp()
        L10n.setLanguage(.zhHans)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    func testCycleConsumptionPredictsHoursAndMinutesInBothLanguages() throws {
        let samples = [
            QuotaSample(agentId: "window", timestamp: now.addingTimeInterval(-1800), remainingPct: 82),
            QuotaSample(agentId: "window", timestamp: now, remainingPct: 67),
        ]
        let snapshot = snapshot(remaining: 67)
        let rate = try XCTUnwrap(UsageAnalytics.burnRate(samples: samples, cycle: snapshot.cycle, now: now))
        let forecast = insights(rate: rate, remaining: 67)
        XCTAssertEqual(QuotaForecast.hint(snapshot: snapshot, insights: forecast, now: now),
                       "耗尽 ~2小时14分")
        L10n.setLanguage(.en)
        XCTAssertEqual(QuotaForecast.hint(snapshot: snapshot, insights: forecast, now: now),
                       "Exhausts ~2h 14m")
    }

    func testHoverKeepsExhaustionEstimateEvenWhenResetComesFirst() throws {
        let forecast = insights(rate: BurnRate(pctPerHour: 30), remaining: 60)
        for resetIn in [3600.0, 7200.0] {
            L10n.setLanguage(.zhHans)
            let hint = try XCTUnwrap(QuotaForecast.hint(snapshot: snapshot(remaining: 60, resetIn: resetIn),
                                                      insights: forecast, now: now))
            XCTAssertEqual(hint, "耗尽 ~2小时")
            L10n.setLanguage(.en)
            XCTAssertEqual(QuotaForecast.hint(snapshot: snapshot(remaining: 60, resetIn: resetIn), insights: forecast, now: now),
                           "Exhausts ~2h")
        }
    }

    func testUntimedQuotaHasNoExhaustionHint() {
        let snapshot = UsageSnapshot(agentId: "api", remainingPct: 20, updatedAt: now)
        XCTAssertNil(QuotaForecast.hint(snapshot: snapshot, insights: insights(rate: .init(pctPerHour: 10), remaining: 20), now: now))
    }

    func testInsufficientOrFlatSamplesDoNotInventAnETA() throws {
        let last = QuotaSample(agentId: "window", timestamp: now, remainingPct: 67)
        let first = QuotaSample(agentId: "window", timestamp: now.addingTimeInterval(-1800), remainingPct: 67)
        for (samples, expected) in [([last], "记录不足"), ([first, last], "暂无消耗")] {
            let rate = UsageAnalytics.burnRate(samples: samples, cycle: snapshot(remaining: 67).cycle, now: now)
            let forecast = rate.map { insights(rate: $0, remaining: 67) }
            let hint = try XCTUnwrap(QuotaForecast.hint(snapshot: snapshot(remaining: 67), insights: forecast, now: now))
            XCTAssertEqual(hint, expected)
        }
    }

    func testOlderReadingsAndPassedResetKeepTheLastEstimate() throws {
        let forecast = insights(rate: BurnRate(pctPerHour: 30), remaining: 60)
        let stale = UsageSnapshot(agentId: "window", remainingPct: 60, resetAt: now.addingTimeInterval(5 * 3600), windowDuration: 5 * 3600,
                                  updatedAt: now.addingTimeInterval(-QuotaForecast.maximumReadingAge))
        let staleHint = try XCTUnwrap(QuotaForecast.hint(snapshot: stale, insights: forecast, now: now))
        XCTAssertEqual(staleHint, "耗尽 ~2小时")
        let resetHint = try XCTUnwrap(QuotaForecast.hint(snapshot: snapshot(remaining: 60, resetIn: -1),
                                                       insights: forecast, now: now))
        XCTAssertEqual(resetHint, "耗尽 ~2小时")
    }

    func testExhaustedQuotaDoesNotRequireABurnRate() {
        XCTAssertEqual(QuotaForecast.hint(snapshot: snapshot(remaining: 0, resetIn: 14 * 60), insights: nil, now: now),
                       "已耗尽")
    }

    func testSubMinuteEstimateNeverSaysZeroMinutes() {
        let forecast = insights(rate: BurnRate(pctPerHour: 120), remaining: 1)
        XCTAssertEqual(QuotaForecast.hint(snapshot: snapshot(remaining: 1), insights: forecast, now: now),
                       "耗尽 ~1分")
    }

    func testUnknownPeriodDoesNotReuseAnOldForecast() throws {
        let snapshot = UsageSnapshot(agentId: "legacy", remainingPct: 50, resetAt: now.addingTimeInterval(3600), updatedAt: now)
        let hint = try XCTUnwrap(QuotaForecast.hint(snapshot: snapshot, insights: insights(rate: .init(pctPerHour: 100), remaining: 50), now: now))
        XCTAssertEqual(hint, "暂无预测")
    }

    private func snapshot(remaining: Double, resetIn: TimeInterval = 3 * 3600) -> UsageSnapshot {
        UsageSnapshot(agentId: "window", remainingPct: remaining, resetAt: now.addingTimeInterval(resetIn), windowDuration: 5 * 3600, updatedAt: now)
    }

    private func insights(rate: BurnRate, remaining: Double) -> UsageInsights {
        UsageInsights(burnRatePctPerHour: rate.pctPerHour, timeToExhaust: rate.timeToExhaust(remainingPct: remaining),
                      weeklyCapHits: 0, weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                      weeklyShare: [:], windowSessionCount: 0, windowUsedPct: 100 - remaining)
    }
}
