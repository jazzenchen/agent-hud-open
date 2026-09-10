import XCTest
@testable import AgentHUDCore

final class ChartDataTests: XCTestCase {
    override func setUp() {
        super.setUp()
        L10n.setLanguage(.zhHans)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    private func sample(_ agent: String, hour: Int, start: Double, end: Double, tokens: Int = 0) -> HistorySample {
        HistorySample(agentId: agent, hourStart: Date(timeIntervalSince1970: Double(hour) * 3600), remainingStart: start, remainingEnd: end, tokens: tokens)
    }

    private func event(_ agent: String, seconds: Double, tokens: Int) -> TranscriptSession.UsageEvent {
        .init(timestamp: Date(timeIntervalSince1970: seconds), agentId: agent, tokensIn: tokens, tokensOut: 0)
    }

    func testRemainingPathStartsAtOpenAndEndsAtClose() {
        let path = ChartData.remainingPath([
            sample("a", hour: 0, start: 100, end: 80),
            sample("a", hour: 1, start: 80, end: 60),
        ])
        XCTAssertEqual(path, [CGPoint(x: 0, y: 100), CGPoint(x: 0.5, y: 80), CGPoint(x: 1, y: 60)])
    }

    func testRemainingPathDrawsVerticalJumpOnReset() {
        let path = ChartData.remainingPath([
            sample("a", hour: 0, start: 100, end: 40),
            sample("a", hour: 1, start: 100, end: 90),
        ])
        XCTAssertEqual(path, [CGPoint(x: 0, y: 100), CGPoint(x: 0.5, y: 40), CGPoint(x: 0.5, y: 100), CGPoint(x: 1, y: 90)])
    }

    func testEmptyPath() {
        XCTAssertEqual(ChartData.remainingPath([]), [])
    }

    func testNewSourceStartsAtItsActualHourOnSharedTimeAxis() {
        let interval = StatsRange.hours24.interval(endingAt: Date(timeIntervalSince1970: 23.5 * 3600))
        let path = ChartData.usedPath([sample("codex", hour: 23, start: 93, end: 92)],
                                     interval: interval)
        XCTAssertEqual(path, [CGPoint(x: 23.5 / 24, y: 7), CGPoint(x: 1, y: 8)])
        XCTAssertTrue(ChartData.usedPath([sample("codex", hour: -2, start: 99, end: 90)], interval: interval).isEmpty)
    }

    func testBucketedSumsEvenly() {
        XCTAssertEqual(ChartData.bucketed([1, 2, 3, 4], buckets: 2), [3, 7])
        XCTAssertEqual(ChartData.bucketed([1, 2, 3], buckets: 3), [1, 2, 3])
        XCTAssertEqual(ChartData.bucketed([1, 2], buckets: 5), [1, 2], "never more buckets than values")
        XCTAssertEqual(ChartData.bucketed([], buckets: 3), [])
        XCTAssertEqual(ChartData.bucketed(Array(repeating: 1, count: 168), buckets: 48).reduce(0, +), 168)
    }

    func testTokenBarsAlignAgentsByHour() {
        let usage = [
            event("a", seconds: 0, tokens: 10), event("b", seconds: 0, tokens: 1),
            event("a", seconds: 3600, tokens: 20), event("b", seconds: 3600, tokens: 2),
        ]
        let now = Date(timeIntervalSince1970: 5 * 3600)
        let bars = ChartData.tokenBars(usage: usage, agentIds: ["a", "b"], range: .hours5, now: now)
        XCTAssertEqual(bars.map(\.tokens), [[10, 1], [20, 2], [0, 0], [0, 0], [0, 0]])
        XCTAssertEqual(bars.map(\.total), [11, 22, 0, 0, 0], "small token counts are retained in the stack")
        let missing = ChartData.tokenBars(usage: usage, agentIds: ["a", "zzz"], range: .hours5, now: now)
        XCTAssertEqual(missing.map(\.tokens), [[10, 0], [20, 0], [0, 0], [0, 0], [0, 0]])
    }

    func testTokenBarsKeepSparseHoursAndExcludeOutsideSamples() {
        let now = Date(timeIntervalSince1970: 5.5 * 3600)
        let usage = [
            event("a", seconds: -3600, tokens: 999),
            event("a", seconds: 3 * 3600, tokens: 123),
            event("b", seconds: 3 * 3600, tokens: 456),
            event("a", seconds: 6 * 3600, tokens: 999),
        ]
        let bars = ChartData.tokenBars(usage: usage, agentIds: ["a", "b"], range: .hours5, now: now)
        XCTAssertEqual(bars.count, 6, "partial boundary hours retain their actual positions")
        XCTAssertEqual(bars[3].tokens, [123, 456])
        XCTAssertEqual(bars.map(\.total).reduce(0, +), 579)
        XCTAssertEqual(bars[3].interval.start, Date(timeIntervalSince1970: 3 * 3600))
    }

    func testEveryBucketSizePreservesWeekTotalsAndEmptyPeriods() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let usage = (0..<168).flatMap { hour in
            [event("a", seconds: Double(hour) * 3600, tokens: 125),
             event("b", seconds: Double(hour) * 3600, tokens: 250)]
        }
        for size in TokenBucketSize.allCases {
            let bars = ChartData.tokenBars(usage: usage, agentIds: ["a", "b"], range: .days7, bucketSize: size,
                                           now: Date(timeIntervalSince1970: 168 * 3600), calendar: calendar)
            XCTAssertEqual(bars.count, 168 * 60 / size.rawValue)
            XCTAssertEqual(bars.filter { $0.total > 0 }.count, min(168, bars.count))
            XCTAssertEqual(bars.map(\.total).reduce(0, +), 168 * 375)
        }
    }

    func testQuarterHourBoundariesUseActualEventTimes() {
        let usage = [event("a", seconds: 0, tokens: 2), event("a", seconds: 899, tokens: 3),
                     event("a", seconds: 900, tokens: 7), event("a", seconds: 1799, tokens: 11),
                     event("a", seconds: 1800, tokens: 13), event("a", seconds: 3599, tokens: 17),
                     event("b", seconds: 900, tokens: 19)]
        let now = Date(timeIntervalSince1970: 5 * 3600)
        let quarters = ChartData.tokenBars(usage: usage, agentIds: ["a", "b"], range: .hours5, bucketSize: .minutes15, now: now)
        XCTAssertEqual(Array(quarters.prefix(4)).map(\.tokens), [[5, 0], [18, 19], [13, 0], [17, 0]])
        let halves = ChartData.tokenBars(usage: usage, agentIds: ["a", "b"], range: .hours5, bucketSize: .minutes30, now: now)
        XCTAssertEqual(Array(halves.prefix(2)).map(\.tokens), [[23, 19], [30, 0]])
        XCTAssertEqual(quarters.reduce(0) { $0 + $1.total }, halves.reduce(0) { $0 + $1.total })
    }

    func testPartialBucketsExcludeEventsOutsideTheExactRange() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let now = Date(timeIntervalSince1970: 5 * 3600 + 7 * 60)
        let usage = [event("a", seconds: 6 * 60, tokens: 999), event("a", seconds: 7 * 60, tokens: 3),
                     event("a", seconds: now.timeIntervalSince1970, tokens: 999),
                     event("a", seconds: now.timeIntervalSince1970 + 1, tokens: 999)]
        for size in TokenBucketSize.allCases {
            let bars = ChartData.tokenBars(usage: usage, agentIds: ["a"], range: .hours5, bucketSize: size, now: now, calendar: calendar)
            XCTAssertEqual(bars.reduce(0) { $0 + $1.total }, 3)
            XCTAssertEqual(bars.first?.interval.start, Date(timeIntervalSince1970: 0))
        }
    }

    func testChartHitTestingIncludesEmptyBucketsAndTheRightEdge() {
        let bars = ChartData.tokenBars(usage: [], agentIds: ["a"], range: .hours5, bucketSize: .minutes15,
                                      now: Date(timeIntervalSince1970: 5 * 3600))
        XCTAssertEqual(ChartData.tokenColumn(at: Date(timeIntervalSince1970: 899), in: bars), bars[0])
        XCTAssertEqual(ChartData.tokenColumn(at: Date(timeIntervalSince1970: 900), in: bars), bars[1])
        XCTAssertEqual(ChartData.tokenColumn(at: Date(timeIntervalSince1970: 5 * 3600), in: bars), bars.last)
        XCTAssertNil(ChartData.tokenColumn(at: Date(timeIntervalSince1970: -1), in: bars))
        XCTAssertNil(ChartData.tokenColumn(at: Date(), in: []))
    }

    func testDailyBucketsUseLocalMidnightAndKeepExactRangeBoundaries() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Shanghai")!
        let now = ISO8601Fast.parse("2026-09-08T02:00:00+08:00")!
        let usage = [
            ("2026-09-07T20:59:59+08:00", 999), ("2026-09-07T21:00:00+08:00", 3),
            ("2026-09-07T23:59:59+08:00", 7), ("2026-09-08T00:00:00+08:00", 11),
            ("2026-09-08T01:59:59+08:00", 13), ("2026-09-08T02:00:00+08:00", 999)
        ].map { event("a", seconds: ISO8601Fast.parse($0.0)!.timeIntervalSince1970, tokens: $0.1) }
        let bars = ChartData.tokenBars(usage: usage, agentIds: ["a"], range: .hours5, bucketSize: .day1, now: now, calendar: calendar)
        XCTAssertEqual(bars.map(\.total), [10, 24])
        XCTAssertTrue(bars.allSatisfy { calendar.component(.hour, from: $0.interval.start) == 0 })
        XCTAssertEqual(bars[0].interval.end, bars[1].interval.start)
        XCTAssertEqual(TokenBucketSize.allCases.last?.label, "1d")
    }

    func testDailyBucketsFollowShortAndLongDaysAcrossDST() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "America/Los_Angeles")!
        for (first, second, end, hours) in [
            ("2026-03-08T01:30:00-08:00", "2026-03-08T03:30:00-07:00", "2026-03-09T12:00:00-07:00", 23),
            ("2026-11-01T01:30:00-07:00", "2026-11-01T01:30:00-08:00", "2026-11-02T12:00:00-08:00", 25)
        ] {
            let firstDate = ISO8601Fast.parse(first)!
            let usage = [event("a", seconds: firstDate.timeIntervalSince1970, tokens: 3),
                         event("a", seconds: ISO8601Fast.parse(second)!.timeIntervalSince1970, tokens: 7)]
            let bars = ChartData.tokenBars(usage: usage, agentIds: ["a"], range: .days7, bucketSize: .day1,
                                          now: ISO8601Fast.parse(end)!, calendar: calendar)
            let day = bars.first { $0.interval.start == calendar.startOfDay(for: firstDate) }!
            XCTAssertEqual(day.total, 10)
            XCTAssertEqual(day.interval.duration, Double(hours * 3600))
            XCTAssertTrue(bars.allSatisfy { calendar.component(.hour, from: $0.interval.start) == 0 })
            XCTAssertEqual(bars.reduce(0) { $0 + $1.total }, 10)
        }
    }

    func testAxisLabelsProduceFourEntries() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Shanghai")!
        // 2026-09-06 (Sunday) 14:32 CST
        let now = calendar.date(from: DateComponents(year: 2026, month: 9, day: 6, hour: 14, minute: 32))!
        let labels = ChartData.axisLabels(range: .hours5, now: now, calendar: calendar)
        XCTAssertEqual(labels, ["周日 09:32", "周日 11:12", "周日 12:52", "现在 14:32"])
        XCTAssertEqual(ChartData.axisLabels(range: .days7, now: now, calendar: calendar).count, 4)
        XCTAssertEqual(ChartData.weekdayTime(now, calendar: calendar), "周日 14:32")
    }

    func testStatsRangeLabels() {
        XCTAssertEqual(StatsRange.allCases.map(\.label), ["5 小时", "24 小时", "7 天"])
        XCTAssertEqual(StatsRange.allCases.map(\.hours), [5, 24, 168])
        let now = Date()
        for range in StatsRange.allCases {
            XCTAssertEqual(range.interval(endingAt: now).duration, Double(range.hours) * 3600)
            XCTAssertEqual(range.interval(endingAt: now).end, now)
        }
    }
}

final class BurnRateTests: XCTestCase {
    func testAveragesConsumptionIgnoringResets() {
        let base = Date(timeIntervalSince1970: 0)
        let samples = [
            HistorySample(agentId: "a", hourStart: base, remainingStart: 100, remainingEnd: 90, tokens: 0),
            HistorySample(agentId: "a", hourStart: base.addingTimeInterval(3600), remainingStart: 90, remainingEnd: 84, tokens: 0),
            HistorySample(agentId: "a", hourStart: base.addingTimeInterval(7200), remainingStart: 84, remainingEnd: 100, tokens: 0),
        ]
        let rate = BurnRate.estimate(samples)
        XCTAssertEqual(rate?.pctPerHour ?? 0, 8, accuracy: 1e-9)
        XCTAssertEqual(rate?.timeToExhaust(remainingPct: 24) ?? 0, 3 * 3600, accuracy: 1e-6)
    }

    func testNeedsTwoSamples() {
        XCTAssertNil(BurnRate.estimate([]))
        XCTAssertNil(BurnRate.estimate([HistorySample(agentId: "a", hourStart: Date(), remainingStart: 100, remainingEnd: 90, tokens: 0)]))
    }

    func testZeroRateHasNoExhaustTime() {
        XCTAssertNil(BurnRate(pctPerHour: 0).timeToExhaust(remainingPct: 50))
    }
}
