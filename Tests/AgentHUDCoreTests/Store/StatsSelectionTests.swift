import XCTest
@testable import AgentHUDCore

@MainActor
final class StatsSelectionTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    private func store(agents: [AgentDescriptor] = []) -> UsageStore {
        UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(
            defaults: UserDefaults(suiteName: "AgentHUDTests.\(UUID().uuidString)")!, defaultAgents: agents))
    }

    private func row(_ credential: OpenAgentCredential) -> AgentDescriptor {
        .init(id: credential.pool.windowID("rolling"), vendor: credential.pool.provider, model: "5-hour limit",
              source: "fixture", enabled: true, billingPool: credential.pool)
    }

    func testGoShowsOpenCodeTokensWithoutAssigningThemToTheCurrentPool() throws {
        let go = OpenAgentCredentials.credential(.go, token: "fixture-go", client: "OpenCode")
        let quota = row(go), store = store(agents: [quota])
        let consumer = AgentDescriptor(id: "opencode-model:muse-spark-1.3-contributor#opencode", vendor: "OpenCode",
                                       model: "muse-spark-1.3-contributor", source: "fixture", enabled: true)
        store.replace(report: UsageReport(generatedAt: now,
            snapshots: [.init(agentId: quota.id, remainingPct: 99, updatedAt: now)], sessions: [], consumers: [consumer],
            usage: [.init(start: now.addingTimeInterval(-900), agentId: consumer.id, tokensIn: 3_700_000, tokensOut: 100_000)],
            services: [.init(client: "OpenCode", provider: go.pool.provider, product: .plan, accountID: go.pool.id)]))

        XCTAssertEqual(store.shownAgents, ["OpenCode"])
        XCTAssertEqual(store.agentUsage.map(\.vendor), ["OpenCode"])
        XCTAssertEqual(store.tokenDimensions.count(try XCTUnwrap(store.agentUsage.first).tokens), 3_800_000)
        XCTAssertEqual(store.tokenColumns.reduce(0) { $0 + $1.total }, 3_800_000)
        XCTAssertEqual(store.tokenCardVendors(for: quota.id), ["OpenCode"])
        XCTAssertNil(store.consumers.first?.billingPool, "a card relationship does not prove historical billing ownership")
        XCTAssertEqual(store.rows.first?.agent.billingPool, go.pool, "quota ownership stays with Go")
    }

    func testConfiguredAPIClientsShowWithoutQuotaRowsOrTokenRecords() {
        let store = store()
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: [], services: [
            .init(client: "OpenCode", provider: "Anthropic", product: .api),
            .init(client: "Pi", provider: "OpenAI", product: .api),
        ]))
        XCTAssertEqual(store.shownAgents, ["OpenCode", "Pi"])
        XCTAssertTrue(store.agentUsage.isEmpty)
    }

    func testRecordedClientsShowIndependentlyOfQuotaSwitches() {
        let quota = AgentDescriptor(id: "claude-session", vendor: "Claude", model: "5h", source: "fixture", enabled: false)
        let store = store(agents: [quota])
        let consumers = [
            AgentDescriptor(id: "claude-model:opus", vendor: "Claude", model: "Opus", source: "fixture", enabled: true),
            AgentDescriptor(id: "opencode-model:m#anthropic", vendor: "OpenCode", model: "m", source: "fixture", enabled: true),
        ]
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: [], consumers: consumers))
        XCTAssertEqual(store.shownAgents, ["Claude", "OpenCode"])
        XCTAssertTrue(store.enabledAgents.isEmpty)
    }

    func testSharedPoolFocusUsesOnlyTheClientsOfThatAccount() {
        let shared = OpenAgentCredentials.credential(.kimi, token: "shared-key", client: "OpenCode")
        let other = OpenAgentCredentials.credential(.kimi, token: "other-key", client: "Kimi")
        let quota = row(shared), otherQuota = row(other), store = store(agents: [quota, otherQuota])
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: [], services: [
            .init(client: "OpenCode", provider: "Kimi", product: .plan, accountID: shared.pool.id),
            .init(client: "Pi", provider: "Kimi", product: .plan, accountID: shared.pool.id),
            .init(client: "Kimi", provider: "Kimi", product: .plan, accountID: other.pool.id),
        ]))
        XCTAssertEqual(store.tokenCardVendors(for: quota.id), ["OpenCode", "Pi"])
        XCTAssertEqual(store.tokenCardVendors(for: otherQuota.id), ["Kimi"])
        XCTAssertEqual(store.shownAgents, ["OpenCode", "Pi", "Kimi"])
        XCTAssertEqual(store.tokenCardVendors(for: "missing"), [])
    }

    func testNativeQuotaOnlyClientsAndExplicitSelectionsKeepTheirBehavior() {
        let claude = AgentDescriptor(id: "claude-session", vendor: "Claude", model: "5h", source: "fixture", enabled: true)
        let disabled = AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: "fixture", enabled: false)
        let disconnected = AgentDescriptor(id: "grok", vendor: "Grok", model: "5h", source: "fixture", enabled: true, connected: false)
        let store = store(agents: [claude, disabled, disconnected])
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: []))
        XCTAssertEqual(store.shownAgents, ["Claude"])
        XCTAssertEqual(store.tokenCardVendors(for: claude.id), ["Claude"])

        store.pickedAgents = ["Codex"]
        XCTAssertEqual(store.shownAgents, ["Codex"])
        store.pickedAgents = []
        XCTAssertTrue(store.shownAgents.isEmpty, "an explicit empty selection stays empty")
        store.pickedAgents = nil
        XCTAssertEqual(store.shownAgents, ["Claude"])
    }
}
