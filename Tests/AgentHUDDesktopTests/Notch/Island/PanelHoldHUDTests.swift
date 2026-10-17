import AgentHUDSupport
import AppKit
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// A screen's panel under an agent's hold, through the tools as an agent calls them, beside the notch, on a Dynamic dock
/// and as a Simple panel: it opens without reading the accounts and stays open with the pointer away; it closes when the
/// hold runs out, when the agent finishes, on Save and on Discard, once the pointer has come onto it and left, on a click
/// off it and on Escape; a call that restyles the HUD alone closes it until the next layout edit; it keeps a closing
/// panel open and waits for an event's card; and on a dock that separates its agents it opens the whole card of the
/// agent the edit touched, whose shadow waits for it as a hovered card's does, until the person opens another. The
/// holds run on a clock the tests move. The checks read whether the panel is open, not its frames, so they hold with
/// Reduce Motion on, and the dock's card is checked both ways whatever this Mac is set to.
@MainActor
final class PanelHoldHUDTests: XCTestCase {
    @MainActor
    private struct Fixture {
        let settings: SettingsStore
        let key: String
        let hud: ScreenHUD
        let tools: HUDTools
        let reads: ReadCount
        let pointer: Pointer
        let screen: NSScreen
        let clock: HoldClock

        var holds: PanelHolds { settings.panelLayout.holds }
        var isOpen: Bool { hud.island.rootView.isOpen }

        @discardableResult
        func call(_ tool: String, _ arguments: [String: JSONValue] = [:], line: UInt = #line) throws -> [String: JSONValue] {
            var arguments = arguments
            if arguments["screen"] == nil { arguments["screen"] = .string(key) }
            guard case .result(let result, _) = tools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
                XCTFail("\(tool) was refused", line: line)
                throw CancellationError()
            }
            return result
        }

        /// An edit of the screen's own layout: Codex's block first, or the blocks as `blocks` sets them.
        func edit(_ blocks: [String: JSONValue] = [:], line: UInt = #line) throws {
            try call(MCPTools.draftHUD, blocks.isEmpty ? ["order": .array([.string("Codex")])] : ["blocks": .object(blocks)],
                     line: line)
        }

        /// Takes the HUD off screen, and lets go of the holds and of their waits.
        func close() {
            hud.close()
            holds.endAll()
            clock.finish()
        }

        /// Puts the pointer far from the HUD, or inside the open panel, and lets the HUD look at it.
        func point(inside: Bool) {
            let frame = hud.hoverRegion
            pointer.point = inside ? CGPoint(x: frame.midX, y: frame.midY)
                : CGPoint(x: screen.frame.minX + 20, y: screen.frame.minY + 20)
            hud.samplePointer()
        }
    }

    /// The pointer the HUD reads.
    private final class Pointer {
        var point = CGPoint(x: -10_000, y: -10_000)
    }

    /// The holds' clock, which the test moves, and their waits, which end when it gets there.
    @MainActor
    private final class HoldClock {
        var now = Date(timeIntervalSince1970: 1_800_000_000)
        private var waits: [(until: Date, resume: CheckedContinuation<Void, Never>)] = []

        func drive(_ holds: PanelHolds) {
            holds.clock = { [weak self] in self?.now ?? .distantPast }
            holds.sleep = { [weak self] until in
                guard let self, until > now else { return }
                await withCheckedContinuation { waits.append((until, $0)) }
            }
        }

        func advance(_ seconds: TimeInterval) {
            now += seconds
            let due = waits.filter { $0.until <= now }
            waits.removeAll { $0.until <= now }
            for wait in due { wait.resume.resume() }
        }

        /// Ends every wait, as the test is over.
        func finish() {
            let all = waits
            waits = []
            for wait in all { wait.resume.resume() }
        }
    }

    /// A provider that reads nothing and counts what it is asked: a read of the accounts starts with `fileChanges(nil)`
    /// and goes on to `fetchUsage`.
    private final class ReadCount: UsageProvider, @unchecked Sendable {
        private let lock = NSLock()
        private var counts = (refreshes: 0, fetches: 0)
        var refreshes: Int { read().refreshes }
        var fetches: Int { read().fetches }

        private func read() -> (refreshes: Int, fetches: Int) {
            lock.lock()
            defer { lock.unlock() }
            return counts
        }

        private func count(_ change: (inout (refreshes: Int, fetches: Int)) -> Void) {
            lock.lock()
            change(&counts)
            lock.unlock()
        }

        func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
            count { $0.fetches += 1 }
            return DemoUsageProvider.report(agents: agents, historyHours: historyHours, now: Date(), stage: nil)
        }

        func fileChanges(_ paths: Set<String>?) async { count { $0.refreshes += 1 } }
    }

    /// The demo on the main display in `placement`, over settings of its own, read by a store that may read the accounts
    /// through `ReadCount`, with the tools an agent calls and the holds on a clock the test moves. The HUD sees Reduce
    /// Motion as `reduceMotion` says, whatever this Mac is set to. The caller closes the fixture.
    private func make(_ placement: ScreenPlacement, agents: [AgentDescriptor] = DemoData.agents, collapseDelayMs: Int = 0,
                      reduceMotion: Bool = false) throws -> Fixture {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        let domain = "app.agenthud.tests.panel-hold.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        addTeardownBlock { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: agents)
        settings.update {
            $0.screens[key] = placement
            $0.requiresOptionToOpen = false
            $0.hoverDelayMs = 0
            $0.collapseDelayMs = collapseDelayMs
        }
        settings.panelLayout.setSwitches { $0.tokenUsage = false; $0.sessionList = false }
        let reads = ReadCount()
        let store = UsageStore(provider: reads, settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        let clock = HoldClock()
        clock.drive(settings.panelLayout.holds)
        let pointer = Pointer()
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings, mouseLocation: { pointer.point },
                            reduceMotion: { reduceMotion })
        let tools = HUDTools(store: store, screens: {
            [PanelScreen(key: key, name: "Main display", mode: placement.mode, edge: placement.edge,
                         hasNotch: ScreenIdentity.hasNotch(screen), agents: [], systemIsLight: false,
                         reducesTransparency: false, hasPointer: true)]
        })
        let fixture = Fixture(settings: settings, key: key, hud: hud, tools: tools, reads: reads, pointer: pointer,
                              screen: screen, clock: clock)
        fixture.point(inside: false)
        return fixture
    }

    /// Beside the notch, on a Dynamic dock and as a Simple panel, none of them separating its agents.
    private let modes = [ScreenPlacement(mode: .notch), ScreenPlacement(mode: .logos, edge: .top),
                         ScreenPlacement(mode: .simple, edge: .right)]

    /// Waits for the main run loop to bring `condition` about, for up to five seconds.
    private func poll(_ what: String, line: UInt = #line, until condition: () -> Bool) {
        let deadline = Date().addingTimeInterval(5)
        while !condition(), Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(condition(), what, line: line)
    }

    /// Lets the main run loop turn for a moment, for a check that something has not happened.
    private func turn() { RunLoop.main.run(until: Date().addingTimeInterval(0.1)) }

    private let ring: JSONValue = .object(["widget": .string("ring")])

    // MARK: Opening and staying open

    /// An edit opens the panel and the hold keeps it open with the pointer away, without reading the accounts, in every
    /// mode. The proof that nothing read is the hover's own opening afterwards: the provider counts its read, and only
    /// its read, though the hold's opening came first.
    func testAHeldPanelOpensWithoutReadingAndStaysOpenWithThePointerAway() throws {
        for placement in modes {
            let name = "\(placement.mode)"
            let f = try make(placement)
            defer { f.close() }
            XCTAssertFalse(f.isOpen, name)
            try f.edit()
            poll("\(name): the hold opens the panel") { f.isOpen }
            XCTAssertTrue(f.hud.isHeldOpen, name)
            for _ in 0..<5 {
                f.point(inside: false)
                RunLoop.main.run(until: Date().addingTimeInterval(0.02))
            }
            XCTAssertTrue(f.isOpen, "\(name): the pointer away does not close a held panel")
            try f.edit(["Claude": ring])
            turn()
            XCTAssertTrue(f.isOpen, name)
            XCTAssertEqual(f.reads.refreshes, 0, name)
            XCTAssertEqual(f.reads.fetches, 0, name)

            try f.call(MCPTools.finishHUDEdit)
            poll("\(name): finishing closes it") { !f.isOpen }
            f.hud.forceOpen()
            poll("\(name): the hover's opening reads the accounts") { f.reads.fetches > 0 }
            XCTAssertEqual(f.reads.refreshes, 1, "\(name): one read, the hover's, and none from the hold's opening before it")
        }
    }

    /// The hold runs out by itself two minutes after the last edit, and the panel closes; finish_hud_edit closes it at
    /// once.
    func testAHeldPanelClosesWhenTheHoldRunsOutOrTheAgentFinishes() throws {
        let f = try make(ScreenPlacement(mode: .notch))
        defer { f.close() }
        try f.edit()
        poll("the hold opens the panel") { f.isOpen }
        f.clock.advance(119)
        turn()
        XCTAssertTrue(f.isOpen, "not yet two minutes")
        f.clock.advance(1)
        poll("two minutes without an edit close it") { !f.isOpen }
        XCTAssertNil(f.holds.hold(on: f.key))
        XCTAssertFalse(f.hud.isHeldOpen)

        try f.edit(["Claude": ring])
        poll("the next edit holds it again") { f.isOpen }
        try f.call(MCPTools.finishHUDEdit, ["screen": .string("all")])
        poll("finishing closes it") { !f.isOpen }
        XCTAssertNil(f.holds.hold(on: f.key))
        XCTAssertEqual(f.reads.refreshes, 0)
    }

    /// A panel the hover was closing, the pointer having left it, stays open once an edit holds it: the hold takes the
    /// panel over from the hover's countdown.
    func testAHoldKeepsOpenAPanelTheHoverWasClosing() throws {
        let f = try make(ScreenPlacement(mode: .notch), collapseDelayMs: 5_000)
        defer { f.close() }
        f.hud.forceOpen()
        f.point(inside: true)
        f.point(inside: false)
        guard case .closing = f.hud.machine.state else { return XCTFail("the hover is closing the panel") }
        try f.edit()
        poll("the hold takes the panel over") { f.hud.isHeldOpen }
        XCTAssertEqual(f.hud.machine.state, .open, "no countdown is left to close it")
    }

    /// An event's card on show keeps a hold from opening the panel, which would hide the card behind it; once the card
    /// goes, the hold opens the panel, still without a read.
    func testAHoldWaitsForAnEventsCard() throws {
        let f = try make(ScreenPlacement(mode: .notch))
        defer { f.close() }
        f.hud.present(.completion(SessionCompletion(sessionID: "s", vendor: "Claude", turnID: "t", task: "Build the dashboard",
                                                     model: "opus", startedAt: nil, completedAt: Date())))
        try f.edit()
        turn()
        XCTAssertFalse(f.isOpen, "the card stays as it is")
        XCTAssertNotNil(f.holds.hold(on: f.key))
        f.hud.dismissCompletionAlert()
        poll("the hold opens the panel once the card has gone") { f.hud.isHeldOpen }
        XCTAssertEqual(f.reads.refreshes, 0)
    }

    // MARK: Ending

    /// Save and Discard end the hold at once: with the pointer away the panel closes, and under the pointer that pressed
    /// the button it stays open as a hovered panel until the pointer leaves.
    func testSaveAndDiscardEndTheHold() throws {
        let f = try make(ScreenPlacement(mode: .notch))
        defer { f.close() }
        let layout = f.settings.panelLayout
        let presses: [(String, () -> Void)] = [("Save", { layout.savePanel(on: f.key) }),
                                               ("Discard", { layout.discardPanel(on: f.key) })]
        for (name, press) in presses {
            try f.edit(["Claude": .object(["widget": .string(name == "Save" ? "ring" : "number")])])
            poll("\(name): the hold opens the panel") { f.isOpen }
            press()
            XCTAssertNil(f.holds.hold(on: f.key), name)
            poll("\(name) with the pointer away closes the panel") { !f.isOpen }

            try f.edit(["Claude": .object(["widget": .string("chart")])])
            poll("\(name): the next edit holds it again") { f.isOpen }
            f.point(inside: true)
            press()
            poll("\(name): the panel lets go of the hold") { !f.hud.isHeldOpen }
            XCTAssertTrue(f.isOpen, "\(name): the pointer that pressed it keeps it open")
            f.point(inside: false)
            poll("\(name): leaving closes it as a hovered panel") { !f.isOpen }
        }
    }

    /// The pointer coming onto the held panel and leaving it is the person taking over: the hold ends, and the panel
    /// closes as a hovered panel does.
    func testThePointerComingAndGoingEndsTheHold() throws {
        for placement in modes {
            let name = "\(placement.mode)"
            let f = try make(placement)
            defer { f.close() }
            try f.edit()
            poll("\(name): the hold opens the panel") { f.isOpen }
            f.point(inside: true)
            XCTAssertNotNil(f.holds.hold(on: f.key), "\(name): the pointer on the panel keeps the hold")
            XCTAssertTrue(f.isOpen, name)
            f.point(inside: false)
            XCTAssertNil(f.holds.hold(on: f.key), "\(name): leaving ends it")
            poll("\(name): and the panel closes") { !f.isOpen }
        }
    }

    /// A held panel that opens under a pointer at rest is not the pointer coming onto it, as a person typing to the agent
    /// may have left the pointer where the panel opens: the pointer moving off it again leaves the hold on and the panel
    /// open, and only the pointer coming back onto it and leaving takes over. The same holds when the panel opens again
    /// under the pointer after a style call closed it. A panel the pointer was on before the hold took it over is the
    /// person's already, and the pointer leaving it takes over.
    func testAPanelThatOpensUnderAPointerAtRestKeepsTheHold() throws {
        for placement in modes {
            let name = "\(placement.mode)"
            let f = try make(placement)
            defer { f.close() }
            // Where the held panel opens, measured on a first hold and let go of before the pointer goes there.
            try f.edit()
            poll("\(name): the hold opens the panel") { f.isOpen }
            let panel = f.hud.hoverRegion
            try f.call(MCPTools.finishHUDEdit)
            poll("\(name): finishing closes it") { !f.isOpen }
            let resting = CGPoint(x: panel.midX, y: panel.midY)
            f.pointer.point = resting
            f.hud.samplePointer()
            XCTAssertFalse(f.hud.pointerInside, "\(name): the pointer rests off the HUD at rest")

            try f.edit()
            poll("\(name): the hold opens the panel under the pointer") { f.isOpen }
            // As the island's tracking area does when the window grows under the pointer.
            f.hud.samplePointer()
            XCTAssertTrue(f.hud.pointerInside, "\(name): the panel is under the pointer")
            f.point(inside: false)
            XCTAssertNotNil(f.holds.hold(on: f.key), "\(name): the pointer moving off the panel leaves the hold on")
            turn()
            XCTAssertTrue(f.hud.isHeldOpen, "\(name): and the panel open")

            try f.call(MCPTools.draftHUD, ["hud": .object(["transparency": .number(0.2)])])
            poll("\(name): the style closes it") { !f.isOpen }
            f.pointer.point = resting
            f.hud.samplePointer()
            try f.edit(["Claude": ring])
            poll("\(name): the next edit opens it under the pointer again") { f.isOpen }
            f.hud.samplePointer()
            f.point(inside: false)
            XCTAssertNotNil(f.holds.hold(on: f.key), "\(name): the pointer moving off it again leaves the hold on")

            f.point(inside: true)
            f.point(inside: false)
            XCTAssertNil(f.holds.hold(on: f.key), "\(name): coming onto it and leaving takes over")
            poll("\(name): and the panel closes") { !f.isOpen }

            f.hud.forceOpen()
            f.point(inside: true)
            try f.edit(["Claude": .object(["widget": .string("number")])])
            poll("\(name): the hold takes the hovered panel over") { f.hud.isHeldOpen }
            f.point(inside: false)
            XCTAssertNil(f.holds.hold(on: f.key), "\(name): the pointer leaving a panel it was on takes over")
            poll("\(name): and the panel closes as a hovered one") { !f.isOpen }
        }
    }

    /// A click off the held panel is the person at work elsewhere, which ends the hold and closes the panel; a click on
    /// it is the panel's own and leaves the hold. Escape, while Agent HUD has the keyboard, ends every hold.
    func testAClickOffThePanelOrEscapeEndsTheHold() throws {
        let f = try make(ScreenPlacement(mode: .logos, edge: .top))
        defer { f.close() }
        try f.edit()
        poll("the hold opens the panel") { f.isOpen }
        let region = f.hud.hoverRegion
        f.pointer.point = CGPoint(x: region.midX, y: region.midY)
        f.hud.personClicked()
        XCTAssertNotNil(f.holds.hold(on: f.key), "a click on the panel is the panel's own")
        f.pointer.point = CGPoint(x: f.screen.frame.minX + 20, y: f.screen.frame.minY + 20)
        f.hud.personClicked()
        XCTAssertNil(f.holds.hold(on: f.key), "a click elsewhere ends the hold")
        poll("and the panel closes") { !f.isOpen }
        f.hud.personClicked()
        XCTAssertNil(f.holds.hold(on: f.key), "with nothing held a click changes nothing")

        let controller = IslandController(store: UsageStore(provider: DemoUsageProvider(), settings: f.settings,
                                                            accessAllowed: { false }), settings: f.settings)
        defer { controller.rebuild(keys: [], screens: []) }
        try f.edit(["Claude": ring])
        XCTAssertNotNil(f.holds.hold(on: f.key))
        controller.personPressedEscape()
        XCTAssertNil(f.holds.hold(on: f.key), "Escape ends the hold of every screen")
        poll("and the panel closes") { !f.isOpen }
    }

    /// A call that restyles the HUD alone closes the held panel so the HUD shows; the next layout edit opens it again.
    func testAStyleAloneClosesTheHeldPanelUntilTheNextLayoutEdit() throws {
        let f = try make(ScreenPlacement(mode: .notch))
        defer { f.close() }
        try f.call(MCPTools.draftHUD, ["hud": .object(["glow": .object(["brightness": .number(0.5)])])])
        turn()
        XCTAssertFalse(f.isOpen, "a style alone opens nothing")

        try f.edit()
        poll("the hold opens the panel") { f.isOpen }
        try f.call(MCPTools.draftHUD, ["hud": .object(["glow": .object(["brightness": .number(0.6)])])])
        poll("the style closes it, so the HUD shows") { !f.isOpen }
        XCTAssertEqual(f.holds.hold(on: f.key)?.paused, true)
        try f.edit(["Claude": ring])
        poll("the next layout edit opens it again") { f.isOpen }
        XCTAssertTrue(f.hud.isHeldOpen)
        XCTAssertEqual(f.reads.refreshes, 0, "neither opening read the accounts")
    }

    // MARK: A dock that separates its agents

    /// On a dock that separates its agents the hold opens the whole card of the agent the edit touched first, and where
    /// it touched none, the card of the first agent the dock shows; it opens on the Dynamic dock and the Simple panel.
    func testASeparatedDockOpensTheCardOfTheAgentTheEditTouched() throws {
        for mode in [HUDMode.logos, .simple] {
            let name = "\(mode)"
            let f = try make(ScreenPlacement(mode: mode, edge: .top, separatesAgents: true), agents: DemoData.everyAgent)
            defer { f.close() }
            let root = { f.hud.island.rootView }
            let first = try XCTUnwrap(root().logoQueue?.items.first?.vendor ?? root().simpleMeter?.items.first?.vendor, name)
            XCTAssertNotEqual(AgentArtwork.markKey(first), AgentArtwork.markKey("Codex"), "\(name): the sample's first mark is another agent")

            let blocks: JSONValue = .object(["Codex": ring])
            try f.call(MCPTools.draftHUD, ["blocks": blocks])
            poll("\(name): the hold opens a card") { f.isOpen }
            XCTAssertEqual(root().focusVendor.map(AgentArtwork.markKey), AgentArtwork.markKey("Codex"), name)
            XCTAssertTrue(root().pinned, "\(name): the agent's whole card, not a peek")

            try f.call(MCPTools.finishHUDEdit)
            poll("\(name): finishing closes it") { !f.isOpen }
            // The same blocks, so no agent's block changes: only the spacing.
            try f.call(MCPTools.draftHUD, ["blocks": blocks, "style": .object(["spacing": .string("compact")])])
            poll("\(name): an edit that touches no agent opens a card") { f.isOpen }
            XCTAssertEqual(root().focusVendor.map(AgentArtwork.markKey), AgentArtwork.markKey(first), name)
            XCTAssertTrue(root().pinned, name)
            XCTAssertEqual(f.reads.refreshes, 0, name)
        }
    }

    /// The card a hold opens is a card like a hovered one: its shadow waits until it has grown out of its dock and then
    /// shows, and the panel beside it keeps its own meanwhile; under Reduce Motion it is there at once. Both ways run on
    /// any Mac. Closing it takes its shadow at once.
    func testAHeldCardsShadowWaitsForTheCardAsAHoveredOnesDoes() throws {
        for reduceMotion in [false, true] {
            let name = reduceMotion ? "Reduce Motion" : "motion"
            let placement = ScreenPlacement(mode: .simple, edge: .right, separatesAgents: true, appearance: .light)
            let f = try make(placement, agents: DemoData.everyAgent, reduceMotion: reduceMotion)
            defer { f.close() }
            XCTAssertNil(f.hud.glowShells?.surface, "\(name): the dock alone")
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))

            let opened = Date()
            try f.edit(["Codex": ring])
            poll("\(name): the hold opens the card") { f.isOpen }
            let card = try XCTUnwrap(f.hud.island.rootView.cardScreenFrame, name)
            XCTAssertNotNil(f.hud.glowShells?.dock, "\(name): the panel keeps its shadow")
            if reduceMotion {
                XCTAssertEqual(f.hud.glowShells?.surface?.frame, card, "\(name): a card placed at once has its shadow at once")
            } else {
                XCTAssertNil(f.hud.glowShells?.surface, "\(name): no shadow while the card grows")
                poll("\(name): the shadow comes once the card has arrived") { f.hud.glowShells?.surface != nil }
                XCTAssertGreaterThan(Date().timeIntervalSince(opened), IslandAnimation.duration - 0.05, name)
                XCTAssertEqual(f.hud.glowShells?.surface?.frame, f.hud.island.rootView.cardScreenFrame, "\(name): under the card")
            }

            try f.call(MCPTools.finishHUDEdit)
            poll("\(name): finishing closes the card") { !f.isOpen }
            XCTAssertNil(f.hud.glowShells?.surface, "\(name): gone with the card")
            XCTAssertNotNil(f.hud.glowShells?.dock, name)
        }
    }

    /// Opening another agent's card is the person taking over: the pointer on another mark ends the hold and shows that
    /// agent's card, which then closes as a hovered one does once the pointer leaves. A click on a mark does the same.
    func testOpeningAnotherCardEndsTheHold() throws {
        let f = try make(ScreenPlacement(mode: .logos, edge: .top, separatesAgents: true), agents: DemoData.everyAgent)
        defer { f.close() }
        try f.edit(["Codex": ring])
        poll("the hold opens Codex's card") { f.isOpen }
        let strip = f.hud.geometry.rect
        let codex = try XCTUnwrap(f.hud.agentMarks.first { $0.key == AgentArtwork.markKey("Codex") })
        let other = try XCTUnwrap(f.hud.agentMarks.first { $0.key != codex.key })
        f.pointer.point = CGPoint(x: codex.anchor, y: strip.midY)
        f.hud.samplePointer()
        XCTAssertNotNil(f.holds.hold(on: f.key), "the pointer on the card's own mark keeps the hold")
        f.pointer.point = CGPoint(x: other.anchor, y: strip.midY)
        f.hud.samplePointer()
        XCTAssertNil(f.holds.hold(on: f.key), "another agent's mark ends it")
        XCTAssertEqual(f.hud.island.rootView.focusVendor.map(AgentArtwork.markKey), other.key, "and opens that agent's card")
        f.point(inside: false)
        poll("leaving closes it as a hovered card") { !f.isOpen }

        try f.edit(["Codex": .object(["widget": .string("number")])])
        poll("the next edit holds it again") { f.isOpen }
        f.pointer.point = CGPoint(x: codex.anchor, y: strip.midY)
        f.hud.samplePointer()
        f.hud.pinAgent()
        XCTAssertNil(f.holds.hold(on: f.key), "a click on a mark ends it")
        XCTAssertTrue(f.isOpen, "the clicked card stays under the pointer")
    }

    // MARK: Which display

    /// The tools hold the panel of the display the person is at: exactly one attached display says it is.
    func testOneDisplayIsTheOneThePersonIsAt() throws {
        _ = NSApplication.shared
        let domain = "app.agenthud.tests.panel-hold-screens.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, accessAllowed: { false })
        let controller = IslandController(store: store, settings: settings)
        defer { controller.rebuild(keys: [], screens: []) }
        XCTAssertEqual(controller.panelScreens.filter(\.hasPointer).count, NSScreen.screens.isEmpty ? 0 : 1)
    }
}
