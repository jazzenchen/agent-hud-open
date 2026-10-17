import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// One agent's own panel through `get_hud_screenshot`: the card a click on its mark opens beside it on a dock that
/// separates its agents, named as `get_hud` names the agent, drawn by the host with any draft, and refused, with how
/// to get one, for a screen whose HUD gives no agent a panel of its own or an agent it shows no mark for. The drawing
/// is the host's, so here it is a fake that answers with what a test gives it.
final class HUDToolsAgentPictureTests: HUDToolsCase {
    private let png = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x03, 0x04])
    private var reducesTransparency = false
    /// The screens and agents the host was asked to draw an agent's own panel for, in order.
    private var asked: [String] = []
    /// What the host draws, which a test changes.
    private var drawn: PanelPicture? = PanelPicture(png: Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x03, 0x04]),
                                                    width: 440, fit: PanelFit(height: 512, maximumHeight: 1_000), scale: 2)

    private var builtIn: PanelScreen {
        PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                    agents: ["Claude", "Codex", "DeepSeek"],
                    otherModes: [.logos: ["Claude", "Codex", "Cursor"], .simple: ["Claude", "Codex", "DeepSeek"]],
                    systemIsLight: false, reducesTransparency: reducesTransparency)
    }

    /// A second display, in the mode its settings and any style draft give it, as the host lists it.
    private var second: PanelScreen {
        let mode = settings.effectivePlacement(on: "SECOND-UUID", hasNotch: false).mode
        let agents: [HUDMode: [String]] = [.notch: ["Claude", "Codex", "DeepSeek"], .logos: ["Claude", "Codex", "Cursor"],
                                           .simple: ["Claude", "Codex", "DeepSeek"]]
        return PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: mode, edge: .right, hasNotch: false,
                           agents: agents[mode] ?? [], otherModes: agents, systemIsLight: false,
                           reducesTransparency: reducesTransparency)
    }

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] },
                                              picture: { [unowned self] _ in
        asked.append("the whole panel")
        return drawn
    }, agentPicture: { [unowned self] key, agent in
        asked.append("\(key) \(agent)")
        return drawn
    })

    /// The second display's saved placement: a Dynamic dock on its right edge, which separates its agents unless a test
    /// says otherwise.
    private func dock(_ mode: HUDMode = .logos, separates: Bool = true) {
        settings.update { $0.screens["SECOND-UUID"] = ScreenPlacement(mode: mode, edge: .right, separatesAgents: separates) }
        changes = 0
    }

    private func picture(_ arguments: [String: JSONValue], line: UInt = #line) throws -> (result: [String: JSONValue], images: [MCPImage]) {
        guard case .result(let result, let images) = screenTools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot,
                                                                                        arguments: arguments)) else {
            XCTFail("get_hud_screenshot was refused", line: line)
            throw CancellationError()
        }
        return (result, images)
    }

    private func refused(_ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        let before = (layout.revision, layout.draft, settings.settings)
        guard case .failure(let sentence) = screenTools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: arguments)) else {
            XCTFail("the call was taken", line: line)
            throw CancellationError()
        }
        XCTAssertEqual(layout.revision, before.0, "a refused call changes nothing", line: line)
        XCTAssertEqual(layout.draft, before.1, line: line)
        XCTAssertEqual(settings.settings, before.2, line: line)
        return sentence
    }

    private func agent(_ name: String, on key: String = "SECOND-UUID") -> [String: JSONValue] {
        ["screen": .string(key), "agent": .string(name)]
    }

    // MARK: The picture

    func testAnAgentsOwnPanelComesWithWhatItShows() throws {
        dock()
        let (result, images) = try picture(agent("Claude"))
        XCTAssertEqual(images, [MCPImage(mimeType: "image/png", data: png)])
        XCTAssertEqual(asked, ["SECOND-UUID Claude"], "the host draws that agent's own panel on that screen, and nothing else")
        XCTAssertEqual(result.keys.sorted(), ["agent", "height", "note", "pixelsPerPoint", "revision", "screen", "scrolls",
                                              "unsaved", "width"],
                       "what the whole panel's picture says, and the agent")
        XCTAssertEqual(result["screen"], .string("SECOND-UUID"))
        XCTAssertEqual(result["agent"], .string("Claude"))
        XCTAssertEqual(result["revision"], .integer(0))
        XCTAssertEqual(result["unsaved"], .bool(false))
        XCTAssertEqual(result["width"], .integer(440))
        XCTAssertEqual(result["height"], .integer(512))
        XCTAssertEqual(result["scrolls"], .bool(false))
        XCTAssertEqual(result["pixelsPerPoint"], .number(2))
        XCTAssertEqual(result["note"], .string("\"Claude\"'s own panel on \"Display 2\", the card a click on its mark opens "
                                               + "beside it, as the saved shared layout draws it, 440 by 512 points, with "
                                               + "account names masked, session titles left out and any reading's reason or "
                                               + "client's notice drawn as grey bars."))
        XCTAssertEqual(changes, 0, "a picture writes nothing")
        XCTAssertEqual(layout.revision, 0)
        XCTAssertNil(layout.holds.hold(on: "SECOND-UUID"), "and holds no panel open")

        // Left out, or given as null, the agent draws the whole panel as before.
        asked = []
        let whole = try picture(["screen": .string("SECOND-UUID"), "agent": .null]).result
        XCTAssertNil(whole["agent"])
        XCTAssertTrue(whole["note"]?.stringValue?.hasPrefix("The hover panel on \"Display 2\"") == true, "\(whole["note"] ?? .null)")
        XCTAssertEqual(asked, ["the whole panel"])
    }

    /// The agent is named as get_hud names it: by the screen's shows, matched by the mark it is drawn with, so a name
    /// that shares a mark names it too, or by its block's key, an API balance's naming its vendor; the answer names it as
    /// the screen lists it.
    func testAnAgentIsNamedAsGetHUDNamesIt() throws {
        dock(.simple)
        guard case .result(let panel, _) = screenTools.answer(MCPToolRequest(tool: MCPTools.getHUD)) else { return XCTFail() }
        let listed = try XCTUnwrap(panel["screens"]?.arrayValue?.last?.objectValue)
        XCTAssertEqual(listed["shows"], .array(["Claude", "Codex", "DeepSeek"].map(JSONValue.string)))
        XCTAssertEqual(blocks(panel).map { $0["key"] }, [.string("Claude"), .string("Codex"), .string("api:DeepSeek")])

        XCTAssertEqual(try picture(agent("Codex")).result["agent"], .string("Codex"))
        XCTAssertEqual(try picture(agent("ChatGPT")).result["agent"], .string("Codex"), "ChatGPT shares Codex's mark")
        let balance = try picture(agent("api:DeepSeek")).result
        XCTAssertEqual(balance["agent"], .string("DeepSeek"), "an API balance's block names its vendor")
        XCTAssertTrue(balance["note"]?.stringValue?.hasPrefix("\"DeepSeek\"'s own panel") == true, "\(balance["note"] ?? .null)")
        XCTAssertEqual(asked, ["SECOND-UUID Codex", "SECOND-UUID Codex", "SECOND-UUID DeepSeek"])
    }

    /// A draft is drawn as the agent's own panel draws it while it waits: in the layout in effect, with the bar that says
    /// it is unsaved, and a dock that separates its agents only in a style draft has them drawn so, which the note says.
    func testAnAgentsOwnPanelIsDrawnWithAWaitingDraft() throws {
        dock(separates: false)
        _ = try result(MCPTools.getHUD)
        guard case .result = screenTools.answer(MCPToolRequest(tool: MCPTools.draftHUD, arguments: [
            "screen": .string("SECOND-UUID"), "hud": .object(["separateAgents": .bool(true)]),
        ])) else { return XCTFail("the style was refused") }
        let styled = try picture(agent("Claude")).result
        XCTAssertEqual(styled["unsaved"], .bool(true))
        XCTAssertEqual(styled["note"], .string("\"Claude\"'s own panel on \"Display 2\", the card a click on its mark opens "
                                               + "beside it, as the saved shared layout draws it, with the bar above its "
                                               + "footer that says its resting HUD's style is unsaved, 440 by 512 points, with "
                                               + "account names masked, session titles left out and any reading's reason or "
                                               + "client's notice drawn as grey bars. The dock separates its agents only in "
                                               + "the unsaved style draft."))

        guard case .result = screenTools.answer(MCPToolRequest(tool: MCPTools.draftHUD, arguments: [
            "screen": .string("SECOND-UUID"), "order": .array([.string("Codex")]), "hud": .object(["separateAgents": .bool(true)]),
        ])) else { return XCTFail("the layout was refused") }
        let both = try XCTUnwrap(try picture(agent("Codex")).result["note"]?.stringValue)
        XCTAssertTrue(both.contains("as its own unsaved draft draws it, with the bar above its footer for the unsaved layout "
                                    + "and its resting HUD's unsaved style"), both)

        // Saved, the dock separates its agents, and the note no longer says it does only in a draft.
        layout.save(.screen("SECOND-UUID"))
        let saved = try XCTUnwrap(try picture(agent("Codex")).result["note"]?.stringValue)
        XCTAssertTrue(saved.contains("as its own saved layout draws it, 440 by 512"), saved)
        XCTAssertFalse(saved.contains("only in"), saved)
    }

    /// What the whole panel's picture says of its size and look, an agent's own says too: that it scrolls on its display,
    /// that it is drawn smaller to stay within the pixel limit, and that glass is a stand-in.
    func testAnAgentsOwnPanelSaysItScrollsAndThatGlassIsAStandIn() throws {
        settings.update {
            $0.screens["SECOND-UUID"] = ScreenPlacement(mode: .logos, edge: .right, separatesAgents: true, material: .glass)
        }
        drawn = PanelPicture(png: png, width: 600, fit: PanelFit(height: 1_500, maximumHeight: 1_360), scale: 1.6)
        let tall = try picture(agent("Claude")).result
        XCTAssertEqual(tall["width"], .integer(600))
        XCTAssertEqual(tall["scrolls"], .bool(true))
        XCTAssertEqual(tall["pixelsPerPoint"], .number(1.6))
        let note = try XCTUnwrap(tall["note"]?.stringValue)
        XCTAssertTrue(note.hasSuffix("600 by 1500 points, with account names masked, session titles left out and any "
                                     + "reading's reason or client's notice drawn as grey bars. That display opens it at most "
                                     + "1360 points tall, so a person scrolls to see the rest; the picture shows all of it. It "
                                     + "is drawn at 1.6 pixels a point instead of 2 to stay within 2400 pixels tall. "
                                     + HUDTools.glassStandIn), note)

        reducesTransparency = true
        XCTAssertFalse(try picture(agent("Claude")).result["note"]?.stringValue?.contains(HUDTools.glassStandIn) == true,
                       "Reduce Transparency draws it solid")
    }

    // MARK: Refusals

    /// A screen whose HUD gives no agent a panel of its own refuses the agent, and says how one would: a Dynamic dock or a
    /// Simple panel with separateAgents on.
    func testAScreenWhoseHUDDoesNotSeparateItsAgentsSaysHowOneWould() throws {
        XCTAssertEqual(try refused(agent("Claude", on: "BUILT-IN-UUID")),
                       "The resting HUD of \"Built-in display\" is a notch HUD, whose panel is every agent's, so no agent has "
                       + "one of its own there. A Dynamic dock or a Simple panel that separates its agents gives each its "
                       + "own: propose hud.mode \"dynamic\" or \"simple\" with hud.separateAgents true for that screen, then "
                       + "draw it.")
        dock(separates: false)
        XCTAssertEqual(try refused(agent("Claude")),
                       "The resting HUD of \"Display 2\", a \"dynamic\" HUD, does not separate its agents, so its panel is "
                       + "every agent's and no agent has one of its own there. Propose hud.separateAgents true for that "
                       + "screen, then draw it.")
        dock(.simple, separates: false)
        XCTAssertTrue(try refused(agent("Claude")).hasPrefix("The resting HUD of \"Display 2\", a \"simple\" HUD, does not"))
        // A notch HUD that separates its agents, as a placement can keep for when it docks, still has one panel.
        dock(.notch, separates: true)
        XCTAssertTrue(try refused(agent("Claude")).hasPrefix("The resting HUD of \"Display 2\" is a notch HUD"))
        XCTAssertEqual(asked, [], "nothing is drawn for a refused call")
    }

    /// An agent the screen's resting HUD shows no mark for has no panel of its own there: one it cannot show now, and
    /// one its layout's list leaves out. The refusal lists those it shows.
    func testAnAgentTheScreenShowsNoMarkForIsRefused() throws {
        dock()
        XCTAssertEqual(try refused(agent("Gemini")),
                       "\"Gemini\" has no mark on the resting HUD of \"Display 2\" now, so it has no panel of its own there. "
                       + "It shows: \"Claude\", \"Codex\", \"Cursor\".")
        XCTAssertTrue(try refused(agent("api:DeepSeek")).hasPrefix("\"api:DeepSeek\" has no mark"),
                      "a balance whose vendor the dock does not show")
        layout.propose(.own(PanelLayout(hud: PanelLayout.HUD(agents: ["Cursor", "Claude"]))), for: "SECOND-UUID")
        XCTAssertEqual(try refused(agent("Codex")),
                       "\"Codex\" has no mark on the resting HUD of \"Display 2\" now, so it has no panel of its own there. "
                       + "It shows: \"Cursor\", \"Claude\".", "the draft's list is the one drawn")
        XCTAssertEqual(try refused(agent("claude")).prefix(9), "\"claude\" ", "a name is matched as get_hud writes it")
        XCTAssertEqual(asked, [])
    }

    /// The agent needs an attached screen, a name, and the panel it narrows: the shared layout, a screen that is away, a
    /// name that is not one, and the resting HUD are refused, each with why.
    func testAnAgentNeedsAnAttachedScreenAndThePanel() throws {
        dock()
        XCTAssertEqual(try refused(["agent": .string("Claude")]),
                       "agent draws one agent's own panel on a screen whose dock separates its agents, so it needs that "
                       + "screen's key. The screens attached now: \"BUILT-IN-UUID\" (Built-in display), \"SECOND-UUID\" "
                       + "(Display 2).")
        XCTAssertTrue(try refused(["screen": .string("all"), "agent": .string("Claude")]).hasPrefix("agent draws one agent's"))
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "AWAY-UUID")
        XCTAssertTrue(try refused(agent("Claude", on: "AWAY-UUID"))
            .hasPrefix("The screen \"AWAY-UUID\", which is not attached now, has no agent's own panel to draw. The screens"))
        for name: JSONValue in [.string(""), .integer(1), .array([.string("Claude")])] {
            XCTAssertEqual(try refused(["screen": .string("SECOND-UUID"), "agent": name]),
                           "agent is an agent's name from that screen's shows in get_hud, or its block's key.")
        }
        XCTAssertEqual(try refused(["screen": .string("SECOND-UUID"), "agent": .string("Claude"), "show": .string("hud")]),
                       "show \"hud\" draws a screen's resting HUD, and agent one agent's own panel, so they are not given "
                       + "together: leave out show, or agent.")
        XCTAssertEqual(asked, [])
        guard case .result = screenTools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: [
            "screen": .string("SECOND-UUID"), "agent": .string("Claude"), "show": .string("panel"),
        ])) else { return XCTFail("an agent's own panel is a panel") }
    }

    func testWithoutARendererAnAgentsOwnPanelIsRefused() throws {
        dock()
        let tools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] })
        XCTAssertEqual(tools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: agent("Claude"))),
                       .failure("This copy of Agent HUD cannot draw an agent's own panel, so there is no picture of it here. "
                                + "get_hud reads the panel as data."))
        drawn = nil
        XCTAssertEqual(try refused(agent("Claude")),
                       "The agent's own panel could not be drawn. Try again, or read the panel with get_hud.")
        XCTAssertEqual(asked, ["SECOND-UUID Claude"])
    }
}
