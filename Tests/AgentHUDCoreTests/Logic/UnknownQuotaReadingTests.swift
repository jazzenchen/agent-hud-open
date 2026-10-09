import XCTest
@testable import AgentHUDCore

final class UnknownQuotaReadingTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    private let agent = AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: "", enabled: true)

    func testObservedUnknownReplacesTheRetainedNumberAndKnownValuesRecover() async throws {
        let values: [Double?] = [60, nil, 0, 100]
        let readings = values.enumerated().map { index, remaining in
            report(remaining: remaining, elapsed: Double(index * 60))
        }
        let provider = RetainedUsageProvider(provider: UnknownQuotaSequenceProvider(readings))
        for (index, remaining) in values.enumerated() {
            let result = try await provider.fetchUsage(agents: [agent], historyHours: 24)
            let snapshot = try XCTUnwrap(result.snapshot(for: agent.id))
            XCTAssertEqual(snapshot.remainingPct, remaining)
            XCTAssertEqual(snapshot.updatedAt, now.addingTimeInterval(Double(index * 60)))
            XCTAssertEqual(snapshot.resetAt, now.addingTimeInterval(3 * 3600))
            XCTAssertEqual(result.assess(.window(agent), now: snapshot.updatedAt).hasValue, remaining != nil)
            XCTAssertEqual(try JSONDecoder().decode(UsageReport.self, from: JSONEncoder().encode(result)), result)
        }
    }

    func testObservedUnknownKeepsResetAndObservationWithoutNumericSurfaces() throws {
        let unknown = report(remaining: nil)
        let view = ReportView(report: unknown, agents: [agent], settings: Settings(), now: now)
        let row = try XCTUnwrap(view.rows.first)
        XCTAssertNil(row.remainingPct)
        XCTAssertNil(row.usedPct)
        XCTAssertNil(row.level)
        XCTAssertEqual(row.missingQuotaLabel, "N/A")
        XCTAssertEqual(row.resetAt, now.addingTimeInterval(3 * 3600))
        XCTAssertEqual(row.assessment.observedAt, now)
        XCTAssertFalse(row.assessment.hasValue)
        XCTAssertFalse(row.assessment.showsLevel)
        XCTAssertFalse(row.assessment.confirmsEvents)
        XCTAssertEqual(row.assessment.status, .normal)
        XCTAssertNil(view.maxUsedPct)
        XCTAssertTrue(view.glowSegments.isEmpty)
        XCTAssertNil(view.outlook(for: agent.id))
        XCTAssertNil(view.forecastHint(for: agent.id))
        XCTAssertNil(view.tokensPerHour(for: agent.id))

        let pending = ReportView(report: nil, agents: [agent], settings: Settings(), now: now)
        XCTAssertEqual(pending.rows.first?.missingQuotaLabel, "—")
        for (remaining, used, level) in [(0.0, 100.0, StatusLevel.critical), (100.0, 0.0, .ok)] {
            let known = ReportView(report: report(remaining: remaining), agents: [agent], settings: Settings(), now: now)
            XCTAssertEqual(known.rows.first?.usedPct, used)
            XCTAssertEqual(known.rows.first?.level, level)
            XCTAssertEqual(known.maxUsedPct, used)
            XCTAssertTrue(known.rows.first?.assessment.confirmsEvents == true)
        }
    }

    func testUnknownDoesNotForecastFromEarlierNumericSamples() throws {
        let samples = [QuotaSample(agentId: agent.id, timestamp: now.addingTimeInterval(-3600), remainingPct: 80),
                       QuotaSample(agentId: agent.id, timestamp: now, remainingPct: 60)]
        let known = try XCTUnwrap(report(remaining: 60).snapshot(for: agent.id))
        let unknown = try XCTUnwrap(report(remaining: nil).snapshot(for: agent.id))
        let oldInsights = QuotaMath.insights(snapshot: known, samples: samples, now: now)
        XCTAssertEqual(oldInsights.burnRatePctPerHour, 20)
        XCTAssertNotNil(oldInsights.timeToExhaust)
        let insights = QuotaMath.insights(snapshot: unknown, samples: samples, now: now)
        XCTAssertNil(insights.burnRatePctPerHour)
        XCTAssertNil(insights.timeToExhaust)
        XCTAssertEqual(insights.weeklyCapHits, 0)
        XCTAssertNil(QuotaForecast.hint(snapshot: unknown, insights: oldInsights, now: now))
        XCTAssertEqual(QuotaMath.outlook(snapshot: unknown, insights: oldInsights, now: now), .noEstimate)
    }

    func testUnknownBreaksAlertBaselineWithoutInventingZeroOrReset() {
        var tracker = QuotaAlertTracker()
        func update(_ remaining: Double?, _ elapsed: Double) -> QuotaAlertTracker.Update {
            tracker.update(report: report(remaining: remaining, elapsed: elapsed), agents: [agent], now: now.addingTimeInterval(elapsed))
        }
        XCTAssertTrue(update(60, 0).alerts.isEmpty)
        let unknown = update(nil, 60)
        XCTAssertTrue(unknown.alerts.isEmpty)
        XCTAssertTrue(unknown.criticalAgentIDs.isEmpty)
        XCTAssertTrue(unknown.exhaustedAgentIDs.isEmpty)
        let recovered = update(0, 120)
        XCTAssertTrue(recovered.alerts.isEmpty, "the first known value after an unknown reading is a new baseline")
        XCTAssertTrue(recovered.criticalAgentIDs.isEmpty)
        XCTAssertTrue(recovered.exhaustedAgentIDs.isEmpty)
        XCTAssertEqual(update(100, 180).alerts.map(\.kind), [.reset], "a genuine full value still confirms a reset")
        let depleted = update(0, 240)
        XCTAssertEqual(depleted.alerts.map(\.kind), [.exhaustion])
        XCTAssertEqual(depleted.exhaustedAgentIDs, [agent.id], "a genuine zero still confirms exhaustion")
        XCTAssertTrue(update(nil, 300).alerts.isEmpty)
        XCTAssertTrue(update(100, 360).alerts.isEmpty, "unknown must not be mistaken for an exhausted value before recovery")
    }

    func testSnapshotsDecodeEarlierNumbersAndPersistUnknownValues() throws {
        let legacy = Data(#"{"agentId":"codex","remainingPct":60,"updatedAt":0}"#.utf8)
        XCTAssertEqual(try JSONDecoder().decode(UsageSnapshot.self, from: legacy).remainingPct, 60)
        for payload in [#"{"agentId":"codex","remainingPct":null,"updatedAt":0}"#,
                        #"{"agentId":"codex","updatedAt":0}"#] {
            XCTAssertNil(try JSONDecoder().decode(UsageSnapshot.self, from: Data(payload.utf8)).remainingPct)
        }
        let unknown = try XCTUnwrap(report(remaining: nil).snapshot(for: agent.id))
        XCTAssertEqual(try JSONDecoder().decode(UsageSnapshot.self, from: JSONEncoder().encode(unknown)), unknown)
    }

    private func report(remaining: Double?, elapsed: TimeInterval = 0) -> UsageReport {
        let at = now.addingTimeInterval(elapsed)
        return UsageReport(generatedAt: at, snapshots: [UsageSnapshot(agentId: agent.id, remainingPct: remaining,
            resetAt: now.addingTimeInterval(3 * 3600), windowDuration: 5 * 3600, updatedAt: at)], sessions: [], discoveredAgents: [agent],
            insightsByAgent: [agent.id: UsageInsights(burnRatePctPerHour: 20, timeToExhaust: 1200, weeklyCapHits: 0,
                weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil)])
    }
}

private actor UnknownQuotaSequenceProvider: UsageProvider {
    private var reports: [UsageReport]
    init(_ reports: [UsageReport]) { self.reports = reports }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) -> UsageReport { reports.removeFirst() }
}
