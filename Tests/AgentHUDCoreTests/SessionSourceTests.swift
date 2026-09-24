import XCTest
@testable import AgentHUDCore

final class SessionSourceTests: XCTestCase {
    @MainActor
    func testSessionsGroupUnderTheDayTheyStarted() {
        let defaults = UserDefaults(suiteName: "AgentHUDSessionDayTests.\(UUID())")!
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults))
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Shanghai")!
        let noon = calendar.date(from: DateComponents(year: 2026, month: 9, day: 24, hour: 12))!
        func session(_ id: String, started: TimeInterval, ended: TimeInterval?) -> LiveSession {
            .init(id: id, agentId: "claude-model:test", task: id, terminal: nil, startedAt: noon.addingTimeInterval(started * 3600),
                  endedAt: ended.map { noon.addingTimeInterval($0 * 3600) }, pctOfWindow: nil, tokensIn: 1, tokensOut: 1,
                  observedAt: noon.addingTimeInterval(1800))
        }
        // One still running, one that began last night and ended after midnight, one from yesterday evening.
        let sessions = [session("running", started: -1, ended: nil), session("overnight", started: -14, ended: -11),
                        session("yesterday", started: -20, ended: -16)]
        let days = store.sessionsByDay(sessions, calendar: calendar)
        XCTAssertEqual(days.map(\.day), [calendar.startOfDay(for: noon), calendar.startOfDay(for: noon.addingTimeInterval(-86400))])
        XCTAssertEqual(days.map { $0.sessions.map(\.id) }, [["running"], ["overnight", "yesterday"]],
                       "a session that ran past midnight stays with the day it began")
    }

    @MainActor
    func testPointingAtAQuotaOrASessionTurnsTheStatisticsPage() {
        let defaults = UserDefaults(suiteName: "AgentHUDStatsTabTests.\(UUID())")!
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults))
        store.focusedSessionID = "s1"
        XCTAssertEqual(store.statsTab, .sessions)
        store.focusedSessionID = nil
        XCTAssertEqual(store.statsTab, .sessions, "going back to the list stays on Sessions")
        store.selectedQuotaId = "claude"
        XCTAssertEqual(store.statsTab, .tokens)
    }

    func testCodexClientGroupsPreserveProviderAndUnknownSurface() {
        let cli = SessionSource(vendor: "Codex", client: "CLI")
        XCTAssertEqual(cli, SessionSource(vendor: "Codex", client: "CLI · exec"))
        XCTAssertEqual(cli.name, "Codex CLI")
        XCTAssertNotEqual(cli, SessionSource(vendor: "Claude", client: "CLI · exec"))
        XCTAssertEqual(SessionSource(vendor: "Codex", client: "Desktop").name, "Codex Desktop")
        XCTAssertEqual(SessionSource(vendor: "Codex", client: "IDE").name, "Codex IDE extension")
        XCTAssertEqual(SessionSource(vendor: "Codex", client: "Codex"), SessionSource(vendor: "Codex", client: nil))
        XCTAssertNotEqual(SessionSource(vendor: "Codex", client: nil), SessionSource(vendor: "Codex", client: "Desktop"))
        XCTAssertEqual(SessionSource(vendor: "DeepSeek", client: "DeepSeek Harness").name, "DeepSeek Harness")
        XCTAssertEqual(SessionSource(vendor: "Claude", client: "Claude Code").name, "Claude Code")
        XCTAssertEqual(SessionSource(vendor: "Codex", client: "Future client").name, "Future client")
    }

    func testClaudeSurfacesComeFromTheTranscriptEntrypoint() {
        XCTAssertEqual(ClaudeEntrypoint.clientLabel("claude-desktop"), "Claude Code Desktop")
        XCTAssertEqual(ClaudeEntrypoint.clientLabel("cli"), "Claude Code CLI")
        XCTAssertEqual(ClaudeEntrypoint.clientLabel("claude-vscode"), "Claude Code IDE extension")
        XCTAssertEqual(ClaudeEntrypoint.clientLabel("sdk-ts"), "Claude Agent SDK")
        XCTAssertEqual(ClaudeEntrypoint.clientLabel(nil), "Claude Code", "older builds never wrote the field")
        XCTAssertEqual(SessionSource(vendor: "Claude", client: "Claude Code Desktop").name, "Claude Code Desktop")
        XCTAssertNotEqual(SessionSource(vendor: "Claude", client: "Claude Code Desktop"), SessionSource(vendor: "Claude", client: "Claude Code CLI"))
        XCTAssertEqual(SessionSource(vendor: "Claude", client: "Claude Code"), SessionSource(vendor: "Claude", client: nil),
                       "the plain product name from older builds and synced peers is one group")
        XCTAssertEqual(SessionSource(vendor: "Claude", client: nil).name, "Claude Code")
        XCTAssertEqual(SessionSource.vendor(impliedBy: "claude-model:Unknown"), "Claude")
        XCTAssertEqual(SessionSource.vendor(impliedBy: "codex-model:gpt-5"), "Codex")
        XCTAssertEqual(SessionSource.vendor(impliedBy: "antigravity"), "Antigravity")
    }

    @MainActor
    func testStoreResolvesSourcesEvenWhenQuotaAgentIsDisabled() {
        let suite = "AgentHUDSessionSourceTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents.map { $0.with(enabled: false) })
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: DemoData.agents, historyHours: 24, now: Date()))
        let session = LiveSession(id: "example", agentId: "codex", task: "Example", terminal: nil,
            startedAt: Date(), pctOfWindow: nil, tokensIn: 10, tokensOut: 2, client: "CLI · exec")
        XCTAssertEqual(store.sessionSource(session).name, "Codex CLI")
        XCTAssertEqual(session.client, "CLI · exec", "Presentation must not rewrite persisted source metadata")
        let silent = LiveSession(id: "silent", agentId: "claude-model:Unknown", task: "Silent", terminal: nil,
            startedAt: Date(), pctOfWindow: nil, tokensIn: 0, tokensOut: 0, client: "Claude Code Desktop")
        XCTAssertEqual(store.sessionSource(silent), SessionSource(vendor: "Claude", client: "Claude Code Desktop"),
                       "a session whose model never answered has no consumer row but still belongs to Claude")
    }
}
