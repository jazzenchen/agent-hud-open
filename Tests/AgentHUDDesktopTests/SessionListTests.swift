import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

final class SessionListTests: XCTestCase {
    @MainActor
    func testActiveOnlyKeepsTheLastDayAndEarlierHoldsWhatStartedBeforeTheWeek() throws {
        let suite = "SessionListTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: []))
        let now = store.now
        func session(_ id: String, startedHoursAgo: Double, endedHoursAgo: Double? = nil) -> LiveSession {
            LiveSession(id: id, agentId: "codex", task: id, terminal: nil,
                        startedAt: now.addingTimeInterval(-startedHoursAgo * 3600),
                        endedAt: endedHoursAgo.map { now.addingTimeInterval(-$0 * 3600) },
                        pctOfWindow: nil, tokensIn: 0, tokensOut: 0, observedAt: now)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: [
            session("running", startedHoursAgo: 240),
            session("recent", startedHoursAgo: 10, endedHoursAgo: 2),
            session("resumed", startedHoursAgo: 240, endedHoursAgo: 20),
            session("quiet", startedHoursAgo: 40, endedHoursAgo: 30),
        ]))
        XCTAssertEqual(store.listedSessions(source: nil, activeOnly: true).map(\.id), ["running", "recent", "resumed"])
        let week = store.listedSessions(source: nil, activeOnly: false)
        XCTAssertEqual(week.map(\.id), ["running", "recent", "resumed", "quiet"])
        let groups = SessionList.groups(week, store: store, today: Calendar.current.startOfDay(for: now), calendar: .current)
        XCTAssertEqual(groups.last?.day, .distantPast)
        XCTAssertEqual(groups.last?.sessions.map(\.id), ["running", "resumed"], "sessions that started before the week, newest activity first")
    }
}
