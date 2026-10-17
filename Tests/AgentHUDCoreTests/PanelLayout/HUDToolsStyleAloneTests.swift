import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// A call that gives one screen's style alone, beside its screen and revision, changes that style and nothing else: the
/// screen's layout, saved or as a draft, its own or the shared one, stays as it is, and the note says so. A call that
/// names a layout field still proposes the whole layout with the style beside it. No tool writes the settings.
final class HUDToolsStyleAloneTests: HUDToolsCase {
    private let builtIn = PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                      agents: ["Claude", "Codex", "DeepSeek"],
                                      otherModes: [.logos: ["Claude", "Codex", "Cursor"], .simple: ["Claude", "Codex", "Kimi"]],
                                      systemIsLight: false, reducesTransparency: false)
    private let second = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                                     agents: ["Claude", "Codex", "Cursor"],
                                     systemIsLight: false, reducesTransparency: false)
    private let key = "SECOND-UUID"
    private let unchanged = " The layout of the panel on \"Display 2\" is unchanged: a call with only hud style fields leaves "
        + "the layout as it is."

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] })

    private func call(_ arguments: [String: JSONValue], line: UInt = #line) throws -> [String: JSONValue] {
        let request = MCPToolRequest(tool: MCPTools.draftHUD, arguments: arguments)
        guard case .result(let result, _) = screenTools.answer(request) else {
            XCTFail("the proposal was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    private func note(_ panel: [String: JSONValue]) -> String { panel["note"]?.stringValue ?? "" }

    /// The second display keeps a layout of its own that hides the token chart.
    private func keepOwnLayout() {
        layout.propose(.own(PanelLayout(hiddenSections: [.tokens])), for: key)
        layout.save(.screen(key))
        changes = 0
    }

    // MARK: The layout stays

    /// A screen with a layout of its own keeps it: no layout draft is proposed, the style is its style draft, and the
    /// note says the layout is unchanged. A revision read beside the style is taken as before.
    func testTheStyleAloneKeepsTheScreensOwnLayout() throws {
        keepOwnLayout()
        let saved = settings.settings
        let panel = try call(["screen": .string(key), "revision": .integer(Int64(layout.revision)),
                              "hud": .object(["logoSize": .integer(30)])])
        XCTAssertNil(layout.draft(of: key), "no layout draft")
        XCTAssertEqual(layout.ownLayout(of: key), PanelLayout(hiddenSections: [.tokens]))
        XCTAssertEqual(layout.effective(for: key), PanelLayout(hiddenSections: [.tokens]), "its panel draws its own layout")
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(logoSize: 30))
        XCTAssertEqual(panel["layout"]?["hiddenSections"], .array([.string("tokens")]))
        XCTAssertEqual(note(panel), "The resting HUD of \"Display 2\" shows its style as an unsaved draft; its panel's layout "
                       + "is the one it has saved. The user keeps the style by hovering the Agent HUD on that display and "
                       + "pressing Save in the panel; it is dropped when Agent HUD restarts, and the style also when the "
                       + "user moves that HUD or changes that display in Settings." + unchanged)
        XCTAssertEqual(settings.settings, saved, "a tool writes nothing")
        XCTAssertEqual(changes, 0)

        // Save keeps the style beside the same layout.
        layout.savePanel(on: key)
        XCTAssertEqual(layout.ownLayout(of: key), PanelLayout(hiddenSections: [.tokens]))
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 30)
    }

    /// A screen on the shared layout stays on it, the shared layout's draft included, which it goes on drawing; a
    /// style alone proposed for every screen at once changes nothing.
    func testTheStyleAloneKeepsAScreenOnTheSharedLayout() throws {
        layout.propose(PanelLayout(hiddenSections: [.sessions]))
        layout.save()
        _ = try call(["screen": .string(key), "hud": .object(["edge": .string("bottom")])])
        XCTAssertTrue(layout.usesShared(key))
        XCTAssertNil(layout.draft(of: key))
        XCTAssertNil(layout.ownLayout(of: key))
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(edge: .bottom))

        layout.discard(.screen(key))
        layout.propose(PanelLayout(hiddenSections: [.tokens]))
        let panel = try call(["screen": .string(key), "hud": .object(["logoSize": .integer(30)])])
        XCTAssertTrue(layout.usesShared(key))
        XCTAssertNil(layout.draft(of: key), "no draft of its own")
        XCTAssertEqual(layout.draft, PanelLayout(hiddenSections: [.tokens]), "the shared draft waits as it was")
        XCTAssertEqual(layout.effective(for: key), PanelLayout(hiddenSections: [.tokens]), "and the screen draws it")
        XCTAssertEqual(note(panel), "The resting HUD of \"Display 2\" shows its style as an unsaved draft, and its panel the "
                       + "shared layout's unsaved draft. The user keeps both by hovering the Agent HUD on that display and "
                       + "pressing Save in the panel; they are dropped when Agent HUD restarts, and the style also when the "
                       + "user moves that HUD or changes that display in Settings." + unchanged)

        let revision = layout.revision
        let shared = try call(["screen": .string("all"), "hud": .object(["reset": .bool(false)])])
        XCTAssertEqual(layout.revision, revision, "nothing to change")
        XCTAssertTrue(note(shared).hasSuffix(" The shared layout is unchanged: a call with only hud style fields leaves "
                                             + "the layout as it is."), note(shared))
        XCTAssertEqual(changes, 0)
    }

    /// A layout draft waiting for the screen stays as it was, its own or one to use the shared layout, and the style
    /// waits beside it, so one Save keeps both.
    func testTheStyleAloneKeepsAWaitingLayoutDraft() throws {
        keepOwnLayout()
        _ = try call(["screen": .string(key), "hiddenSections": .array([.string("sessions")])])
        let panel = try call(["screen": .string(key), "hud": .object(["agents": .null, "showLogos": .bool(false)])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hiddenSections: [.sessions])), "the draft as it was")
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(showLogos: false))
        XCTAssertEqual(note(panel), "Shown on \"Display 2\" as an unsaved draft of its own, with its resting HUD's style as an "
                       + "unsaved draft too. The user keeps both by hovering the Agent HUD on that display and pressing "
                       + "Save in the panel; they are dropped when Agent HUD restarts, and the style also when the user "
                       + "moves that HUD or changes that display in Settings." + unchanged)

        _ = try call(["screen": .string(key), "from": .string("shared")])
        _ = try call(["screen": .string(key), "hud": .object(["logoSize": .integer(30)])])
        XCTAssertEqual(layout.draft(of: key), .shared, "the draft to use the shared layout as it was")
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(logoSize: 30))
        layout.savePanel(on: key)
        XCTAssertTrue(layout.usesShared(key))
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 30)
    }

    /// hud.reset alone proposes the default style and leaves the layout, saved or as a draft, as it is.
    func testHUDResetAloneKeepsTheLayout() throws {
        keepOwnLayout()
        settings.update { $0.screens[key] = ScreenPlacement(mode: .simple, edge: .left, simpleSize: 30) }
        let panel = try call(["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertNil(layout.draft(of: key))
        XCTAssertEqual(layout.ownLayout(of: key), PanelLayout(hiddenSections: [.tokens]))
        XCTAssertNotNil(layout.styleDraft(of: key))
        XCTAssertEqual(settings.effectivePlacement(on: key, hasNotch: false), .default(hasNotch: false))
        XCTAssertTrue(note(panel).hasSuffix(unchanged), note(panel))

        _ = try call(["screen": .string(key), "hiddenSections": .array([.string("sessions")]),
                      "hud": .object(["reset": .bool(true)])])
        _ = try call(["screen": .string(key), "hud": .object(["reset": .bool(true)])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hiddenSections: [.sessions])), "the draft as it was")
        XCTAssertNotNil(layout.styleDraft(of: key))
    }

    // MARK: A layout beside the style

    /// A call that names a layout field, hud.agents included, proposes the whole layout, with anything left out at its
    /// default, and the style beside it, and its note does not say the layout is unchanged.
    func testALayoutFieldBesideTheStyleProposesBoth() throws {
        keepOwnLayout()
        let panel = try call(["screen": .string(key), "hiddenSections": .array([.string("sessions")]),
                              "hud": .object(["logoSize": .integer(30)])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hiddenSections: [.sessions])))
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(logoSize: 30))
        XCTAssertFalse(note(panel).contains("unchanged"), note(panel))

        _ = try call(["screen": .string(key), "hud": .object(["agents": .array([.string("Codex")]), "logoSize": .integer(24)])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hud: PanelLayout.HUD(agents: ["Codex"]))),
                       "the agents are the layout's, so the rest of it is the default")
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(logoSize: 24))

        // A hud that gives nothing is no style: the call is the default layout, as one without a hud is.
        _ = try call(["screen": .string(key), "hud": .object(["logoSize": .null])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout()))
        XCTAssertNil(layout.styleDraft(of: key))
        XCTAssertEqual(changes, 0)
    }
}
