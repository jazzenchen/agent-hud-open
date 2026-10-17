import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tools do with a screen's resting HUD style and the panel's width: a style proposed for one screen is a
/// draft drawn over its settings, read back in get_hud, held to the ranges Settings keep, and refused for every
/// screen at once; the settings are never written by a tool.
final class HUDToolsStyleTests: HUDToolsCase {
    private let builtIn = PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                      agents: ["Claude", "Codex", "DeepSeek"],
                                      otherModes: [.logos: ["Claude", "Codex", "Cursor"], .simple: ["Claude", "Codex", "Kimi"]],
                                      systemIsLight: false, reducesTransparency: false)
    private let second = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                                     agents: ["Claude", "Codex", "Cursor"],
                                     systemIsLight: false, reducesTransparency: false)

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] })

    private func call(_ tool: String, _ arguments: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
        guard case .result(let result, _) = screenTools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("\(tool) was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    private func refused(_ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        let before = (layout.revision, settings.settings)
        guard case .failure(let sentence) = screenTools.answer(MCPToolRequest(tool: MCPTools.draftHUD, arguments: arguments)) else {
            XCTFail("the proposal was taken", line: line)
            throw CancellationError()
        }
        XCTAssertEqual(layout.revision, before.0, "a refused proposal changes nothing", line: line)
        XCTAssertEqual(settings.settings, before.1, line: line)
        XCTAssertTrue(sentence.hasPrefix("Nothing was changed. "), sentence, line: line)
        return sentence
    }

    private func listing(_ key: String, in panel: [String: JSONValue]) throws -> [String: JSONValue] {
        try XCTUnwrap((panel["screens"]?.arrayValue ?? []).compactMap(\.objectValue).first { $0["key"] == .string(key) })
    }

    // MARK: Proposing a style

    /// A style proposed for one screen is that screen's draft, drawn over its settings, read back in its layout's hud
    /// and its listing, and the settings are not written.
    func testAStyleIsADraftOfThatScreenAlone() throws {
        let panel = try call(MCPTools.draftHUD, [
            "screen": .string("SECOND-UUID"),
            "hud": .object(["agents": .array([.string("Codex")]), "logoSize": .integer(30), "edge": .string("bottom"),
                            "glow": .object(["style": .string("dots")])]),
        ])
        XCTAssertEqual(layout.styleDraft(of: "SECOND-UUID"),
                       HUDStyle(edge: .bottom, logoSize: 30, glow: HUDStyle.Glow(style: .dots)))
        XCTAssertNil(layout.styleDraft(of: "BUILT-IN-UUID"))
        XCTAssertEqual(changes, 0, "a tool never writes the settings")
        XCTAssertTrue(settings.settings.screens.isEmpty)
        XCTAssertEqual(settings.effectivePlacement(on: "SECOND-UUID", hasNotch: false).logoSize, 30)

        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(panel["layout"]?["hud"], .object([
            "agents": .array([.string("Codex")]), "edge": .string("bottom"), "logoSize": .number(30),
            "glow": .object(["style": .string("dots")]),
        ]), "the layout read back holds the style, so it can be sent back as it is")
        let dock = try listing("SECOND-UUID", in: panel)
        XCTAssertEqual(dock["styleUnsaved"], .bool(true))
        XCTAssertEqual(dock["hud"]?["logoSize"], .number(30), "the style in effect")
        XCTAssertEqual(dock["hud"]?["mode"], .string("dynamic"))
        XCTAssertEqual(dock["hud"]?["glow"]["style"], .string("dots"))
        XCTAssertEqual(dock["hud"]?["glow"]["speed"], .number(GlowSettings().breathSeconds))
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["styleUnsaved"], .bool(false))
        XCTAssertTrue(panel["note"]?.stringValue?.contains("unsaved draft") == true, "\(panel["note"] ?? .null)")

        // The layout read back, sent back, leaves everything as it is.
        let revision = layout.revision
        let read = try call(MCPTools.getHUD, ["screen": .string("SECOND-UUID")])
        var again = read["layout"]?.objectValue ?? [:]
        again["screen"] = .string("SECOND-UUID")
        _ = try call(MCPTools.draftHUD, again)
        XCTAssertEqual(layout.revision, revision)
        XCTAssertEqual(layout.styleDraft(of: "SECOND-UUID"), HUDStyle(edge: .bottom, logoSize: 30, glow: HUDStyle.Glow(style: .dots)))
    }

    /// The style a screen lists, sent back whole, is no draft: a field that repeats what is saved changes nothing, so
    /// a field the mode does not draw is no issue either.
    func testTheStyleInEffectSentBackIsNoDraft() throws {
        let dock = try listing("BUILT-IN-UUID", in: try call(MCPTools.getHUD))
        var hud = try XCTUnwrap(dock["hud"]?.objectValue)
        _ = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": .object(hud)])
        XCTAssertNil(layout.styleDraft(of: "BUILT-IN-UUID"))
        XCTAssertFalse(layout.hasDraft(on: "BUILT-IN-UUID"))
        hud["glow"] = .object(["style": .string("braille")])
        _ = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": .object(hud)])
        XCTAssertEqual(layout.styleDraft(of: "BUILT-IN-UUID"), HUDStyle(glow: HUDStyle.Glow(style: .braille)))
    }

    /// A proposal is whole: one that leaves the style out drops a waiting style draft, as it would any field it leaves.
    func testAProposalWithoutTheStyleDropsTheDraft() throws {
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hud": .object(["logoSize": .integer(30)])])
        XCTAssertNotNil(layout.styleDraft(of: "SECOND-UUID"))
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hiddenSections": .array([.string("tokens")])])
        XCTAssertNil(layout.styleDraft(of: "SECOND-UUID"))
        XCTAssertNotNil(layout.draft(of: "SECOND-UUID"))
    }

    /// A screen with a notch that turns into a dock is placed clear of the hardware, as Settings places it, and the
    /// agents its list may name are the ones the new mode shows.
    func testANewModeIsPlacedAsSettingsPlacesItAndListsItsAgents() throws {
        let sentence = try refused(["screen": .string("BUILT-IN-UUID"), "hud": .object(["agents": .array([.string("Kimi")])])])
        XCTAssertTrue(sentence.contains("\"Kimi\" is not an agent the resting HUD of \"Built-in display\" can show now."), sentence)
        _ = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"),
                                                "hud": .object(["mode": .string("simple"), "agents": .array([.string("Kimi")])])])
        XCTAssertEqual(layout.styleDraft(of: "BUILT-IN-UUID"), HUDStyle(mode: .simple, position: 0))
        XCTAssertEqual(layout.effective(for: "BUILT-IN-UUID").hud.agents, ["Kimi"], "Kimi is on the Simple panel")
    }

    /// The style a screen with a notch lists, sent back with a dock's mode, is placed as Settings places it: the
    /// position it repeats is the saved centre, which would sit behind the hardware, so the dock starts at the top left.
    /// A position the agent chose is kept.
    func testTheListedStyleSentBackAsADockIsPlacedClearOfTheNotch() throws {
        var hud = try XCTUnwrap(try listing("BUILT-IN-UUID", in: try call(MCPTools.getHUD))["hud"]?.objectValue)
        XCTAssertEqual(hud["position"], .number(0.5))
        hud["mode"] = .string("dynamic")
        _ = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": .object(hud)])
        XCTAssertEqual(layout.styleDraft(of: "BUILT-IN-UUID"), HUDStyle(mode: .logos, position: 0))
        hud["position"] = .number(0.3)
        _ = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"), "hud": .object(hud)])
        XCTAssertEqual(layout.styleDraft(of: "BUILT-IN-UUID"), HUDStyle(mode: .logos, position: 0.3))
    }

    /// The note says when a dock leaving the notch was placed clear of it rather than where the proposal put it.
    func testADockPlacedClearOfTheNotchIsSaidSo() throws {
        let panel = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"),
                                                        "hud": .object(["mode": .string("dynamic"), "position": .number(0.5)])])
        XCTAssertEqual(layout.styleDraft(of: "BUILT-IN-UUID"), HUDStyle(mode: .logos, position: 0))
        XCTAssertTrue(panel["note"]?.stringValue?.contains("starts at position 0, the top left, clear of the notch") == true,
                      "\(panel["note"] ?? .null)")
        let chosen = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"),
                                                         "hud": .object(["mode": .string("dynamic"), "position": .number(0.3)])])
        XCTAssertFalse(chosen["note"]?.stringValue?.contains("clear of the notch") == true, "a position chosen is kept, unsaid")
    }

    /// Discarding the screen's draft drops its style with its layout draft.
    func testDiscardingDropsTheStyle() throws {
        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "hud": .object(["showLogos": .bool(false)])])
        let panel = try call(MCPTools.discardHUDDraft, ["screen": .string("SECOND-UUID")])
        XCTAssertNil(layout.styleDraft(of: "SECOND-UUID"))
        XCTAssertTrue(panel["note"]?.stringValue?.hasSuffix("Its resting HUD is back to the style it has saved.") == true,
                      "\(panel["note"] ?? .null)")
        XCTAssertEqual(changes, 0)
    }

    // MARK: Versions

    /// After a Save that changed only the style, from "previous" proposes the style kept with the version before, as a
    /// draft drawn whole; Save writes it back as it was kept, a placement the screen had none of included, and keeps the
    /// style it replaces, so Revert to Previous undoes it.
    func testFromPreviousBringsTheStyleBack() throws {
        let key = "BUILT-IN-UUID"
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object([
            "mode": .string("dynamic"), "edge": .string("left"), "logoSize": .integer(30), "glow": .object(["style": .string("dots")]),
        ])])
        layout.savePanel(on: key)
        let saved = settings.settings
        XCTAssertEqual(try listing(key, in: try call(MCPTools.getHUD))["hud"]?["mode"], .string("dynamic"))
        changes = 0

        let panel = try call(MCPTools.draftHUD, ["screen": .string(key), "from": .string("previous")])
        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(try listing(key, in: panel)["styleUnsaved"], .bool(true))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: true), .default(hasNotch: true), "drawn as it was")
        XCTAssertEqual(settings.effectiveGlow(on: key), settings.settings.glow)
        XCTAssertEqual(panel["layout"]?["hud"]["mode"], .string("notch"), "the fields it changes, in the layout read back")
        XCTAssertFalse(panel["note"]?.stringValue?.contains("already has saved") == true, "\(panel["note"] ?? .null)")
        XCTAssertEqual(settings.settings, saved, "a tool writes nothing")
        XCTAssertEqual(changes, 0)

        // What was read back, sent back, keeps the draft as it is.
        let revision = layout.revision
        var again = try XCTUnwrap(panel["layout"]?.objectValue)
        again["screen"] = .string(key)
        _ = try call(MCPTools.draftHUD, again)
        XCTAssertEqual(layout.revision, revision)

        layout.savePanel(on: key)
        XCTAssertEqual(changes, 1)
        XCTAssertNil(settings.settings.screens[key], "no placement of its own, as before")
        XCTAssertNil(settings.settings.screenGlow[key])
        layout.revert(.screen(key))
        XCTAssertEqual(settings.settings.screens[key], saved.screens[key], "the style it replaced is kept")
        XCTAssertEqual(settings.settings.screenGlow[key], saved.screenGlow[key])
    }

    /// After a layout and a style saved together, from "previous" proposes both, the shared layout and the style before;
    /// Save keeps what it replaces, so the person's own placement and glow are never lost.
    func testFromPreviousBringsTheLayoutAndTheStyleBackTogether() throws {
        let key = "SECOND-UUID"
        settings.update { $0.screens[key] = ScreenPlacement(mode: .logos, edge: .right, logoSize: 20) }
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hiddenSections": .array([.string("tokens")]),
                                                "hud": .object(["logoSize": .integer(30)])])
        layout.savePanel(on: key)
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 30)

        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "from": .string("previous")])
        XCTAssertTrue(layout.usesShared(key))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).logoSize, 20)
        layout.savePanel(on: key)
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 20)
        XCTAssertTrue(layout.usesShared(key))
        layout.revert(.screen(key))
        XCTAssertEqual(layout.ownLayout(of: key), PanelLayout(hiddenSections: [.tokens]))
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 30, "the version it replaced, style and all")
    }

    /// A screen that is away has no resting HUD to draw a version's style on: from "previous" proposes its layout, and
    /// the note says the style is not part of it.
    func testFromPreviousForAScreenThatIsAwaySaysTheStyleStays() throws {
        let key = "AWAY-UUID"
        settings.update { $0.screens[key] = ScreenPlacement(mode: .simple) }
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: key)
        layout.proposeStyle(HUDStyle(simpleSize: 30), for: key, hasNotch: false)
        layout.save(.screen(key))
        let panel = try call(MCPTools.draftHUD, ["screen": .string(key), "from": .string("previous")])
        XCTAssertEqual(layout.draft(of: key), .shared)
        XCTAssertNil(layout.styleDraft(of: key))
        XCTAssertTrue(panel["note"]?.stringValue?.hasSuffix("are not part of the draft while the screen is not attached.") == true,
                      "\(panel["note"] ?? .null)")
    }

    // MARK: Refusals

    /// A style is one screen's: for every screen at once it is refused with a sentence naming the attached screens'
    /// keys, while the agents for every screen are taken as before.
    func testAStyleForEveryScreenIsRefusedNamingTheScreens() throws {
        let sentence = try refused(["hud": .object(["mode": .string("simple"), "logoSize": .integer(20)])])
        XCTAssertEqual(sentence, "Nothing was changed. hud.mode and hud.logoSize set the resting HUD of one screen, so the "
                       + "proposal needs that screen's key; with screen \"all\" the hud takes agents alone. The screens "
                       + "attached now: \"BUILT-IN-UUID\" (Built-in display), \"SECOND-UUID\" (Display 2).")
        XCTAssertTrue(try refused(["screen": .string("all"), "hud": .object(["glow": .object(["style": .string("dots")])])])
            .contains("hud.glow sets the resting HUD of one screen"))
        _ = try call(MCPTools.draftHUD, ["hud": .object(["agents": .array([.string("Codex")])])])
        XCTAssertEqual(layout.draft?.hud.agents, ["Codex"])
        // A glow that sets nothing, as an empty object or with only nulls, is not given, as a null is not.
        for glow: JSONValue in [.object([:]), .object(["style": .null])] {
            _ = try call(MCPTools.draftHUD, ["screen": .string("all"), "hud": .object([
                "agents": .array([.string("Codex")]), "glow": glow])])
        }
    }

    /// The notch on a screen without one is refused with the reason, and so is every value out of the range Settings
    /// keep, with every reason at once.
    func testWhatTheScreenCannotDrawIsRefusedWithEveryReason() throws {
        let notch = try refused(["screen": .string("SECOND-UUID"), "hud": .object(["mode": .string("notch")])])
        XCTAssertEqual(notch, "Nothing was changed. hud.mode: \"Display 2\" has no notch, so its HUD cannot be \"notch\". "
                       + "It can be \"dynamic\" or \"simple\".")
        let ranges = try refused(["screen": .string("SECOND-UUID"), "hud": .object([
            "logoSize": .integer(70), "position": .number(1.5), "glow": .object(["brightness": .integer(2), "speed": .integer(30)]),
        ])])
        for part in ["hud.logoSize is a number from 20 to 60.", "hud.position is a number from 0 to 1.",
                     "hud.glow.brightness is a number from 0.2 to 1.", "hud.glow.speed is a number from 1 to 12."] {
            XCTAssertTrue(ranges.contains(part), "\(part) — \(ranges)")
        }
        let unseen = try refused(["screen": .string("BUILT-IN-UUID"), "hud": .object(["simpleSize": .integer(30)])])
        XCTAssertTrue(unseen.contains("hud.simpleSize: simpleSize sets a \"simple\" HUD, and the HUD of \"Built-in display\" "
                                      + "would be \"notch\"."), unseen)
        XCTAssertTrue(try refused(["screen": .string("SECOND-UUID"), "hud": .object(["mode": .string("logos")])])
            .contains("hud.mode is one of \"notch\", \"dynamic\", \"simple\"."))
    }

    /// The glow's idle period and a grid's pitch, density, core and fade are read back with the rest of the style and
    /// proposed as Settings › Glow shows them: a grid's on a grid glow, the style that makes it one included, the idle
    /// period on any glow, each held to the range and the step Settings uses, with every reason at once.
    func testTheGlowsIdlePeriodAndGridAreProposedAsSettingsShowsThem() throws {
        let key = "SECOND-UUID"
        let glow = try XCTUnwrap(try listing(key, in: try call(MCPTools.getHUD))["hud"]?["glow"].objectValue)
        let standard = GlowSettings()
        XCTAssertEqual(glow["idleSpeed"], .number(standard.idleBreathSeconds))
        XCTAssertEqual(glow["gridPitch"], .number(standard.gridPitch))
        XCTAssertEqual(glow["gridDensity"], .number(standard.gridDensity))
        XCTAssertEqual(glow["gridCore"], .number(standard.gridCore))
        XCTAssertEqual(glow["gridFade"], .number(standard.gridFade))
        XCTAssertEqual(glow["style"], .string("blur"), "listed beside a soft glow, whose grid is kept for a grid style")

        let soft = try refused(["screen": .string(key), "hud": .object(["glow": .object(["gridPitch": .integer(6)])])])
        XCTAssertEqual(soft, "Nothing was changed. hud.glow.gridPitch: gridPitch sets a grid glow style, one of \"dots\", "
                       + "\"ascii\", \"blocks\", \"braille\", \"binary\", and the glow of \"Display 2\" would be \"blur\".")
        let panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["glow": .object([
            "style": .string("dots"), "gridPitch": .integer(6), "gridDensity": .number(1.25), "gridCore": .integer(2),
            "gridFade": .integer(3), "idleSpeed": .integer(12),
        ])])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(glow: HUDStyle.Glow(style: .dots, idleSpeed: 12, gridPitch: 6,
                                                                                gridDensity: 1.25, gridCore: 2, gridFade: 3)))
        var drawn = standard
        drawn.style = .dots
        drawn.idleBreathSeconds = 12
        drawn.gridPitch = 6
        drawn.gridDensity = 1.25
        drawn.gridCore = 2
        drawn.gridFade = 3
        XCTAssertEqual(settings.effectiveGlow(on: key), drawn)
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["glow"]["gridCore"], .number(2))
        XCTAssertEqual(changes, 0, "a tool never writes the settings")

        let steps = try refused(["screen": .string(key), "hud": .object(["glow": .object([
            "style": .string("dots"), "gridPitch": .number(6.5), "gridDensity": .integer(2), "idleSpeed": .integer(30),
        ])])])
        for part in ["hud.glow.gridDensity is a number from 0.5 to 1.5 in steps of 0.05.",
                     "hud.glow.idleSpeed is a number from 1 to 24 in steps of 0.5.",
                     "hud.glow.gridPitch: gridPitch is a whole number from 4 to 12, as Settings sets it, which 6.5 is not."] {
            XCTAssertTrue(steps.contains(part), "\(part) — \(steps)")
        }

        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["glow": .object(["idleSpeed": .number(9.5)])])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(glow: HUDStyle.Glow(idleSpeed: 9.5)), "the idle period on a soft glow")
        let simple = try refused(["screen": .string(key), "hud": .object(["mode": .string("simple"),
                                                                           "glow": .object(["idleSpeed": .number(9.5)])])])
        XCTAssertTrue(simple.contains("hud.glow.idleSpeed: A \"simple\" HUD draws no glow"), simple)
    }

    /// A screen that is not attached has no resting HUD to style.
    func testAScreenThatIsAwayTakesNoStyle() throws {
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "AWAY-UUID")
        let sentence = try refused(["screen": .string("AWAY-UUID"), "hud": .object(["logoSize": .integer(20)])])
        XCTAssertEqual(sentence, "Nothing was changed. hud.logoSize sets the resting HUD of an attached screen, and the screen "
                       + "\"AWAY-UUID\", which is not attached now, has none to draw it on.")
    }

    // MARK: The panel's width

    /// The width is a layout's like its spacing: proposed for the shared layout or a screen's own, read back in the
    /// style and in each screen's listing, and held to 480 to 720.
    func testTheWidthIsProposedAndReadBack() throws {
        let panel = try call(MCPTools.draftHUD, ["style": .object(["width": .integer(720)])])
        XCTAssertEqual(layout.draft?.style.panelWidth, 720)
        XCTAssertEqual(panel["style"]?["width"], .integer(720))
        XCTAssertEqual(panel["layout"], .object(["style": .object(["width": .integer(720)])]))
        XCTAssertEqual(try listing("SECOND-UUID", in: panel)["width"], .integer(720), "a screen on the shared layout")

        _ = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"), "style": .object(["width": .integer(480)])])
        let read = try call(MCPTools.getHUD)
        XCTAssertEqual(try listing("SECOND-UUID", in: read)["width"], .integer(480), "its own")
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: read)["width"], .integer(720))

        _ = try call(MCPTools.draftHUD, ["style": .object(["width": .integer(540)])])
        XCTAssertNil(layout.draft, "540 is the saved default: no draft")
        XCTAssertTrue(try refused(["style": .object(["width": .integer(800)])]).contains("style.width is a whole number from 480 to 720."))
    }
}
