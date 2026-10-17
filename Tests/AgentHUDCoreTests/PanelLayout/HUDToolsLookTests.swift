import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// A screen's look through the tools: its appearance, material, glass style and glass's transparency are style fields
/// like any other, a draft drawn over the screen's settings that Save keeps, Discard drops, Revert to Previous undoes
/// and the reset gives back as dark, solid and clear; a notch HUD at a notch takes them too, its notch's row kept dark;
/// a transparency is the material in effect's, 0 to 1 for either, and get_hud lists what each material keeps; and
/// what get_hud and the pictures say of the look drawn now, its glass style and its transparency as it is drawn.
final class HUDToolsLookTests: HUDToolsCase {
    /// How the Mac is set, which the host tells with each screen.
    private var systemIsLight = false
    private var reducesTransparency = false

    private var builtIn: PanelScreen {
        PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                    agents: ["Claude", "Codex", "DeepSeek"],
                    otherModes: [.logos: ["Claude", "Codex", "Cursor"], .simple: ["Claude", "Codex", "Kimi"]],
                    systemIsLight: systemIsLight, reducesTransparency: reducesTransparency)
    }

    private var second: PanelScreen {
        PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                    agents: ["Claude", "Codex", "Cursor"], systemIsLight: systemIsLight,
                    reducesTransparency: reducesTransparency)
    }

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] },
                                              picture: { _ in
        PanelPicture(png: Data([0x89]), width: 540, fit: PanelFit(height: 600, maximumHeight: 1_000), scale: 2)
    }, hud: { _ in HUDPicture(png: Data([0x89]), width: 120, height: 40, scale: 2, drawsGlow: true) })

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

    /// The look get_hud lists: glass with the style it is made of, clear unless another is given, and solid with none.
    private func look(_ appearance: String, _ material: String, transparency: Double = 0, glassStyle: String = "clear",
                      note: String? = nil) -> JSONValue {
        var look: [String: JSONValue] = ["appearance": .string(appearance), "material": .string(material),
                                         "transparency": .number(transparency)]
        if material == "glass" { look["glassStyle"] = .string(glassStyle) }
        if let note { look["note"] = .string(note) }
        return .object(look)
    }

    /// What a solid HUD or clear glass draws of the transparency `chosen`, which get_hud's look gives: three fifths of
    /// it (`ScreenPlacement.transparencyScale(material:glassStyle:)`). Glass here is clear unless a test chooses another.
    private func drawn(_ chosen: Double) -> Double { chosen * 0.6 }

    // MARK: A draft like any other

    /// The look proposed for one screen is that screen's style draft, drawn over its settings and read back, and Save
    /// writes it into them as one version that Revert to Previous takes back; a tool writes nothing.
    func testTheLookIsADraftThatSaveKeepsAndRevertTakesBack() throws {
        let key = "SECOND-UUID"
        let panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                       "hud": .object(["appearance": .string("light"), "material": .string("glass")])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(appearance: .light, material: .glass))
        XCTAssertNil(layout.draft(of: key), "the look alone leaves the layout as it is")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false),
                       ScreenPlacement(appearance: .light, material: .glass))
        XCTAssertTrue(settings.settings.screens.isEmpty)
        XCTAssertEqual(changes, 0, "a tool never writes the settings")
        XCTAssertEqual(panel["layout"]?["hud"], .object(["appearance": .string("light"), "material": .string("glass")]))
        let listed = try listing(key, in: panel)
        XCTAssertEqual(listed["hud"]?["appearance"], .string("light"))
        XCTAssertEqual(listed["hud"]?["material"], .string("glass"))
        XCTAssertEqual(listed["look"], look("light", "glass", transparency: drawn(ScreenPlacement.standardGlassTransparency)))
        XCTAssertEqual(listed["hud"]?["transparency"], .number(ScreenPlacement.standardGlassTransparency), "as chosen")
        XCTAssertEqual(listed["styleUnsaved"], .bool(true))

        // What was read back, sent back, keeps the draft as it is.
        let revision = layout.revision
        var again = try XCTUnwrap(panel["layout"]?.objectValue)
        again["screen"] = .string(key)
        _ = try call(MCPTools.draftHUD, again)
        XCTAssertEqual(layout.revision, revision)

        layout.savePanel(on: key)
        XCTAssertNil(layout.styleDraft(of: key))
        XCTAssertEqual(settings.settings.screens[key], ScreenPlacement(appearance: .light, material: .glass))
        XCTAssertEqual(changes, 1)
        layout.revert(.screen(key))
        XCTAssertNil(settings.settings.screens[key], "Revert gives back the dark, solid default")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false))
    }

    /// Discard drops the look with the rest of the draft, and a proposal that leaves it out drops it too.
    func testDiscardDropsTheLook() throws {
        let key = "SECOND-UUID"
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["appearance": .string("system")])])
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).appearance, .system)
        _ = try call(MCPTools.discardHUDDraft, ["screen": .string(key)])
        XCTAssertNil(layout.styleDraft(of: key))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).appearance, .dark)

        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["material": .string("glass")])])
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hiddenSections": .array([.string("tokens")])])
        XCTAssertNil(layout.styleDraft(of: key), "a proposal is whole")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).material, .solid)
        XCTAssertEqual(changes, 0)
    }

    /// hud.reset proposes dark and solid with the rest of the default style, the HUD style's Reset to Default gives them
    /// at once, and Revert to Previous brings the screen's own look back after either.
    func testTheResetsGiveDarkAndSolid() throws {
        let key = "SECOND-UUID"
        settings.update { $0.screens[key] = ScreenPlacement(mode: .simple, appearance: .light, material: .glass) }
        changes = 0
        let saved = settings.settings
        let panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false))
        XCTAssertEqual(panel["layout"]?["hud"]["appearance"], .string("dark"), "listed by the fields that differ")
        XCTAssertEqual(panel["layout"]?["hud"]["material"], .string("solid"))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "solid"))
        layout.savePanel(on: key)
        XCTAssertNil(settings.settings.screens[key])
        layout.revert(.screen(key))
        XCTAssertEqual(settings.settings, saved)

        layout.resetLook(key)
        XCTAssertNil(settings.settings.screens[key], "Reset to Default in Settings")
        layout.revert(.screen(key))
        XCTAssertEqual(settings.settings.screens[key], ScreenPlacement(mode: .simple, appearance: .light, material: .glass))
    }

    /// From "previous" brings a version's look back with its layout, as a draft.
    func testFromPreviousBringsTheLookBack() throws {
        let key = "SECOND-UUID"
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["appearance": .string("light")])])
        layout.savePanel(on: key)
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "from": .string("previous")])
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).appearance, .dark)
        XCTAssertEqual(settings.settings.screens[key]?.appearance, .light, "as a draft, the settings as saved")
    }

    /// The glass style is a draft like the look's other fields. A solid HUD takes one and keeps it, as Settings keeps
    /// it while Solid is chosen, and the reply's note says it takes effect once the material is glass; on glass it is
    /// drawn at once, and get_hud's look names it, at the share of the transparency it draws.
    func testTheGlassStyleIsADraftThatASolidHUDKeeps() throws {
        let key = "SECOND-UUID"
        var panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["glassStyle": .string("regular")])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(glassStyle: .regular))
        XCTAssertNil(layout.draft(of: key), "the glass style alone leaves the layout as it is")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), ScreenPlacement(glassStyle: .regular))
        XCTAssertEqual(changes, 0, "a tool never writes the settings")
        let kept = "The resting HUD of \"Display 2\" would be solid, so its glass style \"regular\" is kept, as Settings "
            + "keeps it, and takes effect once its material is \"glass\"."
        XCTAssertTrue(panel["note"]?.stringValue?.contains(kept) == true, "\(panel["note"] ?? .null)")
        XCTAssertEqual(panel["layout"]?["hud"], .object(["glassStyle": .string("regular")]))
        var listed = try listing(key, in: panel)
        XCTAssertEqual(listed["hud"]?["glassStyle"], .string("regular"), "the style, as the draft has it")
        XCTAssertEqual(listed["look"], look("dark", "solid"), "drawn solid, with no glass style")

        panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                   "hud": .object(["material": .string("glass"), "glassStyle": .string("material")])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, glassStyle: .material))
        XCTAssertFalse(panel["note"]?.stringValue?.contains("would be solid") == true, "\(panel["note"] ?? .null)")
        listed = try listing(key, in: panel)
        XCTAssertEqual(listed["look"], look("dark", "glass", transparency: ScreenPlacement.standardGlassTransparency,
                                            glassStyle: "material"), "the material draws all of its transparency")

        // A screen saved as glass draws the style proposed at once; Save keeps it and Revert takes it back.
        settings.update { $0.screens[key] = ScreenPlacement(material: .glass, glassStyle: .regular) }
        changes = 0
        panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["glassStyle": .string("clear")])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(glassStyle: .clear))
        XCTAssertFalse(panel["note"]?.stringValue?.contains("would be solid") == true)
        XCTAssertEqual(try listing(key, in: panel)["look"],
                       look("dark", "glass", transparency: drawn(ScreenPlacement.standardGlassTransparency)))
        layout.savePanel(on: key)
        XCTAssertEqual(settings.settings.screens[key], ScreenPlacement(material: .glass))
        XCTAssertEqual(changes, 1)
        layout.revert(.screen(key))
        XCTAssertEqual(settings.settings.screens[key]?.glassStyle, .regular)

        // Reduce Transparency draws glass of any style solid, with no glass style, as the look's note says.
        reducesTransparency = true
        listed = try listing(key, in: try call(MCPTools.getHUD))
        XCTAssertEqual(listed["look"],
                       look("dark", "solid", note: "This Mac has Reduce Transparency on, so the HUD is drawn solid and opaque."))
        XCTAssertEqual(listed["hud"]?["glassStyle"], .string("regular"), "the style as chosen still")
    }

    /// get_hud lists beside the transparency in effect what each material keeps, solid's and glass's, which no proposal
    /// sets: the style listed sent back whole changes nothing, a kept transparency is taken where it repeats what the
    /// screen keeps now or would keep under the proposal, and any other is refused with what sets it instead.
    func testEachMaterialsTransparencyIsListedAndReadOnly() throws {
        let key = "SECOND-UUID"
        var hud = try XCTUnwrap(try listing(key, in: try call(MCPTools.getHUD))["hud"]?.objectValue)
        XCTAssertEqual(hud["transparency"], .number(0), "solid's, the material in effect")
        XCTAssertEqual(hud["solidTransparency"], .number(ScreenPlacement.standardSolidTransparency))
        XCTAssertEqual(hud["glassTransparency"], .number(ScreenPlacement.standardGlassTransparency))
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(hud)])
        XCTAssertNil(layout.styleDraft(of: key), "the style listed, sent back whole, changes nothing")
        XCTAssertNil(layout.draft(of: key), "and is the style alone")

        hud["transparency"] = .number(0.3)
        var panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(hud)])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(transparency: 0.3), "what was listed, with one field changed")
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["solidTransparency"], .number(0.3), "kept as the draft has it")
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["glassTransparency"], .number(ScreenPlacement.standardGlassTransparency))

        let sentence = try refused(["screen": .string(key), "hud": .object(["glassTransparency": .number(0.2)])])
        XCTAssertEqual(sentence, "Nothing was changed. hud.glassTransparency is read only: \"Display 2\" keeps 0.5 for glass, "
                       + "as get_hud lists it, and hud.transparency sets the transparency of the material its HUD would "
                       + "have, \"solid\". Leave it out, or give it as listed.")
        panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object([
            "material": .string("glass"), "transparency": .number(0.2), "glassTransparency": .number(0.2),
            "solidTransparency": .number(0),
        ])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, transparency: 0.2),
                       "what the proposal leaves each, given as it would be")
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["glassTransparency"], .number(0.2))
        XCTAssertEqual(try refused(["screen": .string(key), "hud": .object(["solidTransparency": .number(1.5)])]),
                       "Nothing was changed. hud.solidTransparency is a number from 0 to 1.", "out of range, said once")
        XCTAssertEqual(try refused(["screen": .string("all"), "hud": .object(["solidTransparency": .number(0)])]),
                       "Nothing was changed. hud.solidTransparency repeats what get_hud lists for one screen's resting HUD, "
                       + "so the proposal needs that screen's key.")

        // With a layout of its own saved, a hud of kept transparencies alone leaves it as it is, and so does a reset.
        layout.discardStyle(key)
        layout.propose(.own(PanelLayout(hiddenSections: [.tokens])), for: key)
        layout.save(.screen(key))
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["glassTransparency": .number(0.5)])])
        XCTAssertNil(layout.draft(of: key), "the layout as saved")
        XCTAssertNil(layout.styleDraft(of: key), "a proposal is whole: it leaves the style as saved")
        settings.update { $0.screens[key] = ScreenPlacement(material: .glass, glassTransparency: 0.8) }
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object([
            "reset": .bool(true), "glassTransparency": .number(ScreenPlacement.standardGlassTransparency),
        ])])
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false),
                       "glass's as the default keeps it")
        XCTAssertNil(layout.draft(of: key))
    }

    // MARK: What is refused

    /// A name this build does not know is refused with the names it takes; the panel's style has no look.
    func testAnUnknownLookIsRefused() throws {
        let appearance = try refused(["screen": .string("SECOND-UUID"), "hud": .object(["appearance": .string("auto")])])
        XCTAssertTrue(appearance.contains("hud.appearance is one of \"dark\", \"light\", \"system\"."), appearance)
        let material = try refused(["screen": .string("SECOND-UUID"), "hud": .object(["material": .string("frosted")])])
        XCTAssertTrue(material.contains("hud.material is one of \"solid\", \"glass\"."), material)
        let style = try refused(["screen": .string("SECOND-UUID"), "style": .object(["material": .string("glass")])])
        XCTAssertTrue(style.contains("The HUD's material is hud.material, for one screen."), style)
        let shared = try refused(["hud": .object(["appearance": .string("light")])])
        XCTAssertTrue(shared.contains("hud.appearance sets the resting HUD of one screen"), shared)
    }

    /// The notch at a notch takes the look as any HUD does, its panel and its cards drawing it while the notch's row
    /// stays dark: an appearance and a material are proposed there as a draft, with a mode or without.
    func testANotchAtANotchTakesTheLook() throws {
        let key = "BUILT-IN-UUID"
        let panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                       "hud": .object(["appearance": .string("light"),
                                                                       "material": .string("glass")])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(appearance: .light, material: .glass))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: true),
                       ScreenPlacement(mode: .notch, appearance: .light, material: .glass))
        XCTAssertEqual(try listing(key, in: panel)["look"],
                       look("light", "glass", transparency: drawn(ScreenPlacement.standardGlassTransparency)))
        _ = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                "hud": .object(["mode": .string("simple"), "material": .string("glass")])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(mode: .simple, position: 0, material: .glass))

        // The look in effect sent back changes nothing, the notch's included.
        _ = try call(MCPTools.discardHUDDraft, ["screen": .string(key)])
        let hud = try XCTUnwrap(try listing(key, in: try call(MCPTools.getHUD))["hud"]?.objectValue)
        XCTAssertEqual(hud["appearance"], .string("dark"))
        XCTAssertEqual(hud["transparency"], .number(0), "the solid HUD's own, as saved")
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(hud)])
        XCTAssertNil(layout.styleDraft(of: key))
    }

    /// A transparency is a draft like the look's other fields, and sets the material in effect's: solid's on a solid
    /// HUD, glass's on glass or where the same proposal makes it glass, each kept apart from the other. Either takes it
    /// to 1, solid as glass, kept as sent and drawn at three fifths, 0.6 at the top; a number past 1 is refused as any
    /// number out of its range is, whatever the material, and solid has no refusal of its own.
    func testATransparencySetsTheMaterialInEffect() throws {
        let key = "SECOND-UUID"
        for hud: [String: JSONValue] in [["transparency": .number(1.2)],
                                         ["material": .string("glass"), "transparency": .number(1.2)],
                                         ["material": .string("solid"), "transparency": .number(1.5)]] {
            let range = try refused(["screen": .string(key), "hud": .object(hud)])
            XCTAssertTrue(range.contains("hud.transparency is a number from 0 to 1."), range)
            XCTAssertFalse(range.contains("would be solid"), "no refusal of solid's own: \(range)")
        }
        XCTAssertNil(layout.styleDraft(of: key))

        var panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["transparency": .number(0.2)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(transparency: 0.2))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).solidTransparency, 0.2, "solid's")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).glassTransparency,
                       ScreenPlacement.standardGlassTransparency, "glass's as it was")
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "solid", transparency: drawn(0.2)))

        panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["transparency": .number(1)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(transparency: 1))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "solid", transparency: 0.6), "solid's top, drawn")
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["transparency"], .number(1), "as sent")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).glassTransparency,
                       ScreenPlacement.standardGlassTransparency, "glass's as it was")

        panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                   "hud": .object(["material": .string("glass"),
                                                                   "transparency": .number(0.7)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, transparency: 0.7))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "glass", transparency: drawn(0.7)))
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["transparency"], .number(0.7))
        layout.savePanel(on: key)
        XCTAssertEqual(settings.settings.screens[key], ScreenPlacement(material: .glass, glassTransparency: 0.7))
        XCTAssertEqual(changes, 1, "Save alone wrote the settings")

        panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(["transparency": .number(1)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(transparency: 1))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "glass", transparency: 0.6), "glass's top, the same")
        panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                   "hud": .object(["material": .string("solid"),
                                                                   "transparency": .number(1)])])
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "solid", transparency: 0.6),
                       "where the proposal makes it solid")
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).glassTransparency, 0.7, "glass's as saved")
    }

    /// A transparency proposed beside a new material is the new material's, kept where it equals what the old one had:
    /// solid at 0 made glass at 0 draws glass at 0, not glass's own 0.5, and glass made solid at glass's value draws solid
    /// at it. The reply, get_hud and the note say what is drawn, the draft read back keeps it, and Save writes it.
    func testATransparencyBesideANewMaterialIsKept() throws {
        let key = "SECOND-UUID"
        var panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                       "hud": .object(["material": .string("glass"),
                                                                       "transparency": .integer(0)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, transparency: 0))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false),
                       ScreenPlacement(material: .glass, glassTransparency: 0))
        XCTAssertEqual(panel["layout"]?["hud"], .object(["material": .string("glass"), "transparency": .number(0)]))
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["transparency"], .number(0))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "glass", transparency: 0))
        let note = try XCTUnwrap(panel["note"]?.stringValue)
        XCTAssertTrue(note.contains("shows its style as an unsaved draft"), note)
        XCTAssertFalse(note.contains("no hud field sets"), "the draft lists all it draws: \(note)")
        let read = try call(MCPTools.getHUD, ["screen": .string(key)])
        XCTAssertEqual(try listing(key, in: read)["look"], look("dark", "glass", transparency: 0))
        XCTAssertEqual(read["layout"]?["hud"]["transparency"], .number(0))

        let revision = layout.revision
        var again = try XCTUnwrap(panel["layout"]?.objectValue)
        again["screen"] = .string(key)
        _ = try call(MCPTools.draftHUD, again)
        _ = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                "hud": try XCTUnwrap(try listing(key, in: panel)["hud"])])
        XCTAssertEqual(layout.revision, revision, "the draft read back, and the style in effect sent back, keep it")
        layout.savePanel(on: key)
        XCTAssertEqual(settings.settings.screens[key], ScreenPlacement(material: .glass, glassTransparency: 0))

        settings.update { $0.screens[key] = ScreenPlacement(material: .glass, glassTransparency: 0.4) }
        panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                   "hud": .object(["material": .string("solid"),
                                                                   "transparency": .number(0.4)])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .solid, transparency: 0.4))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "solid", transparency: drawn(0.4)))
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["transparency"], .number(0.4))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).glassTransparency, 0.4, "glass's as saved")
    }

    /// A proposal that changes the material sets the transparency of the material it puts in effect, even where it is
    /// the value the other material keeps: glass at 0 on a screen whose solid HUD is opaque is glass at 0, not glass at
    /// what it kept, and the glass's transparency that proposal leaves, given as get_hud would list it, is taken with
    /// it. One that repeats what the new material keeps changes only the material.
    func testAChangeOfMaterialKeepsATransparencyThatRepeatsTheOtherMaterials() throws {
        let key = "SECOND-UUID"
        let glassAtNone: [String: JSONValue] = ["material": .string("glass"), "transparency": .number(0)]
        var panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(glassAtNone)])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, transparency: 0))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false).glassTransparency, 0, "glass at 0, as asked")
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "glass", transparency: 0))
        XCTAssertEqual(try listing(key, in: panel)["hud"]?["glassTransparency"], .number(0))

        panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object(glassAtNone.merging([
            "glassTransparency": .number(0), "solidTransparency": .number(0),
        ]) { $1 })])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, transparency: 0),
                       "what the proposal leaves each, given as get_hud would list it")

        layout.discardStyle(key)
        settings.update { $0.screens[key] = ScreenPlacement(mode: .logos, edge: .right, solidTransparency: 0.2) }
        panel = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object([
            "material": .string("glass"), "transparency": .number(0.2),
        ])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass, transparency: 0.2))
        XCTAssertEqual(try listing(key, in: panel)["look"], look("dark", "glass", transparency: drawn(0.2)))
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "hud": .object([
            "material": .string("glass"), "transparency": .number(ScreenPlacement.standardGlassTransparency),
        ])])
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(material: .glass), "glass's own, repeated, changes nothing")
        XCTAssertEqual(changes, 1, "the settings written by the test alone")
    }

    // MARK: What is read

    /// get_hud says what each screen draws now: a notch at a notch in its look as any HUD, `system` as macOS is set,
    /// glass with its transparency, and glass solid under Reduce Transparency, with a note where it draws other than its
    /// style asks, and then no transparency.
    func testTheListingSaysWhatEachScreenDraws() throws {
        settings.update {
            $0.screens["BUILT-IN-UUID"] = ScreenPlacement(mode: .notch, appearance: .light, material: .glass)
            $0.screens["SECOND-UUID"] = ScreenPlacement(appearance: .system, material: .glass)
        }
        var panel = try call(MCPTools.getHUD)
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["look"],
                       look("light", "glass", transparency: drawn(ScreenPlacement.standardGlassTransparency)))
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["hud"]?["appearance"], .string("light"), "as saved")
        XCTAssertEqual(try listing("SECOND-UUID", in: panel)["look"],
                       look("dark", "glass", transparency: drawn(ScreenPlacement.standardGlassTransparency)))

        systemIsLight = true
        reducesTransparency = true
        panel = try call(MCPTools.getHUD)
        XCTAssertEqual(try listing("SECOND-UUID", in: panel)["look"],
                       look("light", "solid", note: "This Mac has Reduce Transparency on, so the HUD is drawn solid and opaque."))
        XCTAssertEqual(try listing("SECOND-UUID", in: panel)["hud"]?["material"], .string("glass"))
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["look"],
                       look("light", "solid", note: "This Mac has Reduce Transparency on, so the HUD is drawn solid and opaque."),
                       "the notch's too")
    }

    /// The look in effect gives the transparency of the material drawn as the screen has it, a style draft's included,
    /// and as it is drawn: solid's for a HUD drawn solid, whatever it keeps for glass, and glass's once it is glass.
    func testTheLookInEffectGivesTheTransparencyOfTheMaterial() throws {
        settings.update {
            $0.screens["BUILT-IN-UUID"] = ScreenPlacement(mode: .notch, material: .solid, glassTransparency: 0.8)
            $0.screens["SECOND-UUID"] = ScreenPlacement(appearance: .light, material: .glass, glassTransparency: 0.3)
        }
        var panel = try call(MCPTools.getHUD)
        XCTAssertEqual(try listing("SECOND-UUID", in: panel)["look"], look("light", "glass", transparency: drawn(0.3)))
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["look"], look("dark", "solid"))
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["hud"]?["transparency"], .number(0), "solid's, as saved")

        panel = try call(MCPTools.draftHUD, ["screen": .string("SECOND-UUID"),
                                                   "hud": .object(["transparency": .number(0.65)])])
        XCTAssertEqual(try listing("SECOND-UUID", in: panel)["look"], look("light", "glass", transparency: drawn(0.65)))
        panel = try call(MCPTools.draftHUD, ["screen": .string("BUILT-IN-UUID"),
                                                   "hud": .object(["material": .string("glass")])])
        XCTAssertEqual(try listing("BUILT-IN-UUID", in: panel)["look"], look("dark", "glass", transparency: drawn(0.8)),
                       "the transparency kept while solid")
    }

    /// The look in effect gives the transparency as each surface draws it, and the style the one chosen: Solid, clear
    /// glass and regular glass three fifths of it, 0.6 at the top, and the material all of it, the bare material at 1; a
    /// solid HUD at its share whatever glass it keeps for when glass is chosen again; and none under Reduce Transparency.
    func testTheLookInEffectGivesTheTransparencyAsEachSurfaceDrawsIt() throws {
        let key = "SECOND-UUID"
        for (style, top) in [(HUDGlassStyle.clear, 0.6), (.regular, 0.6), (.material, 1)] {
            settings.update { $0.screens[key] = ScreenPlacement(material: .glass, glassStyle: style, glassTransparency: 1) }
            let listed = try listing(key, in: try call(MCPTools.getHUD))
            XCTAssertEqual(listed["look"], look("dark", "glass", transparency: top, glassStyle: style.rawValue), "\(style)")
            XCTAssertEqual(listed["hud"]?["transparency"], .number(1), "\(style): the style as chosen")
        }
        settings.update { $0.screens[key] = ScreenPlacement(material: .glass, glassStyle: .material, glassTransparency: 0.4) }
        XCTAssertEqual(try listing(key, in: try call(MCPTools.getHUD))["look"],
                       look("dark", "glass", transparency: 0.4, glassStyle: "material"), "the material, all of it")

        settings.update { $0.screens[key] = ScreenPlacement(glassStyle: .material, solidTransparency: 0.5) }
        var listed = try listing(key, in: try call(MCPTools.getHUD))
        XCTAssertEqual(listed["look"], look("dark", "solid", transparency: drawn(0.5)), "solid, whatever glass it keeps")
        XCTAssertEqual(listed["hud"]?["transparency"], .number(0.5))

        reducesTransparency = true
        listed = try listing(key, in: try call(MCPTools.getHUD))
        XCTAssertEqual(listed["look"],
                       look("dark", "solid", note: "This Mac has Reduce Transparency on, so the HUD is drawn solid and opaque."))
        XCTAssertEqual(listed["hud"]?["transparency"], .number(0.5), "the style as chosen still")
    }

    /// The pictures say glass is a stand-in where they draw it, and the resting HUD's says the look it is drawn in.
    func testThePicturesSayGlassIsAStandIn() throws {
        let key = "SECOND-UUID"
        var hud = try call(MCPTools.getHUDScreenshot, ["screen": .string(key), "show": .string("hud")])
        XCTAssertEqual(hud["appearance"], .string("dark"))
        XCTAssertEqual(hud["material"], .string("solid"))
        XCTAssertFalse(hud["note"]?.stringValue?.contains("stand-in") == true, "\(hud["note"] ?? .null)")
        XCTAssertTrue(hud["note"]?.stringValue?.contains(", dark and solid, on a dark backdrop") == true, "\(hud["note"] ?? .null)")

        _ = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                "hud": .object(["appearance": .string("light"), "material": .string("glass")])])
        hud = try call(MCPTools.getHUDScreenshot, ["screen": .string(key), "show": .string("hud")])
        XCTAssertEqual(hud["appearance"], .string("light"))
        XCTAssertEqual(hud["material"], .string("glass"))
        let note = try XCTUnwrap(hud["note"]?.stringValue)
        XCTAssertTrue(note.contains(", light and glass, on a dark backdrop"), note)
        XCTAssertTrue(note.contains(HUDTools.glassStandIn), note)
        let panel = try call(MCPTools.getHUDScreenshot, ["screen": .string(key)])
        XCTAssertTrue(panel["note"]?.stringValue?.contains(HUDTools.glassStandIn) == true, "\(panel["note"] ?? .null)")
        XCTAssertFalse(try call(MCPTools.getHUDScreenshot)["note"]?.stringValue?.contains(HUDTools.glassStandIn) == true,
                       "the shared layout is drawn in the main display's look, solid")

        reducesTransparency = true
        hud = try call(MCPTools.getHUDScreenshot, ["screen": .string(key), "show": .string("hud")])
        XCTAssertEqual(hud["material"], .string("solid"), "Reduce Transparency draws it solid")
        XCTAssertFalse(hud["note"]?.stringValue?.contains(HUDTools.glassStandIn) == true)

        let notch = try call(MCPTools.getHUDScreenshot, ["screen": .string("BUILT-IN-UUID"), "show": .string("hud")])
        XCTAssertTrue(notch["note"]?.stringValue?.contains(": the island at its notch, black at rest, as the notch is; a "
                                                           + "reminder's wings are dark and solid, and the panel and the "
                                                           + "cards it opens into dark and solid, on a dark backdrop") == true,
                      "\(notch["note"] ?? .null)")
    }

    /// The resting HUD's picture of the island at a notch draws it black, as it rests there in any look, and says so: its
    /// fields are dark and solid, with no glass to stand in for, and its note says that a reminder's wings are dark in
    /// the look's material while what it opens into takes the look whole.
    func testThePictureOfTheNotchSaysItsRowStaysDark() throws {
        let key = "BUILT-IN-UUID"
        _ = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                "hud": .object(["appearance": .string("light"), "material": .string("glass")])])
        let notch = try call(MCPTools.getHUDScreenshot, ["screen": .string(key), "show": .string("hud")])
        let note = try XCTUnwrap(notch["note"]?.stringValue)
        XCTAssertTrue(note.contains(": the island at its notch, black at rest, as the notch is; a reminder's wings are dark "
                                    + "and glass, and the panel and the cards it opens into light and glass, on a dark "
                                    + "backdrop"), note)
        XCTAssertFalse(note.contains(HUDTools.glassStandIn), note)
        XCTAssertEqual(notch["appearance"], .string("dark"))
        XCTAssertEqual(notch["material"], .string("solid"))

        let bar = try call(MCPTools.getHUDScreenshot, ["screen": .string("SECOND-UUID"), "show": .string("hud")])
        XCTAssertFalse(bar["note"]?.stringValue?.contains("black at rest") == true, "a dock has no notch's row")
    }
}
