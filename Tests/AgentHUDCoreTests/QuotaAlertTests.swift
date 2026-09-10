import XCTest
@testable import AgentHUDCore

final class QuotaAlertTests: XCTestCase {
    private let start = Date(timeIntervalSince1970: 1_800_000_000)
    private var agent: AgentDescriptor { .init(id: "claude-session", vendor: "Claude", model: "5h", source: "", enabled: true) }

    func testFirstReadingIsSilentAndForecastOnlyNotifiesOnEntry() {
        var tracker = QuotaAlertTracker()
        XCTAssertTrue(feed(&tracker, remaining: 50, elapsed: 0, exhaustIn: 30_000).alerts.isEmpty)
        let warning = feed(&tracker, remaining: 45, elapsed: 120, exhaustIn: 2400)
        XCTAssertEqual(warning.alerts.map(\.kind), [.exhaustion])
        XCTAssertEqual(warning.alerts.first?.timeToExhaust, 2400)
        XCTAssertTrue(warning.criticalAgentIDs.isEmpty)
        XCTAssertTrue(feed(&tracker, remaining: 45, elapsed: 120, exhaustIn: 2400).alerts.isEmpty)
        XCTAssertTrue(feed(&tracker, remaining: 40, elapsed: 240, exhaustIn: 2200).alerts.isEmpty)
    }

    func testCriticalFallbackSharesBaselineWithSystemNotification() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 20, elapsed: 0)
        let warning = feed(&tracker, remaining: 9, elapsed: 120)
        XCTAssertEqual(warning.alerts.map(\.kind), [.exhaustion])
        XCTAssertEqual(warning.criticalAgentIDs, [agent.id])
        let repeated = feed(&tracker, remaining: 8, elapsed: 240)
        XCTAssertTrue(repeated.alerts.isEmpty)
        XCTAssertTrue(repeated.criticalAgentIDs.isEmpty)
        var initiallyCritical = QuotaAlertTracker()
        let initial = feed(&initiallyCritical, remaining: 1, elapsed: 0)
        XCTAssertTrue(initial.alerts.isEmpty)
        XCTAssertTrue(initial.criticalAgentIDs.isEmpty)
    }

    func testForecastWarningDoesNotDuplicateAtCriticalButSystemThresholdStillWorks() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 50, elapsed: 0)
        _ = feed(&tracker, remaining: 40, elapsed: 120, exhaustIn: 1000)
        let critical = feed(&tracker, remaining: 5, elapsed: 240, exhaustIn: 500)
        XCTAssertTrue(critical.alerts.isEmpty)
        XCTAssertEqual(critical.criticalAgentIDs, [agent.id])
    }

    func testResetTimeAloneAndCachedReadingDoNotReset() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 1, elapsed: 0, deadline: 60)
        XCTAssertTrue(feed(&tracker, remaining: 1, elapsed: 120, deadline: 60).alerts.isEmpty)
        let placeholder = snapshot(remaining: 100, elapsed: 0, deadline: nil)
        XCTAssertTrue(tracker.update(report: report([placeholder]), agents: [agent], now: start.addingTimeInterval(120)).alerts.isEmpty)
        let reset = feed(&tracker, remaining: 95, elapsed: 240, deadline: 18_240)
        XCTAssertEqual(reset.alerts.map(\.kind), [.reset])
        XCTAssertTrue(feed(&tracker, remaining: 95, elapsed: 240, deadline: 18_240).alerts.isEmpty)
    }

    func testNewObservedFullIdleWindowConfirmsResetWithoutADeadline() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 2, elapsed: 0, deadline: 60)
        let idle = snapshot(remaining: 100, elapsed: 120, deadline: nil)
        let update = tracker.update(report: report([idle]), agents: [agent], now: start.addingTimeInterval(120))
        XCTAssertEqual(update.alerts.map(\.kind), [.reset])
        XCTAssertNil(update.alerts.first?.snapshot.resetAt)
    }

    func testEarlyFullResetButNotSmallCorrections() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 5, elapsed: 0)
        XCTAssertTrue(feed(&tracker, remaining: 6, elapsed: 120).alerts.isEmpty)
        XCTAssertEqual(feed(&tracker, remaining: 100, elapsed: 240).alerts.map(\.kind), [.reset])
        XCTAssertTrue(feed(&tracker, remaining: 100, elapsed: 360).alerts.isEmpty)
    }

    func testNewCycleCanBeRecognizedEvenIfAlreadyHeavilyUsed() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 50, elapsed: 0, deadline: 60)
        XCTAssertEqual(feed(&tracker, remaining: 30, elapsed: 120, deadline: 18_120).alerts.map(\.kind), [.reset])
    }

    func testMissingFailedAndStaleReadingsDoNotEraseConfirmedHistory() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 1, elapsed: 0)
        XCTAssertTrue(tracker.update(report: report([]), agents: [agent], now: start.addingTimeInterval(120)).alerts.isEmpty)
        let restored = snapshot(remaining: 100, elapsed: 120)
        XCTAssertTrue(tracker.update(report: report([restored], notices: ["Claude": "failed"]), agents: [agent], now: start.addingTimeInterval(120)).alerts.isEmpty)
        XCTAssertTrue(tracker.update(report: report([restored]), agents: [agent], now: start.addingTimeInterval(2400)).alerts.isEmpty)
        XCTAssertEqual(feed(&tracker, remaining: 100, elapsed: 2520).alerts.map(\.kind), [.reset])
    }

    func testResetNamesOtherExhaustedWindowWithoutMixingVendors() {
        let weekly = AgentDescriptor(id: "claude-weekly", vendor: "Claude", model: "weekly", source: "", enabled: true)
        let codex = AgentDescriptor(id: "codex", vendor: "Codex", model: "weekly", source: "", enabled: true)
        let agents = [agent, weekly, codex]
        var tracker = QuotaAlertTracker()
        func samples(_ elapsed: Double, _ remaining: Double) -> [UsageSnapshot] {
            [snapshot(remaining: remaining, elapsed: elapsed), snapshot(remaining: 0, elapsed: elapsed, id: weekly.id),
             snapshot(remaining: 0, elapsed: elapsed, id: codex.id)]
        }
        _ = tracker.update(report: report(samples(0, 2)), agents: agents, now: start)
        let events = tracker.update(report: report(samples(120, 100)), agents: agents, now: start.addingTimeInterval(120))
        XCTAssertEqual(events.alerts.count, 1)
        XCTAssertEqual(events.alerts.first?.otherExhaustedWindows, ["weekly"])
    }

    func testReenabledWindowStartsWithANewBaseline() {
        var tracker = QuotaAlertTracker()
        _ = feed(&tracker, remaining: 1, elapsed: 0)
        _ = tracker.update(report: report([]), agents: [], now: start.addingTimeInterval(60))
        XCTAssertTrue(feed(&tracker, remaining: 100, elapsed: 120).alerts.isEmpty)
    }

    func testPreviewUsesIndependentSampleDataAndFreshPresentationIdentity() {
        let warning = QuotaAlert.preview(.exhaustion, agent: agent, now: start)
        let reset = QuotaAlert.preview(.reset, agent: agent, now: start)
        XCTAssertTrue(warning.isPreview)
        XCTAssertEqual(warning.snapshot.remainingPct, 8)
        XCTAssertEqual(reset.snapshot.remainingPct, 100)
        XCTAssertNotEqual(reset.id, QuotaAlert.preview(.reset, agent: agent, now: start).id)
    }

    private func snapshot(remaining: Double, elapsed: Double, deadline: Double? = 18_000, id: String? = nil) -> UsageSnapshot {
        .init(agentId: id ?? agent.id, remainingPct: remaining, resetAt: deadline.map { start.addingTimeInterval($0) },
              updatedAt: start.addingTimeInterval(elapsed))
    }

    private func report(_ snapshots: [UsageSnapshot], exhaustIn: Double? = nil, notices: [String: String] = [:]) -> UsageReport {
        let insights = UsageInsights(burnRatePctPerHour: nil, timeToExhaust: exhaustIn, weeklyCapHits: 0,
                                     weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                                     weeklyShare: [:], windowSessionCount: 0, windowUsedPct: 0)
        return UsageReport(generatedAt: snapshots.last?.updatedAt ?? start, snapshots: snapshots, sessions: [], history: [],
                           activity: .empty, insights: .empty, insightsByAgent: [agent.id: insights], sourceNotices: notices)
    }

    private func feed(_ tracker: inout QuotaAlertTracker, remaining: Double, elapsed: Double,
                      exhaustIn: Double? = nil, deadline: Double = 18_000) -> QuotaAlertTracker.Update {
        tracker.update(report: report([snapshot(remaining: remaining, elapsed: elapsed, deadline: deadline)], exhaustIn: exhaustIn),
                       agents: [agent], now: start.addingTimeInterval(elapsed))
    }
}
