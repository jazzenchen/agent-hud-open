import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the MCP tools do in the application, over the sample's panel: Claude and Codex quota blocks and DeepSeek's
/// balance.
final class HUDToolsTests: HUDToolsCase {
    // MARK: Reading

    func testThePanelIsReadWithItsBlocksRowsAndWhatEachAccepts() throws {
        let panel = try result(MCPTools.getHUD)
        XCTAssertEqual(panel["revision"], .integer(0))
        XCTAssertEqual(panel["unsaved"], .bool(false))
        XCTAssertEqual(panel["hasPrevious"], .bool(false))
        XCTAssertEqual(panel["layout"], .object([:]))
        XCTAssertEqual(panel["style"], .object(["spacing": .string("regular"), "separators": .bool(true), "width": .integer(540)]))
        XCTAssertEqual(panel["countdown"], .bool(true))
        XCTAssertNil(panel["note"])
        XCTAssertEqual(blocks(panel).map { $0["key"] }, [.string("Claude"), .string("Codex"), .string("api:DeepSeek")])
        XCTAssertEqual(panel["sections"]?.arrayValue?.compactMap { $0.objectValue?["key"]?.stringValue }, ["blocks", "tokens", "sessions"])

        let claude = try block("Claude", in: panel)
        XCTAssertEqual(claude["kind"], .string("quota"))
        XCTAssertEqual(claude["shown"], .bool(true))
        XCTAssertEqual(claude["widget"], .string("bar"))
        XCTAssertEqual(claude["widgets"], .array(["bar", "ring", "number", "chart"].map(JSONValue.string)))
        XCTAssertEqual(claude["measures"], .array(["quota", "burn", "tokens", "cost"].map(JSONValue.string)))
        XCTAssertEqual(claude["offeredMeasures"], .array(["quota", "burn", "tokens", "cost"].map(JSONValue.string)))
        XCTAssertEqual(claude["countdown"], .bool(true))
        XCTAssertNil(claude["measure"], "the measure shown first is the first of measures")
        XCTAssertNil(claude["columns"], "bars have no cells")
        XCTAssertNil(claude["ringSize"])
        XCTAssertNil(claude["period"], "bars cover no period")
        let rows = (claude["rows"]?.arrayValue ?? []).compactMap(\.objectValue)
        XCTAssertEqual(rows.map { $0["key"] }, [.string("claude-session"), .string("claude-weekly"), .string("claude-weekly-fable")])
        let session = try XCTUnwrap(rows.first)
        XCTAssertEqual(session["usedPercent"], .number(12))
        XCTAssertEqual(session["level"], .string("ok"))
        XCTAssertEqual(session["state"], .string("normal"))
        XCTAssertEqual(session["shown"], .bool(true))
        XCTAssertNotNil(session["resetsAt"]?.stringValue.flatMap { try? Date($0, strategy: .iso8601) })

        let deepseek = try block("api:DeepSeek", in: panel)
        XCTAssertEqual(deepseek["kind"], .string("billing"))
        XCTAssertEqual(deepseek["widgets"], .array([.string("number"), .string("chart")]))
        XCTAssertEqual(deepseek["measures"], .array(["quota", "burn", "cost"].map(JSONValue.string)))
        XCTAssertEqual(deepseek["offeredMeasures"], .array(["quota", "burn", "cost"].map(JSONValue.string)))
        for property in ["countdown", "columns", "ringSize"] {
            XCTAssertNil(deepseek[property], "a balance has no \(property) to set")
        }
        XCTAssertEqual(deepseek["period"], .string("day"), "its estimated cost covers the token chart's period")
        let balance = try XCTUnwrap(deepseek["balances"]?.arrayValue?.first?.objectValue)
        XCTAssertEqual(balance["currency"], .string("CNY"))
        XCTAssertEqual(balance["amount"], .string("8.85"))
        XCTAssertEqual(balance["level"], .string("warning"))
    }

    /// The app and the agent it answers are the person's own, so a block names its accounts: by the id a layout names
    /// one by, with the email or name the panel writes and its plan, and each row says whose it is.
    func testABlockNamesItsAccountsAndEachRowItsAccount() throws {
        let claude = try block("Claude", in: try result(MCPTools.getHUD))
        XCTAssertEqual(claude["accounts"], .array([.object([
            "id": .string(DemoData.claudeAccount.id), "name": .string("Personal"), "plan": .string("Max x20"),
            "current": .bool(true), "shown": .bool(true),
        ])]))
        let rows = (claude["rows"]?.arrayValue ?? []).compactMap(\.objectValue)
        XCTAssertEqual(rows.map { $0["account"]?.stringValue }, Array(repeating: DemoData.claudeAccount.id, count: 3))
        let deepseek = try block("api:DeepSeek", in: try result(MCPTools.getHUD))
        XCTAssertEqual(deepseek["accounts"], .array([.object([
            "id": .string("DeepSeek"), "name": .string("DeepSeek · API"), "shown": .bool(true),
        ])]))
        XCTAssertEqual(deepseek["balances"]?.arrayValue?.first?.objectValue?["account"], .string("DeepSeek"))
    }

    /// Settings' Token usage switch is the shared layout's tokens section, so a section switched off there is one the
    /// layout hides, read as such, and nothing says it apart.
    func testASectionSwitchedOffInSettingsIsOneTheLayoutHides() throws {
        settings.panelLayout.setSwitches { $0.tokenUsage = false }
        let panel = try result(MCPTools.getHUD)
        let sections = (panel["sections"]?.arrayValue ?? []).compactMap(\.objectValue)
        XCTAssertEqual(try XCTUnwrap(sections.first { $0["key"] == .string("tokens") }), ["key": .string("tokens"), "shown": .bool(false)])
        XCTAssertEqual(panel["layout"], .object(["hiddenSections": .array([.string("tokens")])]))
        XCTAssertEqual(panel["unsaved"], .bool(false), "a person's switch is saved at once")
    }

    /// What the whole app is set to is read with the panel as Settings has it, the language its names are written in
    /// beside the one chosen, and every tool's answer carries it; a proposal that leaves the app out changes none of it.
    func testTheAppsSettingsAreReadAndALayoutLeavesThem() throws {
        XCTAssertEqual(try result(MCPTools.getHUD)["app"], .object([
            "appearance": .string("system"), "matchScreen": .bool(false), "botLogos": .bool(true),
            "showMenuBarIcon": .bool(true), "language": .string("system"), "writtenIn": .string("en"),
            "hover": .object(["pressOptionToOpen": .bool(false), "openAtTopEdge": .bool(false),
                              "delayMs": .integer(400), "collapseDelayMs": .integer(200)]),
        ]), "the defaults, with the names in English as the tests write them")
        XCTAssertEqual(try result(MCPTools.getHUD)["appUnsaved"], .bool(false))

        settings.update {
            $0.appearance = .light
            $0.appearanceFollowsScreen = true
            $0.botLogos = false
            $0.showMenuBarIcon = false
            $0.language = .zhHans
            $0.requiresOptionToOpen = true
            $0.openImmediatelyAtTop = true
            $0.hoverDelayMs = 650
            $0.collapseDelayMs = 0
        }
        changes = 0
        let saved = settings.settings
        let app: JSONValue = .object([
            "appearance": .string("light"), "matchScreen": .bool(true), "botLogos": .bool(false),
            "showMenuBarIcon": .bool(false), "language": .string("zh-Hans"), "writtenIn": .string("zh-Hans"),
            "hover": .object(["pressOptionToOpen": .bool(true), "openAtTopEdge": .bool(true),
                              "delayMs": .integer(650), "collapseDelayMs": .integer(0)]),
        ])
        XCTAssertEqual(try result(MCPTools.getHUD)["app"], app)
        XCTAssertEqual(try result(MCPTools.draftHUD, ["hiddenSections": .array([.string("tokens")])])["app"], app,
                       "a proposal answers with the panel as get_hud reads it")
        XCTAssertNil(layout.appDraft, "a layout leaves the app's settings")
        XCTAssertEqual(try result(MCPTools.discardHUDDraft)["app"], app)
        XCTAssertEqual(settings.settings, saved)
        XCTAssertEqual(changes, 0, "no tool writes the settings")
    }

    /// A window's name is how the person who looks at the panel calls the row, so it follows the application's
    /// language as the panel does; keys, notes and refusals do not.
    func testARowIsNamedAsThePanelNamesItInTheApplicationsLanguage() throws {
        func claude() throws -> [[String: JSONValue]] {
            (try block("Claude", in: try result(MCPTools.getHUD))["rows"]?.arrayValue ?? []).compactMap(\.objectValue)
        }
        XCTAssertEqual(try claude().map { $0["name"] },
                       [.string("Current session"), .string("Weekly limit · All models"), .string("Weekly limit · Fable")])
        let keys = try claude().map { $0["key"] }
        L10n.setLanguage(.zhHans)
        XCTAssertEqual(try claude().map { $0["name"] }, [.string("当前会话"), .string("每周限制 · 所有模型"), .string("每周限制 · Fable")])
        XCTAssertEqual(try claude().map { $0["key"] }, keys)
        XCTAssertEqual(try claude().compactMap { $0["name"]?.stringValue },
                       (store.rowGroups.first { $0.vendor == "Claude" }?.rows ?? []).map(\.agent.name), "as the panel's rows are named")
        XCTAssertEqual(try result(MCPTools.discardHUDDraft)["note"], .string("There is no draft now. The panel shows the saved layout."))
        XCTAssertTrue(try refusal(["order": .array([.string("Zed")])]).contains("No block has the key \"Zed\""))
    }

    // MARK: Proposing

    func testAProposalIsADraftThatNothingSavesAndNothingElseHears() throws {
        let agents = settings.agents
        changes = 0
        let panel = try result(MCPTools.draftHUD, [
            "blocks": .object(["Claude": .object(["widget": .string("ring"), "measures": .array([.string("burn"), .string("quota")]),
                                                  "hiddenRows": .array([.string("claude-weekly-fable")])]),
                               "api:DeepSeek": .object(["hidden": .bool(true)])]),
            "order": .array([.string("Codex")]), "hiddenSections": .array([.string("tokens")]),
        ])
        XCTAssertTrue(layout.hasDraft)
        XCTAssertTrue(layout.current.isDefault, "only a person's Save keeps it")
        XCTAssertNil(UserDefaults(suiteName: suite)?.data(forKey: SettingsStore.Keys.panelLayout))
        XCTAssertEqual(settings.agents, agents)
        XCTAssertEqual(changes, 0, "a proposal wakes no collection")

        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(panel["revision"], .integer(1))
        XCTAssertTrue(panel["note"]?.stringValue?.contains("unsaved draft") == true)
        XCTAssertEqual(blocks(panel).map { $0["key"] }, [.string("Codex"), .string("Claude"), .string("api:DeepSeek")],
                       "shown blocks in the layout's order, then the hidden ones")
        let claude = try block("Claude", in: panel)
        XCTAssertEqual(claude["widget"], .string("ring"))
        XCTAssertEqual(claude["measures"], .array([.string("burn"), .string("quota")]))
        XCTAssertEqual(claude["offeredMeasures"], .array(["quota", "burn", "tokens", "cost"].map(JSONValue.string)))
        XCTAssertEqual(claude["columns"], .integer(4))
        XCTAssertEqual(claude["ringSize"], .string("medium"))
        let fable = try XCTUnwrap(claude["rows"]?.arrayValue?.compactMap(\.objectValue).first { $0["key"] == .string("claude-weekly-fable") })
        XCTAssertEqual(fable["shown"], .bool(false))
        XCTAssertEqual(try block("api:DeepSeek", in: panel)["shown"], .bool(false))
        let tokens = try XCTUnwrap(panel["sections"]?.arrayValue?.compactMap(\.objectValue).first { $0["key"] == .string("tokens") })
        XCTAssertEqual(tokens["shown"], .bool(false))
    }

    func testProposingTheSavedLayoutLeavesNoDraft() throws {
        let panel = try result(MCPTools.draftHUD, ["sections": .array(["blocks", "tokens", "sessions"].map(JSONValue.string))])
        XCTAssertFalse(layout.hasDraft)
        XCTAssertEqual(panel["unsaved"], .bool(false))
        XCTAssertTrue(panel["note"]?.stringValue?.contains("already saved") == true)
    }

    /// An agent reads every block's widget and measure and may send them all back. Spelled out, the saved layout is
    /// still the saved layout: nothing waits to be saved, and a default view that is saved is the default layout. A
    /// block's own countdown is no such spelling: it holds whichever way the layout's is turned later, so it is kept.
    func testAProposalThatDrawsWhatIsSavedLeavesNoDraft() throws {
        let everyKey = JSONValue.array(["Claude", "Codex", "api:DeepSeek"].map(JSONValue.string))
        let spelled = try result(MCPTools.draftHUD, [
            "order": everyKey,
            "blocks": .object(["Claude": .object(["widget": .string("bar"), "measures": .array(["quota", "burn", "tokens", "cost"].map(JSONValue.string)),
                                                  "hidden": .bool(false)]),
                               "api:DeepSeek": .object(["widget": .string("number")])]),
            "style": .object(["spacing": .string("regular"), "separators": .bool(true)]),
        ])
        XCTAssertEqual(spelled["unsaved"], .bool(false))
        XCTAssertTrue(spelled["note"]?.stringValue?.contains("already saved") == true)
        XCTAssertFalse(layout.hasDraft)
        XCTAssertEqual(layout.revision, 0)
        _ = try result(MCPTools.draftHUD, ["blocks": .object(["Claude": .object(["countdown": .bool(true)])])])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(countdown: true)]))
        layout.discard()

        _ = try result(MCPTools.draftHUD, ["blocks": .object(["Codex": .object(["widget": .string("ring")])]),
                                                 "order": .array([.string("Codex"), .string("Claude")])])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Codex": PanelLayout.Block(widget: .ring)], order: ["Codex"]),
                       "a draft holds only what it changes")
        layout.save()
        let again = try result(MCPTools.draftHUD, [
            "blocks": .object(["Codex": .object(["widget": .string("ring"), "columns": .integer(4), "ringSize": .string("medium")]),
                               "Claude": .object(["widget": .string("bar")])]),
            "order": .array(["Codex", "Claude", "api:DeepSeek"].map(JSONValue.string)),
        ])
        XCTAssertEqual(again["unsaved"], .bool(false), "the saved layout, with every default written out")
        XCTAssertEqual(again["layout"], try result(MCPTools.getHUD)["layout"])

        _ = try result(MCPTools.draftHUD, ["blocks": .object(["Codex": .object(["widget": .string("bar")])]), "order": everyKey])
        XCTAssertEqual(layout.draft, PanelLayout(), "the default view written out is the default layout")
        layout.save()
        XCTAssertTrue(layout.current.isDefault)
    }

    /// A layout keeps what it holds for a block or a row that is away. What `get_hud` returned can be sent back
    /// with them, and a person's Save then loses nothing of it.
    func testTheLayoutThatWasReadCanBeSentBackWithWhatItHoldsForWhatIsAway() throws {
        layout.propose(PanelLayout(blocks: ["Codex": PanelLayout.Block(widget: .ring),
                                            "Claude": PanelLayout.Block(hiddenRows: ["claude-weekly-fable"])], order: ["Codex", "Claude"]))
        layout.save()
        // Switched off in Settings: Codex's windows, and the one window of Claude's that the layout hides.
        let away = settings.agents.filter { $0.vendor == "Codex" || $0.windowKey == "claude-weekly-fable" }
        for agent in away { settings.setAgent(id: agent.id, enabled: false) }
        let read = try result(MCPTools.getHUD)
        XCTAssertEqual(blocks(read).map { $0["key"] }, [.string("Claude"), .string("api:DeepSeek")])
        let held = try XCTUnwrap(read["layout"]?.objectValue)
        XCTAssertEqual(held["order"], .array([.string("Codex"), .string("Claude")]))

        XCTAssertEqual(try result(MCPTools.draftHUD, held)["unsaved"], .bool(false), "sent back as it was read, it is the saved layout")
        var changed = held
        changed["hiddenSections"] = .array([.string("tokens")])
        let panel = try result(MCPTools.draftHUD, changed)
        XCTAssertEqual(panel["unsaved"], .bool(true))
        let draft = try XCTUnwrap(layout.draft)
        XCTAssertEqual(draft.blocks, layout.current.blocks, "what is held for Codex and for Claude's window is in the draft")
        XCTAssertEqual(draft.order.first, "Codex")
        XCTAssertEqual(draft.hiddenSections, [.tokens])

        // Changing what is held for a block that is away is asking for something the panel cannot weigh.
        var rewritten = changed
        rewritten["blocks"] = .object(["Codex": .object(["widget": .string("chart"), "measures": .array([.string("tokens")])])])
        XCTAssertTrue(try refusal(rewritten).contains("blocks.Codex: No block has the key \"Codex\" now. The layout holds it"))
        XCTAssertTrue(try refusal(["blocks": .object(["Claude": .object(["hiddenRows": .array([.string("claude-monthly")])])])])
            .contains("blocks.Claude.hiddenRows[0]:"))

        layout.save()
        for agent in away { settings.setAgent(id: agent.id, enabled: true) }
        let back = try result(MCPTools.getHUD)
        XCTAssertEqual(blocks(back).first?["key"], .string("Codex"), "Codex is first again")
        XCTAssertEqual(try block("Codex", in: back)["widget"], .string("ring"))
        let fable = try XCTUnwrap(try block("Claude", in: back)["rows"]?.arrayValue?.compactMap(\.objectValue)
            .first { $0["key"] == .string("claude-weekly-fable") })
        XCTAssertEqual(fable["shown"], .bool(false))
    }

    func testTheBuiltInAndThePreviousLayoutCanBeProposed() throws {
        XCTAssertTrue(try refusal(["from": .string("previous")]).contains("no previous saved layout"))
        layout.propose(PanelLayout(blocks: ["Claude": PanelLayout.Block(widget: .ring)]))
        layout.save()
        XCTAssertEqual(try result(MCPTools.draftHUD, ["from": .string("default")])["unsaved"], .bool(true))
        XCTAssertEqual(layout.draft, PanelLayout())
        layout.save()
        XCTAssertEqual(try result(MCPTools.draftHUD, ["from": .string("previous")])["unsaved"], .bool(true))
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(widget: .ring)]))
        XCTAssertTrue(try refusal(["from": .string("last")]).contains("\"default\", \"previous\" or \"shared\""))
        XCTAssertTrue(try refusal(["from": .string("shared")]).contains("from \"shared\" is for a screen"),
                      "the shared layout cannot be asked to use itself")
        XCTAssertTrue(try refusal(["from": .string("default"), "order": .array([])]).contains("either from or a layout"))
        // A key given as null is not given: this proposes the default view, which is what is saved, so the draft goes.
        XCTAssertTrue(layout.hasDraft)
        let nulls: [String: JSONValue] = ["from": .string("default"), "order": .null, "blocks": .null, "revision": .null]
        XCTAssertEqual(try result(MCPTools.draftHUD, nulls)["unsaved"], .bool(false))
        XCTAssertFalse(layout.hasDraft)
    }

    func testARevisionThatIsNoLongerCurrentIsRefused() throws {
        _ = try result(MCPTools.draftHUD, ["order": .array([.string("Codex")]), "revision": .integer(0)])
        XCTAssertEqual(layout.revision, 1)
        XCTAssertTrue(try refusal(["order": .array([.string("Claude")]), "revision": .integer(0)]).contains("revision is now 1"))
        XCTAssertTrue(try refusal(["revision": .string("1")]).contains("whole number"))
        _ = try result(MCPTools.draftHUD, ["order": .array([.string("Claude")]), "revision": .integer(1)])
    }

    /// The store counts its changes from 0 at every launch. The revisions an agent is given start from a number drawn
    /// at launch, so one read before a relaunch does not pass for the current one after it.
    func testARevisionReadBeforeARelaunchIsNotCurrentAfterIt() throws {
        let before = HUDTools(store: store, launch: 482_000)
        guard case .result(let read, _) = before.answer(MCPToolRequest(tool: MCPTools.getHUD)) else { return XCTFail("get_hud was refused") }
        XCTAssertEqual(read["revision"], .integer(482_000))

        let relaunched = SettingsStore(defaults: try XCTUnwrap(UserDefaults(suiteName: suite)), defaultAgents: DemoData.agents)
        let later = UsageStore(provider: DemoUsageProvider(), settings: relaunched, clock: { DemoClock.now })
        later.replace(report: DemoUsageProvider.report(agents: relaunched.agents, historyHours: 48, now: DemoClock.now, stage: nil))
        XCTAssertEqual(relaunched.panelLayout.revision, layout.revision, "both stores have counted no change")
        let after = HUDTools(store: later, launch: 917_000)
        let stale = MCPToolRequest(tool: MCPTools.draftHUD, arguments: ["order": .array([.string("Codex")]), "revision": .integer(482_000)])
        guard case .failure(let sentence) = after.answer(stale) else { return XCTFail("a revision of another launch passed") }
        XCTAssertTrue(sentence.contains("revision is now 917000"), sentence)
        XCTAssertFalse(relaunched.panelLayout.hasDraft)
        guard case .result = before.answer(stale) else { return XCTFail("the launch that gave the revision takes it") }

        // The application's own tools do not count from 0.
        guard case .result(let own, _) = HUDTools(store: store).answer(MCPToolRequest(tool: MCPTools.getHUD)),
              case .integer(let revision)? = own["revision"] else { return XCTFail("get_hud gave no revision") }
        XCTAssertGreaterThanOrEqual(revision, 1_000)
        XCTAssertEqual(revision % 1_000, Int64(layout.revision))
    }

    /// What stored layouts forgive, an agent is told: a forgiven name would be a change that silently did not happen.
    func testAProposalThePanelCannotDoAsAskedIsRefusedWholeWithEveryReason() throws {
        let unknown = try refusal(["block": .object([:])])
        XCTAssertTrue(unknown.contains("Unknown: \"block\""), unknown)
        let names = try refusal(["blocks": .object(["Claude": .object(["widget": .string("gauge"), "measures": .array([.string("speed"), .integer(2)]),
                                                                     "hide": .bool(true), "hidden": .string("yes"), "measure": .string("burn")])]),
                                 "sections": .array([.string("weather")]), "order": .string("Codex")])
        for part in ["blocks.Claude.widget is one of \"bar\", \"ring\", \"number\", \"chart\"",
                     "blocks.Claude.measures[0]: \"speed\" is not one of \"quota\", \"burn\", \"tokens\", \"cost\"",
                     "blocks.Claude.measures[1] is a string", "blocks.Claude has no property \"hide\"",
                     "blocks.Claude has no property \"measure\"; a block takes widget, measures, hidden, hiddenRows, rows, accounts, countdown, columns, ringSize, span and period.",
                     "blocks.Claude.hidden is true or false", "sections[0]: \"weather\" is not one of", "order is a list"] {
            XCTAssertTrue(names.contains(part), "\(part) — \(names)")
        }
        let rules = try refusal(["blocks": .object(["api:DeepSeek": .object(["widget": .string("ring")]),
                                                    "Zed": .object(["hidden": .bool(true)]),
                                                    "Codex": .object(["hiddenRows": .array([.string("nope")])])])])
        for part in ["blocks.api:DeepSeek.widget:", "has no ceiling", "blocks.Zed: No block has the key \"Zed\"",
                     "blocks.Codex.hiddenRows[0]:"] {
            XCTAssertTrue(rules.contains(part), "\(part) — \(rules)")
        }
        // Both kinds in one answer: a name that would be forgiven, and what the rest asks that the panel cannot do.
        let both = try refusal(["blocks": .object(["Claude": .object(["widget": .string("gauge")]),
                                                   "api:DeepSeek": .object(["measures": .array([.string("tokens")])])]),
                                "order": .array([.string("Zed")])])
        for part in ["blocks.Claude.widget is one of", "blocks.api:DeepSeek.measures[0]:", "order[0]: No block has the key \"Zed\""] {
            XCTAssertTrue(both.contains(part), "\(part) — \(both)")
        }
    }

    func testDiscardingDropsTheDraftAndAnUnknownToolIsRefused() throws {
        layout.propose(PanelLayout(order: ["Codex"]))
        let panel = try result(MCPTools.discardHUDDraft)
        XCTAssertFalse(layout.hasDraft)
        XCTAssertEqual(panel["unsaved"], .bool(false))
        XCTAssertNotNil(panel["note"])
        XCTAssertEqual(tools.answer(MCPToolRequest(tool: "save_panel_layout")), .failure("Agent HUD has no tool named \"save_panel_layout\"."))
    }

    /// An argument a tool does not take is something its caller believes was heeded: a revision that nothing checked.
    func testAToolThatTakesOnlyAScreenRefusesAnyOtherArgument() throws {
        layout.propose(PanelLayout(order: ["Codex"]))
        XCTAssertEqual(tools.answer(MCPToolRequest(tool: MCPTools.discardHUDDraft, arguments: ["revision": .integer(999), "junk": .bool(true)])),
                       .failure("Nothing was changed. discard_hud_draft takes only screen. Unknown: \"junk\", \"revision\"."))
        XCTAssertTrue(layout.hasDraft, "the draft a refused call was to drop is still there")
        XCTAssertEqual(tools.answer(MCPToolRequest(tool: MCPTools.getHUD, arguments: ["junk": .bool(true)])),
                       .failure("get_hud takes only screen. Unknown: \"junk\"."))
        XCTAssertEqual(try result(MCPTools.discardHUDDraft, ["screen": .string("all")])["unsaved"], .bool(false))
    }

    // MARK: Through the channel

    /// A line a client writes to the `--mcp` process reaches the tools in the application over the socket, and the
    /// draft is there when the answer comes back.
    func testAToolCallFromAClientsLineChangesTheDraftInTheApplication() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("pt-" + UUID().uuidString.prefix(8))
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        let tools = tools
        let channel = MCPAppChannel(directory: directory) { tools.answer($0) }
        XCTAssertTrue(channel.start())
        addTeardownBlock { await channel.stop() }
        let path = channel.path
        let line = #"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"draft_hud","arguments":{"blocks":{"Codex":{"widget":"number"}}}}}"#

        var answer: [String: JSONValue] = [:]
        for _ in 0..<300 {
            let lines = await Task.detached {
                MCPServer(version: "0", call: { MCPAppClient.call($0, socket: path, timeout: 5) }).lines(for: line)
            }.value
            answer = try XCTUnwrap(JSONValue.parse(Data(try XCTUnwrap(lines.first).utf8)).objectValue)
            let text = answer["result"]?.objectValue?["content"]?.arrayValue?.first?.objectValue?["text"]?.stringValue ?? ""
            // The listener comes up on the main queue, so the first calls may find nobody serving yet.
            guard text.contains("not running") else { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(answer["id"], .integer(7))
        let result = try XCTUnwrap(answer["result"]?.objectValue)
        XCTAssertNotEqual(result["isError"], .bool(true))
        let text = try XCTUnwrap(result["content"]?.arrayValue?.first?.objectValue?["text"]?.stringValue)
        let panel = try XCTUnwrap(JSONValue.parse(Data(text.utf8)).objectValue)
        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Codex": PanelLayout.Block(widget: .number)]))
    }
}
