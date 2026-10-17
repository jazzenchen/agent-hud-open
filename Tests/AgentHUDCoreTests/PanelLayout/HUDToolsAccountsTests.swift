import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tools say and take of a block's rows and accounts, and of the Expanded panel's switches, over the sample
/// with a second Claude account, Work, signed in from another home with the session and the week.
@MainActor
final class HUDToolsAccountsTests: XCTestCase {
    private static let work = ProviderAccount.identified(provider: "Claude", user: "demo-claude-work", workspace: nil)!

    /// The sample's store with Work beside Personal, over settings of its own.
    private lazy var store: UsageStore = {
        let suite = "app.agenthud.tests.panel-accounts.\(UUID().uuidString)"
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let work = HUDToolsAccountsTests.work
        let workRows = DemoData.claudeRows.prefix(2).map {
            AgentDescriptor(id: work.windowID($0.windowKey), vendor: $0.vendor, model: $0.model, shortModel: $0.shortModel,
                            source: $0.source, enabled: true, account: work)
        }
        let agents = DemoData.claudeRows + workRows + DemoData.agents.filter { $0.vendor != "Claude" }
        let settings = SettingsStore(defaults: UserDefaults(suiteName: suite)!, defaultAgents: agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, clock: { DemoClock.now })
        let now = DemoClock.now
        let sample = DemoUsageProvider.report(agents: DemoData.agents, historyHours: 48, now: now, stage: nil)
        var accounts = sample.accounts ?? [:]
        accounts["Claude", default: []].append(AccountObservation(account: work, home: "work", label: "jazzen@work.example",
                                                                  plan: "pro", observedAt: now))
        let snapshots = workRows.map {
            UsageSnapshot(agentId: $0.id, remainingPct: 70, resetAt: now.addingTimeInterval(3600), updatedAt: now)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: sample.snapshots + snapshots, sessions: sample.sessions,
                                          discoveredAgents: sample.discoveredAgents + workRows, consumers: sample.consumers,
                                          usage: sample.usage, insightsByAgent: sample.insightsByAgent,
                                          subscriptions: sample.subscriptions, consumerIdsByQuota: sample.consumerIdsByQuota,
                                          billing: sample.billing, accounts: accounts))
        return store
    }()
    private var layout: PanelLayoutStore { store.settings.panelLayout }
    private lazy var tools = HUDTools(store: store, launch: 0)

    override func setUp() {
        super.setUp()
        L10n.setLanguage(.en)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    private func result(_ tool: String, _ arguments: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
        guard case .result(let result, _) = tools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("\(tool) was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    private func refusal(_ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        let revision = layout.revision
        guard case .failure(let sentence) = tools.answer(MCPToolRequest(tool: MCPTools.draftHUD, arguments: arguments)) else {
            XCTFail("the proposal was taken", line: line)
            throw CancellationError()
        }
        XCTAssertEqual(layout.revision, revision, "a refused proposal changes nothing", line: line)
        return sentence
    }

    private func claude(_ panel: [String: JSONValue]) throws -> [String: JSONValue] {
        try XCTUnwrap((panel["blocks"]?.arrayValue ?? []).compactMap(\.objectValue).first { $0["key"] == .string("Claude") })
    }

    private func rows(_ block: [String: JSONValue]) -> [[String: JSONValue]] { (block["rows"]?.arrayValue ?? []).compactMap(\.objectValue) }

    private func claudeChange(_ change: [String: JSONValue]) -> [String: JSONValue] {
        ["blocks": .object(["Claude": .object(change)])]
    }

    // MARK: Accounts

    /// A block lists its accounts in the order the panel draws them, each by the id a proposal names it by, with its
    /// email or name and its plan, and its rows account by account, each with its account.
    func testABlockListsItsAccountsAndItsRowsAccountByAccount() throws {
        let personal = DemoData.claudeAccount.id, work = Self.work.id
        let block = try claude(try result(MCPTools.getHUD))
        XCTAssertEqual(block["accounts"], .array([
            .object(["id": .string(personal), "name": .string("Personal"), "plan": .string("Max x20"), "current": .bool(true),
                     "shown": .bool(true)]),
            .object(["id": .string(work), "name": .string("jazzen@work.example"), "plan": .string("Pro"), "current": .bool(true),
                     "shown": .bool(true)]),
        ]))
        XCTAssertEqual(rows(block).map { [$0["account"]?.stringValue, $0["key"]?.stringValue] }, [
            [personal, "claude-session"], [personal, "claude-weekly"], [personal, "claude-weekly-fable"],
            [work, "claude-session"], [work, "claude-weekly"],
        ])
    }

    /// An agent chooses the accounts a block shows: the others are read as not shown, and so are their rows, and the
    /// layout that was read, sent back, changes nothing.
    func testAProposalChoosesTheAccountsABlockShows() throws {
        let work = Self.work.id
        let panel = try result(MCPTools.draftHUD, claudeChange(["accounts": .array([.string(work)])]))
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(accounts: [work])]))
        let block = try claude(panel)
        XCTAssertEqual((block["accounts"]?.arrayValue ?? []).compactMap { $0.objectValue?["shown"] }, [.bool(false), .bool(true)])
        XCTAssertEqual(rows(block).map { $0["shown"] }, [.bool(false), .bool(false), .bool(false), .bool(true), .bool(true)])
        let read = try result(MCPTools.getHUD)
        let revision = layout.revision
        _ = try result(MCPTools.draftHUD, read["layout"]?.objectValue ?? [:])
        XCTAssertEqual(layout.revision, revision, "the layout that was read, sent back, keeps the draft as it is")

        let every = [DemoData.claudeAccount.id, work].sorted()
        _ = try result(MCPTools.draftHUD, claudeChange(["accounts": .array(every.map(JSONValue.string))]))
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(accounts: every)]),
                       "every account the block has now still leaves out one that joins later")
        _ = try result(MCPTools.draftHUD, claudeChange(["accounts": .array([])]))
        XCTAssertNil(layout.draft, "no account named is every account, the saved layout")
    }

    /// The accounts a saved layout chooses stay chosen while one the block left out is away: the layout that was read,
    /// sent back with another change, keeps them, so that Save does not bring the account back with the change.
    func testTheAccountsChosenStayChosenWhileAnotherIsAway() throws {
        let work = Self.work.id
        layout.propose(PanelLayout(blocks: ["Claude": PanelLayout.Block(accounts: [work])]))
        layout.save()
        store.settings.setAccount(id: DemoData.claudeAccount.id, visible: false)
        XCTAssertEqual(store.view.panelBlocks.first { $0.key == "Claude" }?.accounts.map(\.id), [work],
                       "Personal is switched off in Settings, so Work is all the block has now")
        var arguments = try XCTUnwrap(try result(MCPTools.getHUD)["layout"]?.objectValue)
        arguments["style"] = .object(["width": .integer(600)])
        _ = try result(MCPTools.draftHUD, arguments)
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(accounts: [work])],
                                                 style: PanelStyle(width: 600)))
    }

    func testAnAccountTheBlockDoesNotHaveIsRefusedWithTheAccountsItHas() throws {
        let sentence = try refusal(claudeChange(["accounts": .array([.string("account:nobody")])]))
        XCTAssertTrue(sentence.contains("blocks.Claude.accounts[0]: \"Claude\" has no account \"account:nobody\". Its accounts: "
                                        + "\"\(DemoData.claudeAccount.id)\" (Personal), \"\(Self.work.id)\" (jazzen@work.example)."),
                      sentence)
        let misread = try refusal(claudeChange(["accounts": .string(Self.work.id)]))
        XCTAssertTrue(misread.contains("blocks.Claude.accounts is a list of account ids from get_hud."), misread)
        let balance = try refusal(["blocks": .object(["api:DeepSeek": .object(["accounts": .array([.string("pool:none")])])])])
        XCTAssertTrue(balance.contains("\"api:DeepSeek\" has no account \"pool:none\". Its accounts: \"DeepSeek\" (DeepSeek · API)."),
                      balance)
    }

    // MARK: The order of rows

    func testAProposalOrdersABlocksRowsAndGetHUDReadsThemInThatOrder() throws {
        let panel = try result(MCPTools.draftHUD, claudeChange(["rows": .array([.string("claude-weekly-fable"),
                                                                                       .string("claude-weekly")])]))
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(rows: ["claude-weekly-fable", "claude-weekly"])]))
        XCTAssertEqual(rows(try claude(panel)).map { $0["key"]?.stringValue }, [
            "claude-weekly-fable", "claude-weekly", "claude-session", "claude-weekly", "claude-session",
        ], "each account's windows in the block's order, the session after the named ones")

        let refused = try refusal(claudeChange(["rows": .array([.string("claude-opus")])]))
        XCTAssertTrue(refused.contains("blocks.Claude.rows[0]: \"Claude\" has no row \"claude-opus\". Its rows: \"claude-session\", "
                                       + "\"claude-weekly\", \"claude-weekly-fable\"."), refused)
        let misread = try refusal(claudeChange(["rows": .array([.integer(1)])]))
        XCTAssertTrue(misread.contains("blocks.Claude.rows is a list of row keys."), misread)
        _ = try result(MCPTools.draftHUD, claudeChange(["rows": .array([.string("claude-session"), .string("claude-weekly")])]))
        XCTAssertNil(layout.draft, "the rows' own order is the saved layout")
    }

    // MARK: The Expanded panel's switches

    /// The switches are the layout's: an agent's draft can show what Settings switched off, a screen's own layout keeps
    /// its own, and Reset to Default and Revert to Previous reach them like the rest of the layout.
    func testTheSwitchesAreLayoutFieldsAnAgentDrafts() throws {
        layout.setSwitches { $0.tokenUsage = false; $0.resetCountdown = false }
        let read = try result(MCPTools.getHUD)
        XCTAssertEqual(read["layout"], .object(["hiddenSections": .array([.string("tokens")]), "countdown": .bool(false)]))
        XCTAssertEqual(read["countdown"], .bool(false))
        XCTAssertEqual(try claude(read)["countdown"], .bool(false))
        XCTAssertNil(read["countdownOnInSettings"])

        let shown = try result(MCPTools.draftHUD, [:])
        XCTAssertEqual(layout.draft, PanelLayout(), "a proposal is whole: what it leaves out is the default view's")
        XCTAssertEqual(shown["countdown"], .bool(true))
        XCTAssertEqual((shown["sections"]?.arrayValue ?? []).compactMap { $0.objectValue?["shown"] },
                       [.bool(true), .bool(true), .bool(true)])
        XCTAssertEqual(layout.switches, PanelSwitches(resetCountdown: false, tokenUsage: false), "Settings shows the saved layout")
        layout.save()
        XCTAssertEqual(layout.switches, PanelSwitches(), "until a person saves the draft")
        layout.revert()
        XCTAssertEqual(layout.switches, PanelSwitches(resetCountdown: false, tokenUsage: false), "Revert to Previous brings them back")
        layout.reset()
        XCTAssertEqual(layout.switches, PanelSwitches(), "and Reset to Default gives the default view's")

        let refused = try refusal(["countdown": .string("no")])
        XCTAssertTrue(refused.contains("countdown is true or false."), refused)
    }

    /// A block's own countdown is kept as written, the one the layout has now included, so it holds whichever way the
    /// person turns Settings' Reset countdown later.
    func testABlocksOwnCountdownHoldsWhenTheSwitchTurns() throws {
        _ = try result(MCPTools.draftHUD, claudeChange(["countdown": .bool(true)]))
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(countdown: true)]))
        layout.save()
        layout.setSwitches { $0.resetCountdown = false }
        let read = try result(MCPTools.getHUD)
        XCTAssertEqual(read["countdown"], .bool(false))
        XCTAssertEqual(try claude(read)["countdown"], .bool(true), "Claude's reset times still show")
        let revision = layout.revision
        _ = try result(MCPTools.draftHUD, read["layout"]?.objectValue ?? [:])
        XCTAssertEqual(layout.revision, revision, "the layout that was read, sent back, is the saved one")
    }

    func testAScreensOwnLayoutKeepsItsOwnSwitches() throws {
        let screen = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                                 agents: ["Claude", "Codex"], systemIsLight: false, reducesTransparency: false)
        let screenTools = HUDTools(store: store, launch: 0, screens: { [screen] })
        guard case .result(let own, _) = screenTools.answer(MCPToolRequest(tool: MCPTools.draftHUD, arguments: [
            "screen": .string(screen.key), "hiddenSections": .array([.string("sessions")]),
        ])) else { return XCTFail("refused") }
        XCTAssertEqual(own["layout"], .object(["hiddenSections": .array([.string("sessions")])]))
        layout.save(.screen(screen.key))
        layout.setSwitches { $0.tokenUsage = false }
        XCTAssertEqual(layout.effective(for: screen.key), PanelLayout(hiddenSections: [.sessions]),
                       "Settings' switch is the shared layout's, which the screen does not draw")
        XCTAssertEqual(layout.effective(for: "OTHER"), PanelLayout(hiddenSections: [.tokens]))
    }
}
