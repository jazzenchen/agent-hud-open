import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// The tools' screen argument, over the sample's panel and two attached screens the host lists: the Mac's own display
/// beside its notch, and a second one with a Dynamic dock.
final class HUDToolsScreensTests: HUDToolsCase {
    private let builtIn = PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                      agents: ["Claude", "Codex", "DeepSeek"],
                                      systemIsLight: false, reducesTransparency: false)
    private let second = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                                     agents: ["Claude", "Codex", "Cursor"],
                                     systemIsLight: false, reducesTransparency: false)
    /// The scopes the renderer was asked to draw or measure, in order.
    private var asked: [PanelLayoutScope] = []

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] },
                                              picture: { [unowned self] scope in
        asked.append(scope)
        return PanelPicture(png: Data([0x89]), width: 540, fit: PanelFit(height: 600, maximumHeight: 1_000), scale: 2)
    }, fit: { [unowned self] scope in
        asked.append(scope)
        return PanelFit(height: scope == .shared ? 700 : 500, maximumHeight: 1_000)
    })

    private func call(_ tool: String, _ arguments: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
        guard case .result(let result, _) = screenTools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("\(tool) was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    private func refused(_ tool: String, _ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        let before = (layout.revision, layout.screenKeys)
        guard case .failure(let sentence) = screenTools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("the call was taken", line: line)
            throw CancellationError()
        }
        XCTAssertEqual(layout.revision, before.0, "a refused call changes nothing", line: line)
        XCTAssertEqual(layout.screenKeys, before.1, line: line)
        return sentence
    }

    private func screens(_ panel: [String: JSONValue]) -> [[String: JSONValue]] {
        (panel["screens"]?.arrayValue ?? []).compactMap(\.objectValue)
    }

    private func hud(_ agents: [String]) -> JSONValue { .object(["agents": .array(agents.map(JSONValue.string))]) }

    // MARK: Reading

    func testThePanelListsTheAttachedScreensByRole() throws {
        let panel = try call(MCPTools.getHUD)
        XCTAssertEqual(panel["screen"], .string("all"), "without a screen, the shared layout")
        XCTAssertNil(panel["usesSharedLayout"])
        XCTAssertEqual(panel["height"], .integer(700), "measured as the main display draws the shared layout")
        let listed = screens(panel)
        XCTAssertEqual(listed.map { $0["key"] }, [.string("BUILT-IN-UUID"), .string("SECOND-UUID")])
        let dock = try XCTUnwrap(listed.last)
        XCTAssertEqual(dock["name"], .string("Display 2"))
        XCTAssertEqual(dock["mode"], .string("dynamic"), "a mode as Agent HUD names it to a person")
        XCTAssertEqual(dock["edge"], .string("right"))
        XCTAssertEqual(dock["hasNotch"], .bool(false))
        XCTAssertEqual(dock["layout"], .string("shared"))
        XCTAssertEqual(dock["unsaved"], .bool(false))
        XCTAssertEqual(dock["agents"], .array(["Claude", "Codex", "Cursor"].map(JSONValue.string)))
        XCTAssertEqual(dock["shows"], dock["agents"], "a layout without a list shows every agent")
        XCTAssertEqual(listed.first?["mode"], .string("notch"))
        XCTAssertEqual(asked, [.shared])
    }

    func testAScreensPanelIsReadInTheLayoutItDraws() throws {
        layout.propose(.own(PanelLayout(hiddenSections: [.tokens], hud: PanelLayout.HUD(agents: ["Cursor", "Claude"]))),
                       for: "SECOND-UUID")
        layout.save(.screen("SECOND-UUID"))
        let panel = try call(MCPTools.getHUD, ["screen": .string("SECOND-UUID")])
        XCTAssertEqual(panel["screen"], .string("SECOND-UUID"))
        XCTAssertEqual(panel["usesSharedLayout"], .bool(false))
        XCTAssertEqual(panel["layout"], .object(["hiddenSections": .array([.string("tokens")]), "hud": hud(["Cursor", "Claude"])]))
        XCTAssertEqual(panel["height"], .integer(500), "measured as that display draws it")
        let tokens = try XCTUnwrap(panel["sections"]?.arrayValue?.compactMap(\.objectValue).first { $0["key"] == .string("tokens") })
        XCTAssertEqual(tokens["shown"], .bool(false))
        let dock = try XCTUnwrap(screens(panel).last)
        XCTAssertEqual(dock["layout"], .string("own"))
        XCTAssertEqual(dock["shows"], .array([.string("Cursor"), .string("Claude")]), "narrowed and in the list's order")

        let other = try call(MCPTools.getHUD, ["screen": .string("BUILT-IN-UUID")])
        XCTAssertEqual(other["usesSharedLayout"], .bool(true))
        XCTAssertEqual(other["layout"], .object([:]), "the other screen draws the shared layout")
        XCTAssertEqual(asked, [.screen("SECOND-UUID"), .screen("BUILT-IN-UUID")])
    }

    /// Each screen tells the width one agent's own panel has there on a dock that separates its agents, whether or not
    /// it separates them now: 100 points narrower than the panel its layout gives, a draft's included, and never below
    /// 440.
    func testAScreenTellsTheWidthOfAnAgentsOwnPanel() throws {
        XCTAssertEqual(screens(try call(MCPTools.getHUD)).map { $0["agentPanelWidth"] }, [.integer(440), .integer(440)],
                       "at the default 540")
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "style": .object(["width": .integer(700)])])
        var listed = screens(try call(MCPTools.getHUD))
        XCTAssertEqual(listed.map { $0["width"] }, [.integer(540), .integer(700)])
        XCTAssertEqual(listed.map { $0["agentPanelWidth"] }, [.integer(440), .integer(600)], "the draft's width less 100")
        _ = try call(MCPTools.draftHUD, ["style": .object(["width": .integer(480)])])
        listed = screens(try call(MCPTools.getHUD))
        XCTAssertEqual(listed.map { $0["width"] }, [.integer(480), .integer(700)], "the shared draft, and the screen's own")
        XCTAssertEqual(listed.map { $0["agentPanelWidth"] }, [.integer(440), .integer(600)], "never below 440")
    }

    /// A screen named by key: attached, or away with a layout of its own; anything else is refused with the screens
    /// that are attached, by key and role.
    func testAScreenIsNamedByAKeyGetHUDGave() throws {
        let unknown = try refused(MCPTools.draftHUD, ["screen": .string("Studio Display"), "order": .array([.string("Codex")])])
        XCTAssertEqual(unknown, "Nothing was changed. No screen has the key \"Studio Display\". The screens attached now: "
                       + "\"BUILT-IN-UUID\" (Built-in display), \"SECOND-UUID\" (Display 2). Use \"all\" for the shared layout.")
        XCTAssertTrue(try refused(MCPTools.getHUD, ["screen": .integer(2)]).hasPrefix("screen is a screen key"))
        XCTAssertTrue(try refused(MCPTools.discardHUDDraft, ["screen": .string("gone")]).hasPrefix("Nothing was changed. No screen"))

        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "AWAY-UUID")
        layout.save(.screen("AWAY-UUID"))
        let away = try call(MCPTools.getHUD, ["screen": .string("AWAY-UUID")])
        XCTAssertEqual(away["layout"], .object(["order": .array([.string("Codex")])]), "a screen that is away keeps its layout")
        XCTAssertFalse(screens(away).contains { $0["key"] == .string("AWAY-UUID") }, "and is not listed as attached")
        XCTAssertEqual(try call(MCPTools.getHUD, ["screen": .string("all")])["screen"], .string("all"))
    }

    // MARK: Proposing

    func testAProposalForAScreenIsThatScreensDraftAndLeavesTheShared() throws {
        let panel = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hud": hud(["Codex", "Claude"]),
                                                       "order": .array([.string("Codex")])])
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .own(PanelLayout(order: ["Codex"], hud: PanelLayout.HUD(agents: ["Codex", "Claude"]))))
        XCTAssertFalse(layout.hasDraft, "the shared layout has no draft")
        XCTAssertEqual(layout.effective(for: "BUILT-IN-UUID"), PanelLayout())
        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(panel["note"], .string("Shown in the panel on \"Display 2\" as an unsaved draft of its own. The user keeps it "
                                              + "by hovering the Agent HUD on that display and pressing Save in the panel; it is "
                                              + "dropped when Agent HUD restarts."))
        let listed = screens(panel)
        XCTAssertEqual(listed.last?["layout"], .string("own"))
        XCTAssertEqual(listed.last?["unsaved"], .bool(true))
        XCTAssertEqual(listed.first?["unsaved"], .bool(false))
        XCTAssertEqual(changes, 0, "a proposal writes no settings")

        let shared = try call(MCPTools.draftHUD, ["hiddenSections": .array([.string("sessions")])])
        XCTAssertTrue(shared["note"]?.stringValue?.hasSuffix("Screens with a layout of their own do not show it: \"Display 2\".") == true,
                      "\(shared["note"] ?? .null)")
        XCTAssertEqual(screens(shared).first?["unsaved"], .bool(true), "a shared draft shows on every screen that uses it")
    }

    func testAScreenCanBeProposedTheSharedLayoutAgain() throws {
        XCTAssertTrue(try refused(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "from": .string("previous")])
            .contains("\"Display 2\" has no previous saved layout"))
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "SECOND-UUID")
        layout.save(.screen("SECOND-UUID"))
        let panel = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "from": .string("shared")])
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .shared)
        XCTAssertEqual(panel["layout"], .object([:]), "it draws the shared layout at once")
        XCTAssertEqual(panel["usesSharedLayout"], .bool(true))
        XCTAssertTrue(panel["note"]?.stringValue?.contains("an unsaved draft that uses the shared layout") == true)
        XCTAssertEqual(try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "from": .string("default")])["layout"],
                       .object([:]))
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .own(PanelLayout()), "the default view, as the screen's own")

        _ = try call(MCPTools.discardHUDDraft, ["screen": .string("SECOND-UUID")])
        XCTAssertNil(layout.draft(of: "SECOND-UUID"))
        XCTAssertEqual(layout.ownLayout(of: "SECOND-UUID"), PanelLayout(order: ["Codex"]), "discarding saves nothing")
    }

    /// Spelled out, a screen's saved own layout is still that layout, and leaves no draft.
    func testAProposalThatDrawsWhatTheScreenHasSavedLeavesNoDraft() throws {
        layout.propose(.own(PanelLayout(blocks: ["Codex": PanelLayout.Block(widget: .ring)], hud: PanelLayout.HUD(agents: ["Codex"]))),
                       for: "SECOND-UUID")
        layout.save(.screen("SECOND-UUID"))
        let spelled = try call(MCPTools.draftHUD, [
            "screen": .string("SECOND-UUID"), "hud": hud(["Codex"]),
            "blocks": .object(["Codex": .object(["widget": .string("ring"), "columns": .integer(4)]),
                               "Claude": .object(["widget": .string("bar")])]),
        ])
        XCTAssertEqual(spelled["unsaved"], .bool(false))
        XCTAssertNil(layout.draft(of: "SECOND-UUID"))
        XCTAssertEqual(spelled["note"], .string("This is what \"Display 2\" already has saved, so there is no draft for it."))
    }

    /// The layout get_hud returned for a screen that draws the shared layout, sent back for that screen, changes
    /// nothing: it is the shared layout the screen already uses, not a copy of its own. Nor does a screen's draft to use
    /// the shared layout turn into one, and a screen whose saved choice is the shared layout drops a draft of its own.
    func testSendingBackWhatAScreenThatUsesTheSharedLayoutReadsChangesNothing() throws {
        func sendBack(_ screen: String) throws {
            let read = try call(MCPTools.getHUD, ["screen": .string(screen)])
            var arguments = try XCTUnwrap(read["layout"]?.objectValue)
            arguments["screen"] = .string(screen)
            arguments["revision"] = read["revision"]
            _ = try call(MCPTools.draftHUD, arguments)
        }
        layout.propose(PanelLayout(order: ["Codex"], hud: PanelLayout.HUD(agents: ["Codex"])))
        layout.save()
        var revision = layout.revision
        try sendBack("BUILT-IN-UUID")
        XCTAssertNil(layout.draft(of: "BUILT-IN-UUID"), "the saved shared layout, read and sent back")
        XCTAssertEqual(layout.revision, revision)

        layout.propose(PanelLayout(hiddenSections: [.tokens]))
        revision = layout.revision
        try sendBack("BUILT-IN-UUID")
        XCTAssertNil(layout.draft(of: "BUILT-IN-UUID"), "the shared draft it shows, read and sent back")
        XCTAssertEqual(layout.draftScope(on: "BUILT-IN-UUID"), .shared)
        XCTAssertEqual(layout.revision, revision)

        layout.propose(.own(PanelLayout(order: ["Cursor"])), for: "SECOND-UUID")
        layout.save(.screen("SECOND-UUID"))
        layout.propose(.shared, for: "SECOND-UUID")
        revision = layout.revision
        try sendBack("SECOND-UUID")
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .shared, "a draft to use the shared layout stays that")
        XCTAssertEqual(layout.revision, revision)

        layout.propose(.own(PanelLayout(order: ["Claude"])), for: "BUILT-IN-UUID")
        let back = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hiddenSections": .array([.string("tokens")])])
        XCTAssertNil(layout.draft(of: "BUILT-IN-UUID"), "what the screen's saved choice draws leaves no draft of its own")
        XCTAssertEqual(back["usesSharedLayout"], .bool(true))

        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hiddenSections": .array([.string("tokens")])])
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .shared, "the shared layout's draft, sent for a screen that uses it")
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "order": .array([.string("Cursor")])])
        XCTAssertNil(layout.draft(of: "SECOND-UUID"), "its own saved layout, spelled out")
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hiddenSections": .array([.string("tokens")])])
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .own(PanelLayout(hiddenSections: [.tokens])),
                       "a screen with a layout of its own is sent a copy of the shared one as its own")

        layout.discard()
        layout.useShared("SECOND-UUID")
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "order": .array([.string("Codex")]),
                                               "hud": hud(["Codex"])])
        XCTAssertNil(layout.draft(of: "SECOND-UUID"), "the saved shared layout, sent for a screen that now uses it")
    }

    /// Dropping the shared draft leaves the screens' own, and says which keep one; a draft for a screen that is away is
    /// said to wait for it.
    func testTheNotesSayWhichScreensKeepADraft() throws {
        layout.propose(.own(PanelLayout(order: ["Cursor"])), for: "SECOND-UUID")
        layout.propose(PanelLayout(order: ["Codex"]))
        let dropped = try call(MCPTools.discardHUDDraft)
        XCTAssertFalse(layout.hasDraft)
        XCTAssertEqual(layout.draft(of: "SECOND-UUID"), .own(PanelLayout(order: ["Cursor"])), "the screen's own draft stays")
        XCTAssertEqual(dropped["note"], .string("The shared layout has no draft now. Screens with a draft of their own keep it: "
                                                + "\"Display 2\"; drop one with that screen's key."))
        _ = try call(MCPTools.discardHUDDraft, ["screen": .string("SECOND-UUID")])
        XCTAssertEqual(try call(MCPTools.discardHUDDraft)["note"], .string("There is no draft now. The panel shows the saved layout."))

        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "AWAY-UUID")
        layout.save(.screen("AWAY-UUID"))
        let away = try call(MCPTools.draftHUD, ["screen": .string("AWAY-UUID"), "order": .array([.string("Claude")])])
        XCTAssertEqual(away["note"], .string("Kept for the screen \"AWAY-UUID\", which is not attached now, as an unsaved draft "
                                             + "of its own. It shows in that display's panel if it returns before Agent HUD "
                                             + "restarts, which drops it; the user keeps it with Save in that panel."))
    }

    /// A screen's list may name only the agents that screen's resting HUD can show now; the shared layout's, those any
    /// attached screen can. What the layout already lists may come back with it.
    func testTheHUDsAgentsAreCheckedAgainstWhatTheScreenCanShow() throws {
        let cursor = try refused(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": hud(["Claude", "Cursor"])])
        XCTAssertEqual(cursor, "Nothing was changed. hud.agents[1]: \"Cursor\" is not an agent the resting HUD of "
                       + "\"Built-in display\" can show now. It can show: \"Claude\", \"Codex\", \"DeepSeek\".")
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hud": hud(["Cursor"])])
        _ = try call(MCPTools.draftHUD, ["hud": hud(["Cursor", "DeepSeek", "ChatGPT"])])
        XCTAssertTrue(try refused(MCPTools.draftHUD, ["hud": hud(["Pi"])]).contains("any attached screen's resting HUD"))

        layout.propose(.own(PanelLayout(hud: PanelLayout.HUD(agents: ["Kimi", "Claude"]))), for: "BUILT-IN-UUID")
        layout.save(.screen("BUILT-IN-UUID"))
        let kept = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": hud(["Claude", "Kimi"])])
        XCTAssertEqual(kept["unsaved"], .bool(true), "Kimi is away now, and the layout holds it")
        XCTAssertTrue(try refused(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": hud(["Hermes"])])
            .contains("hud.agents[0]"))
    }

    func testThePictureIsOfTheScreenAsked() throws {
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "SECOND-UUID")
        guard case .result(let result, let images) = screenTools.answer(MCPToolRequest(
            tool: MCPTools.getHUDScreenshot, arguments: ["screen": .string("SECOND-UUID")])) else { return XCTFail("refused") }
        XCTAssertEqual(asked, [.screen("SECOND-UUID")])
        XCTAssertEqual(images.count, 1)
        XCTAssertEqual(result["screen"], .string("SECOND-UUID"))
        XCTAssertEqual(result["unsaved"], .bool(true))
        XCTAssertTrue(result["note"]?.stringValue?.hasPrefix("The hover panel on \"Display 2\" as its own unsaved draft draws it") == true,
                      "\(result["note"] ?? .null)")
    }

    /// Screens are named by role: what an agent reads of them names no display by the name its owner gave it, and no
    /// account, which only the blocks name.
    func testWhatIsReadOfTheScreensNamesNoOneAndNoAccount() throws {
        let listing = try call(MCPTools.getHUD)["screens"] ?? .null
        let text = String(decoding: try RecordCoding.encoder().encode(listing), as: UTF8.self)
        for listed in screens(try call(MCPTools.getHUD)) {
            XCTAssertEqual(listed.keys.sorted(), ["agentPanelWidth", "agents", "edge", "hasNotch", "held", "hud", "key",
                                                  "layout", "look", "mode", "name", "shows", "styleUnsaved", "unsaved",
                                                  "width"])
        }
        let labels = (store.report?.accounts ?? [:]).values.flatMap { $0 }.flatMap { [$0.displayName, $0.account.id] }
        for label in labels { XCTAssertFalse(text.contains(label), label) }
        XCTAssertFalse(text.contains("@"))
    }
}
