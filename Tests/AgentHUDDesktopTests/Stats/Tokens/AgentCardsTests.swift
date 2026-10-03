import XCTest
import AgentHUDCore
@testable import AgentHUDDesktop

final class AgentCardsTests: XCTestCase {
    @MainActor
    func testRowsHoldThreeCardsAndNeverLeaveOneAlone() {
        let sizes = (1...10).map { AgentCards.rows(Array(0..<$0)).map(\.count) }
        XCTAssertEqual(sizes, [[1], [2], [3], [2, 2], [3, 2], [3, 3], [3, 2, 2], [3, 3, 2], [3, 3, 3], [3, 3, 2, 2]])
        XCTAssertEqual(AgentCards.rows(Array(0..<5)).flatMap { $0 }, Array(0..<5), "cards keep their order")
    }

    @MainActor
    func testHistoricalClientRemainsSelectableWithoutInstalledSourceOrCurrentRangeUsage() throws {
        let domain = "AgentHUDTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: []))
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let openCode = AgentDescriptor(id: "opencode-model:m#opencode", vendor: "OpenCode", model: "m", source: "fixture", enabled: true)
        let claude = AgentDescriptor(id: "claude-model:opus", vendor: "Claude", model: "Opus", source: "fixture", enabled: true)
        let report = UsageReport(generatedAt: now, snapshots: [], sessions: [], consumers: [openCode, claude], usage: [
            .init(start: now.addingTimeInterval(-12 * 3600), agentId: openCode.id, tokensIn: 1_000, tokensOut: 100),
            .init(start: now.addingTimeInterval(-900), agentId: claude.id, tokensIn: 1_000, tokensOut: 100),
        ])
        store.replace(report: report)
        store.setStatsRange(.hours5)
        XCTAssertEqual(store.agentUsage.map(\.vendor), ["Claude"])
        XCTAssertEqual(store.shownAgents, ["OpenCode", "Claude"])

        let sources = [
            SourceStatus(id: "claude-code", name: "Claude", detail: "fixture", state: .notDetected),
            SourceStatus(id: "opencode", name: "OpenCode", detail: "fixture", state: .notDetected),
        ]
        let resolved = SourceDetector.resolve(sources, report: report)
        let openCodeSource = try XCTUnwrap(resolved.first { $0.name == "OpenCode" })
        XCTAssertEqual(openCodeSource.state, .ready(plan: nil), "historical consumers keep a client detectable outside the selected range")
        let groups = AgentSettingsGroup.make(sources: resolved, agents: store.settings.agents, report: report)
        XCTAssertTrue(groups.contains { $0.id == "OpenCode" }, "the card picker includes clients with a resolved source or recorded usage")

        store.pickedAgents = ["Claude"]
        store.replace(report: report)
        store.setStatsRange(.hours24)
        XCTAssertEqual(Set(store.agentUsage.map(\.vendor)), ["OpenCode", "Claude"])
        XCTAssertEqual(store.shownAgents, ["Claude"], "deselecting one client survives refresh and a range change")
    }
}
