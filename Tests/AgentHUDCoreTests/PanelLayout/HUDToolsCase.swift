import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tool tests share: the sample's panel, with Claude and Codex quota blocks and DeepSeek's balance, over
/// settings of their own, and the tools answering from it.
@MainActor
class HUDToolsCase: XCTestCase {
    let suite = "app.agenthud.tests.panel-tools.\(UUID().uuidString)"
    var changes = 0
    /// The sample's store over settings of its own, made when a test first reaches for either.
    lazy var store: UsageStore = {
        let suite = suite
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: UserDefaults(suiteName: suite)!, defaultAgents: DemoData.agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, clock: { DemoClock.now })
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: 48, now: DemoClock.now, stage: nil))
        // In place of the store's own listener, which would wake collection: the tests count what the settings say.
        settings.onChange = { [weak self] _ in self?.changes += 1 }
        return store
    }()
    var settings: SettingsStore { store.settings }
    /// Revisions counted from 0, so that the tests can name them; the application's own start from a drawn number.
    lazy var tools = HUDTools(store: store, launch: 0)

    override func setUp() {
        super.setUp()
        L10n.setLanguage(.en)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    var layout: PanelLayoutStore { settings.panelLayout }

    func result(_ tool: String, _ arguments: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
        guard case .result(let result, _) = tools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("\(tool) was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    func refusal(_ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        let before = (layout.revision, layout.draft)
        guard case .failure(let sentence) = tools.answer(MCPToolRequest(tool: MCPTools.draftHUD, arguments: arguments)) else {
            XCTFail("the proposal was taken", line: line)
            throw CancellationError()
        }
        XCTAssertEqual(layout.revision, before.0, "a refused proposal changes nothing", line: line)
        XCTAssertEqual(layout.draft, before.1, line: line)
        XCTAssertTrue(sentence.hasPrefix("Nothing was changed. "), sentence, line: line)
        return sentence
    }

    func blocks(_ panel: [String: JSONValue]) -> [[String: JSONValue]] {
        (panel["blocks"]?.arrayValue ?? []).compactMap(\.objectValue)
    }

    func block(_ key: String, in panel: [String: JSONValue]) throws -> [String: JSONValue] {
        try XCTUnwrap(blocks(panel).first { $0["key"] == .string(key) })
    }
}
