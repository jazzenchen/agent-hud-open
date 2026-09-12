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
        let events = consumers.map {
            UsageEvent(timestamp: now, agentId: $0.id, tokensIn: 10, tokensOut: 2)
        }
        let report = UsageReport(generatedAt: now, snapshots: [], sessions: sessions, history: [], activity: .empty,
            insights: .empty, consumers: consumers, consumption: events)
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

    func testSettingsDecodeDefaultsAndNormalizeAtTheBoundary() throws {
        let defaults = try JSONDecoder().decode(Settings.self, from: Data("{}".utf8))
        XCTAssertTrue(SessionSource.agentVendors.allSatisfy { defaults.liveStatusEnabled(for: $0) })
        let value = try JSONDecoder().decode(Settings.self, from: Data(#"{"disabledLiveStatusSources":["Pi","CODEX"]}"#.utf8))
        XCTAssertEqual(value.disabledLiveStatusSources, ["pi", "codex"])
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: JSONEncoder().encode(value)), value)
    }

    func testLiveStatusBelongsToClientsRegardlessOfQuotaWindows() {
        let names = SessionSource.agentVendors + ["GLM", "Anthropic", "ChatGPT"]
        let sources = names.map { SourceStatus(id: $0.lowercased(), name: $0, detail: "", state: .notDetected) }
        let groups = AgentSettingsGroup.make(sources: sources, agents: [])
        XCTAssertEqual(Set(groups.filter(\.hasLiveStatus).map(\.id)), Set(SessionSource.agentVendors))
        XCTAssertTrue(groups.allSatisfy { $0.agents.isEmpty })
    }
}
