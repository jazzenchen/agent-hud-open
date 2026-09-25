import AgentHUDSupport
import XCTest
@testable import AgentHUDCore

final class LiveStatusTests: XCTestCase {
    @MainActor
    func testEveryAgentUsesTheSamePreferenceWithoutChangingUsageOrSourceState() throws {
        let suite = "LiveStatusTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [])
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        let now = Date()
        let consumers = SessionSource.agentVendors.map {
            AgentDescriptor(id: $0.lowercased() + "-model:test", vendor: $0, model: "Model", source: "", enabled: false)
        }
        let sessions = consumers.map {
            LiveSession(id: $0.id, agentId: $0.id, task: "Task", terminal: nil,
                startedAt: now.addingTimeInterval(-60), pctOfWindow: nil, tokensIn: 10, tokensOut: 2)
        }
        let buckets = consumers.map {
            UsageBucket(start: Date(timeIntervalSince1970: (now.timeIntervalSince1970 / 900).rounded(.down) * 900), agentId: $0.id, tokensIn: 10, tokensOut: 2)
        }
        let report = UsageReport(generatedAt: now, snapshots: [], sessions: sessions, consumers: consumers, usage: buckets)
        store.replace(report: report)
        let columns = store.tokenColumns
        for vendor in SessionSource.agentVendors {
            settings.update { $0.setLiveStatus(for: vendor, enabled: false) }
            XCTAssertEqual(store.liveSessions.count, sessions.count - 1, vendor)
            XCTAssertFalse(store.liveSessions.contains { store.sessionSource($0).vendor == vendor })
            XCTAssertEqual(Set(store.sessions.map(\.id)), Set(sessions.map(\.id)))
            XCTAssertEqual(store.tokenColumns, columns)
            XCTAssertEqual(store.report, report, "Preferences must not turn source observations into fake ended events")
            let reloaded = SettingsStore(defaults: defaults)
            XCTAssertFalse(reloaded.settings.liveStatusEnabled(for: vendor.lowercased()))
            settings.update { $0.setLiveStatus(for: vendor, enabled: true) }
            XCTAssertEqual(store.liveSessions.count, sessions.count)
        }
        XCTAssertTrue(settings.settings.disabledLiveStatusSources.isEmpty)
    }

    @MainActor
    func testSessionsAreOrderedByTheirLastEventRegardlessOfRunning() {
        let suite = "LiveStatusOrderTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults))
        let now = Date()
        func session(_ id: String, startedAt: TimeInterval, endedAt: TimeInterval? = nil, observedAt: TimeInterval = 0) -> LiveSession {
            .init(id: id, agentId: "claude-model:test", task: id, terminal: nil, startedAt: now.addingTimeInterval(startedAt),
                  endedAt: endedAt.map(now.addingTimeInterval), pctOfWindow: nil, tokensIn: 10, tokensOut: 2,
                  observedAt: now.addingTimeInterval(observedAt))
        }
        func turn(_ sessionID: String, observedAt: TimeInterval) -> SessionTurn {
            .init(provider: "claude", sessionID: sessionID, turnID: sessionID, state: .running,
                  startedAtMs: nil, observedAtMs: RecordCoding.milliseconds(now.addingTimeInterval(observedAt)))
        }
        store.replace(report: UsageReport(
            generatedAt: now, snapshots: [],
            sessions: [session("quiet-run", startedAt: -600), session("ended", startedAt: -3600, endedAt: -60),
                       session("busy-run", startedAt: -7200), session("turnless-run", startedAt: -86400, observedAt: -5)],
            turns: [turn("quiet-run", observedAt: -540), turn("busy-run", observedAt: -10)]))
        XCTAssertEqual(store.sessions.map(\.id), ["turnless-run", "busy-run", "ended", "quiet-run"],
                       "A running session that has been quiet longer than another session has been finished ranks below it")
    }

    func testSettingsDecodeDefaultsAndNormalizeAtTheBoundary() throws {
        let defaults = try JSONDecoder().decode(Settings.self, from: Data("{}".utf8))
        XCTAssertTrue(SessionSource.agentVendors.allSatisfy { defaults.liveStatusEnabled(for: $0) })
        let value = try JSONDecoder().decode(Settings.self, from: Data(#"{"disabledLiveStatusSources":["Pi","CODEX"]}"#.utf8))
        XCTAssertEqual(value.disabledLiveStatusSources, ["pi", "codex"])
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: JSONEncoder().encode(value)), value)
    }

    func testLiveStatusBelongsToClientsRegardlessOfQuotaWindows() {
        let names = SessionSource.agentVendors + ["GLM", "Anthropic", "ChatGPT"]
        let sources = names.map { SourceStatus(id: $0.lowercased(), name: $0, detail: "", state: .installed) }
        let groups = AgentSettingsGroup.make(sources: sources, agents: [])
        XCTAssertEqual(Set(groups.filter(\.hasLiveStatus).map(\.id)), Set(SessionSource.agentVendors))
        XCTAssertTrue(groups.allSatisfy { $0.agents.isEmpty })
    }
}
