import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// The defaults an agent proposes for one screen, apart as a person resets them: from "default" proposes the default
/// layout alone, and hud.reset the screen's default style, a draft drawn over its settings that a person's Save keeps.
/// No tool writes the settings.
final class HUDToolsResetTests: HUDToolsCase {
    private let builtIn = PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                      agents: ["Claude", "Codex", "DeepSeek"],
                                      otherModes: [.logos: ["Claude", "Codex", "Cursor"], .simple: ["Claude", "Codex", "Kimi"]],
                                      systemIsLight: false, reducesTransparency: false)
    private let second = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .simple, edge: .left, hasNotch: false,
                                     agents: ["Claude", "Codex", "Kimi"],
                                     otherModes: [.logos: ["Claude", "Codex", "Cursor"]],
                                     systemIsLight: false, reducesTransparency: false)

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] })

    private func call(_ arguments: [String: JSONValue], tool: String = MCPTools.draftHUD,
                      line: UInt = #line) throws -> [String: JSONValue] {
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

    /// The second display keeps a Simple panel of 30 pt on the left edge and a glow of half brightness.
    private func keepOwnStyle() {
        settings.update {
            $0.screens["SECOND-UUID"] = ScreenPlacement(mode: .simple, edge: .left, simpleSize: 30)
            var glow = GlowSettings()
            glow.brightness = 0.5
            $0.screenGlow["SECOND-UUID"] = glow
        }
        changes = 0
    }

    // MARK: From default

    /// From "default" proposes the default layout and leaves the style as saved, as the layout's Reset to Default
    /// does; a proposal is whole, so a waiting style draft is dropped. From "shared" does the same, as Use Shared
    /// Layout does.
    func testFromDefaultProposesTheLayoutAlone() throws {
        let key = "SECOND-UUID"
        keepOwnStyle()
        let saved = settings.settings
        layout.propose(.own(PanelLayout(hiddenSections: [.tokens])), for: key)
        layout.save(.screen(key))
        _ = try call(["screen": .string(key), "hud": .object(["simpleSize": .integer(24)])])
        XCTAssertNotNil(layout.styleDraft(of: key))

        let panel = try call(["screen": .string(key), "from": .string("default")])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout()))
        XCTAssertNil(layout.styleDraft(of: key), "the style as saved")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), saved.screens[key])
        XCTAssertEqual(settings.effectiveGlow(on: key), saved.screenGlow[key])
        XCTAssertEqual(try listing(key, in: panel)["styleUnsaved"], .bool(false))
        XCTAssertFalse(panel["note"]?.stringValue?.contains("not attached") == true, "\(panel["note"] ?? .null)")
        layout.savePanel(on: key)
        XCTAssertEqual(layout.ownLayout(of: key), PanelLayout())
        XCTAssertEqual(settings.settings, saved, "Save keeps the layout, the style as it was")
        XCTAssertEqual(changes, 0)

        _ = try call(["screen": .string(key), "hud": .object(["simpleSize": .integer(24)])])
        _ = try call(["screen": .string(key), "from": .string("shared")])
        XCTAssertNil(layout.styleDraft(of: key))
        XCTAssertEqual(layout.draft(of: key), .shared)
    }

    // MARK: hud.reset

    /// hud.reset proposes the screen's default style as its style draft, drawn as the defaults at once and listed by
    /// the fields that differ, which can be sent back as they are; Save writes it as the person's reset does, keeping
    /// the style before with the previous version, so Revert to Previous brings it back.
    func testHUDResetProposesTheDefaultStyle() throws {
        let key = "SECOND-UUID"
        keepOwnStyle()
        let saved = settings.settings
        let panel = try call(["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertNotNil(layout.styleDraft(of: key))
        XCTAssertNil(layout.draft(of: key), "a call with the style alone leaves the layout as it is")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false))
        XCTAssertEqual(settings.effectiveGlow(on: key), settings.settings.glow)
        XCTAssertEqual(settings.settings, saved, "a tool writes nothing")
        XCTAssertEqual(changes, 0)
        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(try listing(key, in: panel)["styleUnsaved"], .bool(true))
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["mode"], .string("dynamic"))
        XCTAssertEqual(panel["layout"]?["hud"]["mode"], .string("dynamic"), "the fields it changes, in the layout read back")
        XCTAssertEqual(panel["layout"]?["hud"]["reset"], .null, "reset is asked, never read back")

        // What was read back, sent back, keeps the draft as it is, and so does reset sent again.
        let revision = layout.revision
        var again = try XCTUnwrap(panel["layout"]?.objectValue)
        again["screen"] = .string(key)
        _ = try call(again)
        _ = try call(["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertEqual(layout.revision, revision)

        layout.savePanel(on: key)
        XCTAssertNil(settings.settings.screens[key])
        XCTAssertNil(settings.settings.screenGlow[key])
        XCTAssertEqual(changes, 1)
        layout.revert(.screen(key))
        XCTAssertEqual(settings.settings, saved, "Revert brings the style back")
    }

    /// hud.reset brings back every field an agent reads, the glass style, the idle period and the grid among them: a
    /// screen that differs from the defaults only there gets a draft listing them, which Save writes as the reset does.
    func testHUDResetRestoresTheGlassStyleAndTheGlowsGrid() throws {
        let key = "SECOND-UUID"
        settings.update {
            $0.screens[key] = ScreenPlacement(glassStyle: .regular)
            var glow = GlowSettings()
            glow.idleBreathSeconds = 12
            glow.gridPitch = 6
            glow.gridDensity = 1.25
            $0.screenGlow[key] = glow
        }
        changes = 0
        let panel = try call(["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertNotNil(layout.styleDraft(of: key))
        XCTAssertEqual(panel["layout"]?["hud"], .object([
            "glassStyle": .string("clear"),
            "glow": .object(["idleSpeed": .number(7), "gridPitch": .number(10), "gridDensity": .number(1)]),
        ]), "listed by the fields that differ")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false))
        XCTAssertEqual(settings.effectiveGlow(on: key), settings.settings.glow)
        layout.savePanel(on: key)
        XCTAssertNil(settings.settings.screens[key])
        XCTAssertNil(settings.settings.screenGlow[key])
        XCTAssertEqual(changes, 1)
    }

    /// hud.reset goes with a layout as any style does: the layout is the screen's draft and the default style its
    /// style draft, and the agents its list may name are those the default mode shows.
    func testHUDResetGoesWithALayout() throws {
        settings.update { $0.screens["BUILT-IN-UUID"] = ScreenPlacement(mode: .simple, edge: .bottom) }
        let kimi = try refused(["screen": .string("BUILT-IN-UUID"),
                                "hud": .object(["agents": .array([.string("Kimi")]), "reset": .bool(true)])])
        XCTAssertTrue(kimi.contains("\"Kimi\" is not an agent the resting HUD of \"Built-in display\" can show now."), kimi)
        _ = try call(["screen": .string("BUILT-IN-UUID"), "hiddenSections": .array([.string("tokens")]),
                      "hud": .object(["agents": .array([.string("DeepSeek")]), "reset": .bool(true)])])
        XCTAssertEqual(layout.draft(of: "BUILT-IN-UUID"),
                       .own(PanelLayout(hiddenSections: [.tokens], hud: PanelLayout.HUD(agents: ["DeepSeek"]))))
        XCTAssertEqual(settings.effectivePlacement(on: "BUILT-IN-UUID", hasNotch: true), .default(hasNotch: true))
    }

    /// hud.reset of a screen that differs from the defaults only in what no style field names, here the transparency it
    /// keeps for glass while its HUD is solid, is a draft as Reset to Default in Settings would change it: drawn as the
    /// defaults, listing no field, with a note that says it brings back what no hud field sets. get_hud lists the
    /// transparency it brings back; the draft read back, or the style listed, sent back keeps it, and Save writes it as
    /// that reset does.
    func testHUDResetBringsBackWhatNoStyleFieldNames() throws {
        let key = "SECOND-UUID"
        settings.update { $0.screens[key] = ScreenPlacement(glassTransparency: 0.9) }
        changes = 0
        let panel = try call(["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle())
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false))
        XCTAssertEqual(panel["unsaved"], .bool(true))
        XCTAssertEqual(try listing(key, in: panel)["styleUnsaved"], .bool(true))
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["glassTransparency"],
                       .number(ScreenPlacement.standardGlassTransparency))
        let note = try XCTUnwrap(panel["note"]?.stringValue)
        XCTAssertTrue(note.contains("shows its style as an unsaved draft"), note)
        XCTAssertTrue(note.contains(" Beside the fields its hud lists, the draft brings back what no hud field sets, such "
                                    + "as the transparency kept for the material the HUD does not draw, and Save writes "
                                    + "that too."), note)

        let revision = layout.revision
        var again = try XCTUnwrap(panel["layout"]?.objectValue)
        again["screen"] = .string(key)
        _ = try call(again)
        _ = try call(["screen": .string(key), "hud": try XCTUnwrap(try listing(key, in: panel)["hud"])])
        XCTAssertEqual(layout.revision, revision, "what was read back, and the style listed, sent back, keep the draft")
        layout.savePanel(on: key)
        XCTAssertNil(settings.settings.screens[key])
        XCTAssertNil(settings.settings.screenGlow[key])
        XCTAssertEqual(changes, 1)
    }

    /// A screen that already draws the defaults gets no style draft, and reset false asks nothing.
    func testHUDResetOfTheDefaultsLeavesNoDraft() throws {
        let panel = try call(["screen": .string("SECOND-UUID"), "hud": .object(["reset": .bool(true)])])
        XCTAssertNil(layout.styleDraft(of: "SECOND-UUID"))
        XCTAssertEqual(panel["unsaved"], .bool(false))
        XCTAssertTrue(panel["note"]?.stringValue?.contains("already has saved") == true, "\(panel["note"] ?? .null)")
        _ = try call(["screen": .string("SECOND-UUID"), "hud": .object(["reset": .bool(false), "logoSize": .integer(30)])])
        XCTAssertEqual(layout.styleDraft(of: "SECOND-UUID"), HUDStyle(logoSize: 30))
    }

    /// hud.reset is one screen's, for an attached screen, alone among the style fields, and true or false.
    func testHUDResetIsRefusedWhereItCannotBeDone() throws {
        XCTAssertEqual(try refused(["hud": .object(["reset": .bool(true)])]),
                       "Nothing was changed. hud.reset sets the resting HUD of one screen, so the proposal needs that "
                       + "screen's key; with screen \"all\" the hud takes agents alone. The screens attached now: "
                       + "\"BUILT-IN-UUID\" (Built-in display), \"SECOND-UUID\" (Display 2).")
        XCTAssertEqual(try refused(["screen": .string("SECOND-UUID"), "hud": .object([
            "reset": .bool(true), "simpleSize": .integer(30), "glow": .object(["brightness": .number(0.5)]),
        ])]), "Nothing was changed. hud.reset proposes the default style of the resting HUD of \"Display 2\" whole, so the "
            + "hud takes no other style field with it: leave out hud.simpleSize and hud.glow, or leave out hud.reset.")
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "AWAY-UUID")
        XCTAssertEqual(try refused(["screen": .string("AWAY-UUID"), "hud": .object(["reset": .bool(true)])]),
                       "Nothing was changed. hud.reset sets the resting HUD of an attached screen, and the screen "
                       + "\"AWAY-UUID\", which is not attached now, has none to draw it on.")
        XCTAssertEqual(try refused(["screen": .string("SECOND-UUID"), "hud": .object(["reset": .string("yes")])]),
                       "Nothing was changed. hud.reset is true or false.")
        XCTAssertTrue(try refused(["screen": .string("SECOND-UUID"), "from": .string("default"),
                                   "hud": .object(["reset": .bool(true)])]).contains("either from or a layout"))
        XCTAssertEqual(changes, 0)

        // A style field given as null, or a glow with no property, is not given, so reset goes with it.
        keepOwnStyle()
        _ = try call(["screen": .string("SECOND-UUID"), "hud": .object(["reset": .bool(true), "mode": .null, "glow": .object([:])])])
        XCTAssertEqual(settings.effectivePlacement(on: "SECOND-UUID", hasNotch: false), .default(hasNotch: false))
        XCTAssertEqual(changes, 0)
    }
}
