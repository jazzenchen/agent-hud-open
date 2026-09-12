import XCTest
@testable import AgentHUDCore

final class SessionSourceTests: XCTestCase {
    @MainActor
    func testPreviewIncludesPiBehindThreeClientsAndEveryRunningSession() {
        let defaults = UserDefaults(suiteName: "AgentHUDPreviewTests.\(UUID())")!
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults))
        let now = Date()
        func session(_ id: String, _ source: String, live: Bool = false) -> LiveSession {
            .init(id: id, agentId: "\(source.lowercased())-model:test", task: source, terminal: nil,
                startedAt: now, endedAt: live ? nil : now, pctOfWindow: nil, tokensIn: 2262, tokensOut: 35, client: source)
        }
        let sessions = [session("claude", "Claude"), session("codex", "Codex"), session("dsh", "DeepSeek"),
                        session("pi", "Pi"), session("old-pi", "Pi"), session("running-pi", "Pi", live: true)]
        XCTAssertEqual(store.sessionPreview(from: sessions).map(\.id), ["claude", "codex", "dsh", "pi", "running-pi"])
        XCTAssertEqual(store.sessionPreview(from: sessions.filter { $0.client == "Pi" }).map(\.id), ["pi", "running-pi"])
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
