import XCTest
@testable import AgentHUDCore

final class QuotaCycleTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_123)

    func testCyclesUseResetAnchorsAndTheirOwnGranularity() throws {
        let five = try XCTUnwrap(QuotaCycle(resetAt: now.addingTimeInterval(2 * 3600), duration: 5 * 3600))
        XCTAssertEqual(five.start, now.addingTimeInterval(-3 * 3600))
        XCTAssertEqual(five.sampleInterval, 15 * 60)
        let weekly = try XCTUnwrap(QuotaCycle(resetAt: now.addingTimeInterval(2 * 86400), duration: 7 * 86400))
        XCTAssertEqual(weekly.start, now.addingTimeInterval(-5 * 86400))
        XCTAssertEqual(weekly.sampleInterval, 3600)
        XCTAssertNil(QuotaCycle(resetAt: nil, duration: 5 * 3600))
        XCTAssertNil(QuotaCycle(resetAt: now, duration: nil))
        XCTAssertNil(QuotaCycle(resetAt: now, duration: 0))
    }

    func testFiveHourForecastRetainsConsumptionBeforeAnIdleLastHalfHour() throws {
        let cycle = QuotaCycle(resetAt: now.addingTimeInterval(2 * 3600), duration: 5 * 3600)
        let samples = [sample(hoursAgo: 3, remaining: 100), sample(hoursAgo: 2, remaining: 80),
                       sample(hoursAgo: 1, remaining: 60), sample(hoursAgo: 0.5, remaining: 60), sample(hoursAgo: 0, remaining: 60)]
        let rate = try XCTUnwrap(UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: now))
        XCTAssertEqual(rate.pctPerHour, 40.0 / 3, accuracy: 1e-9)
        XCTAssertEqual(try XCTUnwrap(rate.timeToExhaust(remainingPct: 60)), 4.5 * 3600, accuracy: 1e-6)
    }

    func testWeeklyForecastUsesDaysOfHistory() throws {
        let cycle = QuotaCycle(resetAt: now.addingTimeInterval(2 * 86400), duration: 7 * 86400)
        let samples = [sample(hoursAgo: 120, remaining: 100), sample(hoursAgo: 72, remaining: 80),
                       sample(hoursAgo: 24, remaining: 70), sample(hoursAgo: 1, remaining: 60), sample(hoursAgo: 0, remaining: 58)]
        let rate = try XCTUnwrap(UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: now))
        XCTAssertEqual(rate.pctPerHour, 42.0 / 120, accuracy: 1e-9)
    }

    func testPreviousCycleAndFutureReadingsAreExcluded() throws {
        let cycle = try XCTUnwrap(QuotaCycle(resetAt: now.addingTimeInterval(2 * 3600), duration: 5 * 3600))
        let samples = [sample(hoursAgo: 4, remaining: 10), sample(hoursAgo: 3, remaining: 100),
                       sample(hoursAgo: 1, remaining: 80), sample(hoursAgo: 0, remaining: 70), sample(hoursAgo: -1, remaining: 0)]
        let selected = UsageAnalytics.sampledQuota(samples, cycle: cycle, now: now)
        XCTAssertEqual(selected.first?.timestamp, cycle.start)
        XCTAssertEqual(selected.last?.timestamp, now)
        XCTAssertEqual(try XCTUnwrap(UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: now)).pctPerHour, 10, accuracy: 1e-9)
        XCTAssertNil(UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: cycle.resetAt))
    }

    func testQuarterHourBucketsAreAlignedToCycleStartAndKeepLatestPartialBucket() throws {
        let cycle = try XCTUnwrap(QuotaCycle(resetAt: now.addingTimeInterval(5 * 3600), duration: 5 * 3600))
        let minutes = [0, 1, 14, 15, 29, 30, 38]
        let samples = minutes.enumerated().map { index, minute in
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(Double(minute * 60)), remainingPct: 100 - Double(index))
        }
        let selected = UsageAnalytics.sampledQuota(samples, cycle: cycle, now: now.addingTimeInterval(38 * 60))
        XCTAssertEqual(selected.map { Int($0.timestamp.timeIntervalSince(now) / 60) }, [0, 14, 29, 38])
    }

    func testWeeklyBucketsAreHourly() throws {
        let cycle = try XCTUnwrap(QuotaCycle(resetAt: now.addingTimeInterval(7 * 86400), duration: 7 * 86400))
        let samples = [0, 59, 60, 119, 120, 150].enumerated().map { index, minute in
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(Double(minute * 60)), remainingPct: 100 - Double(index))
        }
        let selected = UsageAnalytics.sampledQuota(samples, cycle: cycle, now: now.addingTimeInterval(150 * 60))
        XCTAssertEqual(selected.map { Int($0.timestamp.timeIntervalSince(now) / 60) }, [0, 59, 119, 150])
    }

    func testMissingBeginningIsNotFilledWithUnusedQuotaOrIdleTime() throws {
        let cycle = QuotaCycle(resetAt: now.addingTimeInterval(2 * 86400), duration: 7 * 86400)
        let rate = try XCTUnwrap(UsageAnalytics.burnRate(samples: [sample(hoursAgo: 2, remaining: 80), sample(hoursAgo: 0, remaining: 60)],
                                                       cycle: cycle, now: now))
        XCTAssertEqual(rate.pctPerHour, 10, accuracy: 1e-9, "only the two observed hours are covered, not five days")
    }

    func testRequiresAtLeastOneSamplingIntervalAndDistinguishesIdleFromMissing() throws {
        for (duration, minimum) in [(5.0 * 3600, 15.0 * 60), (7.0 * 86400, 3600.0)] {
            let cycle = QuotaCycle(resetAt: now.addingTimeInterval(duration / 2), duration: duration)
            XCTAssertNil(UsageAnalytics.burnRate(samples: [sample(hoursAgo: (minimum - 1) / 3600, remaining: 80), sample(hoursAgo: 0, remaining: 60)],
                                                cycle: cycle, now: now))
            let idle = try XCTUnwrap(UsageAnalytics.burnRate(samples: [sample(hoursAgo: minimum / 3600, remaining: 80), sample(hoursAgo: 0, remaining: 80)],
                                                            cycle: cycle, now: now))
            XCTAssertEqual(idle.pctPerHour, 0)
            XCTAssertNil(idle.timeToExhaust(remainingPct: 80))
        }
    }

    func testSnapshotPersistsPeriodAndLegacyDataDoesNotAssumeFiveHours() throws {
        let snapshot = UsageSnapshot(agentId: "a", remainingPct: 50, resetAt: now.addingTimeInterval(86400), windowDuration: 7 * 86400, updatedAt: now)
        let encoder = JSONEncoder(), decoder = JSONDecoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        decoder.dateDecodingStrategy = .secondsSince1970
        XCTAssertEqual(try decoder.decode(UsageSnapshot.self, from: encoder.encode(snapshot)), snapshot)
        let legacy = Data(#"{"agentId":"a","remainingPct":50,"resetAt":1800010800,"updatedAt":1800000000}"#.utf8)
        let restored = try decoder.decode(UsageSnapshot.self, from: legacy)
        XCTAssertNil(restored.windowDuration)
        XCTAssertNil(restored.cycle)
    }

    private func sample(hoursAgo: Double, remaining: Double) -> QuotaSample {
        QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-hoursAgo * 3600), remainingPct: remaining)
    }
}
