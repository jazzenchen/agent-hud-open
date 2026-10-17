import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// The hold an agent's edit puts on a screen's panel: started or renewed for two minutes by each edit of a layout and
/// each drop of a draft, on the screen whose panel draws it, paused by a call that restyles the resting HUD alone, ended
/// by its time, by finish_hud_edit and by a person's choice, and read back in get_hud. The holds run on a clock the
/// tests move, with waits that end when it gets there, so the two minutes pass without waiting, and they write no
/// settings and count no revision.
final class HUDToolsHoldTests: HUDToolsCase {
    private let builtIn = PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                      agents: ["Claude", "Codex", "DeepSeek"],
                                      systemIsLight: false, reducesTransparency: false)
    private let second = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                                     agents: ["Claude", "Codex", "Cursor"],
                                     systemIsLight: false, reducesTransparency: false, hasPointer: true)
    private lazy var attached = [builtIn, second]
    private var now = Date(timeIntervalSince1970: 1_800_000_000)
    /// The holds' waits, each until a time on the test's clock.
    private var waits: [(until: Date, resume: CheckedContinuation<Void, Never>)] = []

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in attached })

    override func setUp() {
        super.setUp()
        MainActor.assumeIsolated {
            holds.clock = { [unowned self] in now }
            holds.sleep = { [weak self] until in
                guard let self, until > now else { return }
                await withCheckedContinuation { waits.append((until, $0)) }
            }
        }
    }

    override func tearDown() {
        MainActor.assumeIsolated {
            holds.endAll()
            let all = waits
            waits = []
            for wait in all { wait.resume.resume() }
        }
        super.tearDown()
    }

    /// Moves the clock on and ends the waits it reaches, as time passing would.
    private func advance(_ seconds: TimeInterval) {
        now += seconds
        let due = waits.filter { $0.until <= now }
        waits.removeAll { $0.until <= now }
        for wait in due { wait.resume.resume() }
    }

    /// Lets what the main actor has queued run, until `condition` holds or `seconds` have passed.
    private func settle(within seconds: TimeInterval = 2, until condition: () -> Bool) async throws {
        let deadline = Date().addingTimeInterval(seconds)
        while !condition(), Date() < deadline { try await Task.sleep(for: .milliseconds(5)) }
    }

    private var holds: PanelHolds { layout.holds }
    private var held: [String] { holds.screens }
    /// Whether no hold is kept at all, past its time or not: what the clock alone cannot tell.
    private var swept: Bool {
        let at = now
        now = .distantPast
        defer { now = at }
        return held.isEmpty
    }

    private func call(_ tool: String, _ arguments: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
        guard case .result(let result, _) = screenTools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("\(tool) was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    private func refused(_ tool: String, _ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        guard case .failure(let sentence) = screenTools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("the call was taken", line: line)
            throw CancellationError()
        }
        return sentence
    }

    /// An edit of one screen's own layout, or of the shared one: Codex's block first, or the blocks as `blocks` sets
    /// them.
    @discardableResult
    private func edit(_ key: String?, _ blocks: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
        var arguments: [String: JSONValue] = blocks.isEmpty ? ["order": .array([.string("Codex")])] : ["blocks": .object(blocks)]
        if let key { arguments["screen"] = .string(key) }
        return try call(MCPTools.draftHUD, arguments, line: line)
    }

    private func listing(_ key: String, in panel: [String: JSONValue]) throws -> [String: JSONValue] {
        try XCTUnwrap((panel["screens"]?.arrayValue ?? []).compactMap(\.objectValue).first { $0["key"] == .string(key) })
    }

    private let ring: JSONValue = .object(["widget": .string("ring")])

    // MARK: Starting, renewing and running out

    /// A layout edit for a screen holds that screen's panel for two minutes, says so in every screen's listing, and
    /// changes no setting and no revision.
    func testALayoutEditHoldsItsScreensPanelForTwoMinutes() throws {
        let before = try call(MCPTools.getHUD)
        for key in [builtIn.key, second.key] {
            XCTAssertEqual(try listing(key, in: before)["held"], .bool(false))
            XCTAssertNil(try listing(key, in: before)["holdSecondsLeft"])
            XCTAssertNil(try listing(key, in: before)["holdPaused"])
        }

        let panel = try edit(second.key)
        XCTAssertEqual(held, [second.key])
        XCTAssertEqual(holds.remaining(on: second.key), 120)
        XCTAssertEqual(holds.hold(on: second.key)?.paused, false)
        let listed = try listing(second.key, in: panel)
        XCTAssertEqual(listed["held"], .bool(true), "the answer already tells the hold it started")
        XCTAssertEqual(listed["holdSecondsLeft"], .integer(120))
        XCTAssertEqual(listed["holdPaused"], .bool(false))
        XCTAssertEqual(try listing(builtIn.key, in: panel)["held"], .bool(false), "the other screen is not held")

        let revision = layout.revision
        _ = try call(MCPTools.finishHUDEdit)
        _ = try edit(second.key)
        XCTAssertEqual(layout.revision, revision, "holding and letting go count no revision; the same edit again changes nothing")
        XCTAssertEqual(held, [second.key], "an edit that changes nothing is still an edit, and holds")
        XCTAssertEqual(changes, 0, "and nothing writes a setting")
    }

    /// Each edit renews the hold for two minutes from then; two minutes without one end it, which get_hud reads, and
    /// the hold that ran out is gone once its time is swept, not only hidden by the clock.
    func testAnEditRenewsTheHoldAndTwoMinutesWithoutOneEndIt() throws {
        try edit(second.key)
        now += 90
        XCTAssertEqual(try listing(second.key, in: try call(MCPTools.getHUD))["holdSecondsLeft"], .integer(30))
        try edit(second.key, ["Claude": ring])
        XCTAssertEqual(holds.remaining(on: second.key), 120, "renewed from the second edit")

        now += 119.5
        XCTAssertEqual(try listing(second.key, in: try call(MCPTools.getHUD))["holdSecondsLeft"], .integer(1),
                       "whole seconds, rounded up")
        now += 0.5
        XCTAssertNil(holds.hold(on: second.key), "two minutes after the last edit")
        let panel = try call(MCPTools.getHUD)
        XCTAssertEqual(try listing(second.key, in: panel)["held"], .bool(false))
        XCTAssertNil(try listing(second.key, in: panel)["holdSecondsLeft"])

        holds.expire()
        now -= 10
        XCTAssertNil(holds.hold(on: second.key), "swept away")
        XCTAssertTrue(held.isEmpty)
    }

    /// The hold ends by itself two minutes after the last edit, with nobody reading it: each edit waits anew from its own
    /// time, and when the wait ends the hold is gone, not only past its time.
    func testTheHoldEndsByItselfTwoMinutesAfterTheLastEdit() async throws {
        try edit(second.key)
        advance(90)
        try edit(second.key, ["Claude": ring])
        advance(119)
        try await settle(within: 0.1) { swept }
        XCTAssertEqual(held, [second.key], "the first edit's two minutes have passed, the second's have not")
        advance(1)
        XCTAssertTrue(held.isEmpty, "two minutes after the last edit")
        try await settle { swept }
        XCTAssertTrue(swept, "and gone once its wait ends, not only past its time")
    }

    /// A proposal from a version holds as an edit does; a refused proposal holds nothing, and neither does an edit for a
    /// screen that is not attached, which has no panel.
    func testOnlyAnEditThatWasMadeOfAnAttachedScreenHolds() throws {
        let sentence = try refused(MCPTools.draftHUD, ["screen": .string(second.key), "order": .string("Codex")])
        XCTAssertTrue(sentence.hasPrefix("Nothing was changed."), sentence)
        XCTAssertTrue(held.isEmpty)

        _ = try call(MCPTools.draftHUD, ["screen": .string(builtIn.key), "from": .string("default")])
        XCTAssertEqual(held, [builtIn.key], "from proposes a layout too")
        _ = try call(MCPTools.finishHUDEdit)

        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "AWAY-UUID")
        _ = try edit("AWAY-UUID", ["Claude": ring])
        XCTAssertTrue(held.isEmpty, "a screen that is away has no panel to hold")
    }

    /// An edit of the shared layout holds the panel of the screen the person is at, where that screen draws the shared
    /// layout, else of the first attached screen that does; with none that does, or none attached, it holds none.
    func testAnEditOfTheSharedLayoutHoldsThePanelOfTheScreenThePersonIsAt() throws {
        try edit(nil)
        XCTAssertEqual(held, [second.key], "the pointer's screen")
        _ = try call(MCPTools.finishHUDEdit)

        attached = [builtIn, PanelScreen(key: second.key, name: second.name, mode: .logos, edge: .right, hasNotch: false,
                                         agents: second.agents, systemIsLight: false, reducesTransparency: false)]
        try edit(nil, ["Claude": ring])
        XCTAssertEqual(held, [builtIn.key], "where no screen has the pointer, the main display")
        _ = try call(MCPTools.finishHUDEdit)

        attached = [builtIn, second]
        layout.propose(.own(PanelLayout(hiddenSections: [.tokens])), for: second.key)
        layout.save(.screen(second.key))
        try edit(nil, ["Claude": .object(["widget": .string("number")])])
        XCTAssertEqual(held, [builtIn.key], "the pointer's screen draws a layout of its own, so the first that draws the shared")
        _ = try call(MCPTools.finishHUDEdit)

        layout.propose(.own(PanelLayout(hiddenSections: [.tokens])), for: builtIn.key)
        layout.save(.screen(builtIn.key))
        try edit(nil, ["Claude": .object(["widget": .string("chart")])])
        XCTAssertTrue(held.isEmpty, "no screen draws the shared layout")

        attached = []
        try edit(nil, ["Claude": ring])
        XCTAssertTrue(held.isEmpty)
    }

    // MARK: Ending

    /// finish_hud_edit lets go of one screen's panel, or with "all" or no screen of every one, and says which; a
    /// screen that was not held changes nothing, and a call it cannot read is refused with nothing changed. Drafts stay.
    func testFinishingLetsGoOfOneScreenOrOfEvery() throws {
        try edit(second.key)
        try edit(builtIn.key)
        let one = try call(MCPTools.finishHUDEdit, ["screen": .string(second.key)])
        XCTAssertEqual(held, [builtIn.key])
        XCTAssertEqual(one["note"], .string("The panel on \"Display 2\" is no longer held open: it closes unless the user's "
                                            + "pointer is on it. The draft waits for the user to press Save in the panel."))
        XCTAssertEqual(try listing(second.key, in: one)["held"], .bool(false))
        XCTAssertEqual(one["screen"], .string(second.key), "it answers with that screen's panel")
        XCTAssertTrue(layout.hasDraft(on: second.key), "the draft stays")
        XCTAssertEqual(try call(MCPTools.finishHUDEdit, ["screen": .string(second.key)])["note"],
                       .string("The panel on \"Display 2\" was not held open, so nothing changed. The draft waits for the "
                               + "user to press Save in the panel."))

        XCTAssertTrue(try refused(MCPTools.finishHUDEdit, ["screen": .string("GONE")]).hasPrefix("Nothing was changed. No screen"))
        XCTAssertEqual(try refused(MCPTools.finishHUDEdit, ["screen": .string("all"), "done": .bool(true)]),
                       "Nothing was changed. finish_hud_edit takes only screen. Unknown: \"done\".")
        XCTAssertEqual(held, [builtIn.key], "a refusal lets go of nothing")

        try edit(second.key, ["Claude": ring])
        let every = try call(MCPTools.finishHUDEdit)
        XCTAssertTrue(held.isEmpty)
        XCTAssertEqual(every["screen"], .string("all"))
        XCTAssertEqual(every["note"], .string("No panel is held open now: the panels on \"Built-in display\", \"Display 2\" "
                                              + "close unless the user's pointer is on them. Drafts wait for the user to "
                                              + "press Save in the panel."))
        try edit(second.key)
        _ = try call(MCPTools.finishHUDEdit, ["screen": .string("all")])
        XCTAssertTrue(held.isEmpty, "\"all\" is every screen")
        XCTAssertEqual(try call(MCPTools.finishHUDEdit)["note"], .string("No panel was held open, so nothing changed. "
                                                                           + "Drafts wait for the user to press Save in the panel."))
        layout.discard(.screen(second.key))
        layout.discard(.screen(builtIn.key))
        XCTAssertEqual(try call(MCPTools.finishHUDEdit)["note"], .string("No panel was held open, so nothing changed."))
        XCTAssertEqual(changes, 0)
    }

    /// Dropping a draft is an edit too, which holds the panel it changes: a screen's own draft that screen's, and the
    /// shared draft the person's screen's. Dropping a screen's style draft alone closes its held panel for now, as a
    /// call with only hud style fields does.
    func testDiscardingADraftHoldsItsPanelAsAnEditDoes() throws {
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: builtIn.key)
        _ = try call(MCPTools.discardHUDDraft, ["screen": .string(builtIn.key)])
        XCTAssertEqual(held, [builtIn.key], "the drop starts a hold")
        XCTAssertEqual(holds.hold(on: builtIn.key)?.agent, "Claude", "the first block whose place the drop changed")

        now += 60
        layout.propose(PanelLayout(hiddenSections: [.tokens]))
        _ = try call(MCPTools.discardHUDDraft)
        XCTAssertEqual(held, [builtIn.key, second.key], "the shared draft's drop holds the pointer's screen")
        XCTAssertEqual(holds.remaining(on: second.key), 120)
        XCTAssertEqual(holds.remaining(on: builtIn.key), 60, "and leaves the other's as it was")

        _ = try call(MCPTools.draftHUD, ["screen": .string(second.key), "hud": .object(["logoSize": .integer(30)])])
        XCTAssertEqual(holds.hold(on: second.key)?.paused, true)
        try edit(second.key, ["Claude": ring])
        XCTAssertEqual(holds.hold(on: second.key)?.paused, false)
        layout.discard(.screen(second.key))
        _ = try call(MCPTools.draftHUD, ["screen": .string(second.key), "hud": .object(["logoSize": .integer(32)])])
        XCTAssertEqual(holds.hold(on: second.key)?.paused, true, "a style alone closes it")
        try edit(second.key, ["Claude": ring])
        _ = try call(MCPTools.draftHUD, ["screen": .string(second.key), "hud": .object(["logoSize": .integer(32)]),
                                               "blocks": .object(["Claude": ring])])
        XCTAssertEqual(holds.hold(on: second.key)?.paused, false, "a style beside a layout keeps it open")
        layout.discard(.screen(second.key))
        layout.proposeStyle(HUDStyle(logoSize: 30), for: second.key, hasNotch: false)
        _ = try call(MCPTools.discardHUDDraft, ["screen": .string(second.key)])
        XCTAssertNil(layout.styleDraft(of: second.key))
        XCTAssertEqual(holds.hold(on: second.key)?.paused, true, "the style's drop alone closes it too, so the HUD shows")
        XCTAssertEqual(changes, 0)
    }

    /// A call that restyles a screen's resting HUD alone pauses its hold, so the panel closes and the HUD shows, without
    /// renewing it, and starts none; the next layout edit opens it again for two minutes.
    func testAStyleAloneClosesTheHeldPanelUntilTheNextLayoutEdit() throws {
        let style: [String: JSONValue] = ["screen": .string(second.key), "hud": .object(["logoSize": .integer(30)])]
        _ = try call(MCPTools.draftHUD, style)
        XCTAssertTrue(held.isEmpty, "the style alone opens nothing")

        try edit(second.key)
        now += 30
        let paused = try call(MCPTools.draftHUD, style.merging(["hud": .object(["logoSize": .integer(32)])]) { $1 })
        XCTAssertEqual(holds.hold(on: second.key)?.paused, true)
        XCTAssertEqual(holds.remaining(on: second.key), 90, "not renewed")
        XCTAssertEqual(try listing(second.key, in: paused)["holdPaused"], .bool(true))
        XCTAssertEqual(try listing(second.key, in: paused)["held"], .bool(true))

        try edit(second.key, ["Claude": ring])
        XCTAssertEqual(holds.hold(on: second.key)?.paused, false)
        XCTAssertEqual(holds.remaining(on: second.key), 120)
    }

    /// The hold names the agent the edit touches first, whose card a dock that separates its agents opens: the first
    /// block, in the panel's order, whose settings or place changed, an API balance by its vendor. An edit that touches
    /// no block names none, and a renewal keeps the agent named before.
    func testTheHoldNamesTheFirstAgentTheEditTouches() throws {
        let key = second.key
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "style": .object(["spacing": .string("compact")])])
        XCTAssertNil(holds.hold(on: key)?.agent, "the style alone touches no agent")
        XCTAssertEqual(held, [key], "and holds all the same")

        let hidden: JSONValue = .object(["hidden": .bool(true)])
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "blocks": .object(["api:DeepSeek": hidden])])
        XCTAssertEqual(holds.hold(on: key)?.agent, "DeepSeek")
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "blocks": .object(["api:DeepSeek": hidden]),
                                               "style": .object(["spacing": .string("roomy")])])
        XCTAssertEqual(holds.hold(on: key)?.agent, "DeepSeek", "a renewal that touches no block keeps the agent")

        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "blocks": .object(["api:DeepSeek": hidden]),
                                               "order": .array([.string("Codex")])])
        XCTAssertEqual(holds.hold(on: key)?.agent, "Codex", "Codex moved first, and Claude after it")
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "order": .array([.string("Codex")]),
                                               "blocks": .object(["api:DeepSeek": hidden, "Claude": ring])])
        XCTAssertEqual(holds.hold(on: key)?.agent, "Claude")
    }

    /// A block hidden, or with every row hidden, touches its own agent, not the agents after it that move up into its
    /// place: a block's place is the one it has among the blocks shown both before and after the edit. Showing it again
    /// touches it too.
    func testHidingABlockTouchesItsOwnAgentNotTheOnesAfterIt() throws {
        let key = second.key
        let first = try XCTUnwrap(blocks(try call(MCPTools.getHUD, ["screen": .string(key)])).first)
        XCTAssertEqual(first["key"], .string("Claude"), "the sample's first block")

        _ = try call(MCPTools.draftHUD, ["screen": .string(key),
                                               "blocks": .object(["Claude": .object(["hidden": .bool(true)])])])
        XCTAssertEqual(holds.hold(on: key)?.agent, "Claude", "Codex moved up into its place, and is not touched")
        _ = try call(MCPTools.draftHUD, ["screen": .string(key), "order": .array([])])
        XCTAssertEqual(holds.hold(on: key)?.agent, "Claude", "shown again")

        let rows = (first["rows"]?.arrayValue ?? []).compactMap { $0["key"] }
        XCTAssertGreaterThan(rows.count, 1)
        let panel = try call(MCPTools.draftHUD, ["screen": .string(key),
                                                       "blocks": .object(["Claude": .object(["hiddenRows": .array(rows)])])])
        XCTAssertEqual(try block("Claude", in: panel)["shown"], .bool(false), "every row hidden leaves the block out")
        XCTAssertEqual(holds.hold(on: key)?.agent, "Claude")
    }

    /// A call for every screen with a hud that sets nothing, which is all a hud for the shared layout can be beside no
    /// layout, holds no panel; where an edit of the shared layout holds one, it closes it for now, as a style alone does
    /// for a screen.
    func testAStyleAloneForEveryScreenHoldsNoPanel() throws {
        for hud: JSONValue in [.object(["reset": .bool(false)]), .object(["glow": .object([:])])] {
            let panel = try call(MCPTools.draftHUD, ["screen": .string("all"), "hud": hud])
            XCTAssertTrue(held.isEmpty, "\(hud) opens no panel")
            XCTAssertEqual(panel["note"]?.stringValue?.hasSuffix(
                "The shared layout is unchanged: a call with only hud style fields leaves the layout as it is."), true)
        }
        try edit(nil)
        XCTAssertEqual(held, [second.key])
        now += 30
        _ = try call(MCPTools.draftHUD, ["hud": .object(["reset": .bool(false)])])
        XCTAssertEqual(holds.hold(on: second.key)?.paused, true, "the panel an edit of the shared layout holds closes")
        XCTAssertEqual(holds.remaining(on: second.key), 90, "not renewed")
        XCTAssertEqual(changes, 0)
    }

    /// A person's choice about a layout ends the holds on the panels it reaches: Save and Discard in a panel's bar, and
    /// Settings' Revert to Previous, Reset to Default, Use Shared Layout and the HUD style's Reset to Default; so does a
    /// person's own change of that display's placement or glow in Settings. The shared draft's Save or Discard from any
    /// screen's bar ends the holds of the screens that draw it, and leaves a screen with a layout of its own held.
    func testAPersonsChoiceEndsTheHold() throws {
        let key = second.key
        let choices: [(String, () -> Void)] = [
            ("Save", { self.layout.savePanel(on: key) }),
            ("Discard", { self.layout.discardPanel(on: key) }),
            ("Revert to Previous", { self.layout.revert(.screen(key)) }),
            ("Reset to Default", { self.layout.reset(.screen(key)) }),
            ("Use Shared Layout", { self.layout.useShared(key) }),
            ("HUD style Reset to Default", { self.layout.resetLook(key) }),
            ("the display's settings", { self.settings.updateScreen(key) { $0.screens[key] = ScreenPlacement(mode: .simple) } }),
        ]
        for (index, (name, choose)) in choices.enumerated() {
            try edit(key, ["Claude": .object(["widget": .string(["ring", "number", "chart"][index % 3])])])
            XCTAssertEqual(held, [key], name)
            XCTAssertTrue(layout.hasDraft(on: key), "\(name): the bar shows")
            choose()
            XCTAssertTrue(held.isEmpty, name)
        }

        layout.useShared(key)
        for (name, choose) in [("Save", { self.layout.savePanel(on: self.builtIn.key) }),
                               ("Discard", { self.layout.discardPanel(on: self.builtIn.key) })] {
            try edit(nil, ["Codex": .object(["widget": .string(name == "Save" ? "number" : "ring")])])
            XCTAssertEqual(held, [key], name)
            choose()
            XCTAssertTrue(held.isEmpty, "\(name): the shared draft's, from another screen's bar, ends the hold of a screen "
                          + "that draws it")
        }
        try edit(builtIn.key, ["Claude": ring])
        try edit(nil, ["Codex": .object(["widget": .string("chart")])])
        XCTAssertEqual(held, [builtIn.key, key])
        layout.savePanel(on: key)
        XCTAssertEqual(held, [builtIn.key], "a screen with a draft of its own stays held")
    }
}
