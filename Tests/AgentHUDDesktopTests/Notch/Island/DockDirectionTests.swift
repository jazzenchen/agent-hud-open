import AppKit
import AgentHUDCore
import QuartzCore
import XCTest
@testable import AgentHUDDesktop

/// The director's word reaching the marks: the dock and the Simple panel are told each mark's state and its newest event,
/// a card's attention goes on top, and with motion reduced or bots off a mark wears its sign still.
@MainActor
final class DockDirectionTests: XCTestCase {
    private func queue(_ items: [LogoQueueItem], bots: Bool = true) -> LogoQueueConfig {
        var settings = Settings()
        settings.botLogos = bots
        return LogoQueueConfig(items: items, placement: ScreenPlacement(mode: .logos), settings: settings)
    }

    private func hosted() -> (LogoQueueLayerView, NSWindow) {
        let frame = CGRect(x: 0, y: 0, width: 240, height: 48)
        let window = SeenWindow(contentRect: frame, styleMask: .borderless, backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        let view = LogoQueueLayerView(frame: frame)
        window.contentView?.addSubview(view)
        return (view, window)
    }

    private func store(_ settings: SettingsStore) -> UsageStore {
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours,
                                                       now: Date()))
        return store
    }

    private func settings(_ placement: ScreenPlacement? = nil, key: String = "") throws -> SettingsStore {
        let domain = "app.agenthud.tests.direction.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        addTeardownBlock { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
        if let placement { settings.update { $0.screens[key] = placement } }
        return settings
    }

    func testEveryMarkIsToldWhatTheDirectorSaysAndAMarkWithoutAWordFollowsItsWork() throws {
        let store = store(try settings())
        let vendors = LogoQueueItem.queue(rows: store.queueVendors)
        XCTAssertGreaterThan(vendors.count, 2)
        let first = AgentArtwork.markKey(vendors[0].vendor)
        let cue = BotCue(event: .failed, at: Date())
        let said = [first: MarkDirector.Direction(state: BotState(look: .question, badge: .failed), cue: cue)]
        let agents = ScreenAgents(store: store, listed: [], mode: .logos, directions: said)
        XCTAssertEqual(agents.queueItems[0].state, BotState(look: .question, badge: .failed))
        XCTAssertEqual(agents.queueItems[0].cue, cue)
        for item in agents.queueItems.dropFirst() {
            XCTAssertEqual(item.state, BotState(look: item.isWorking ? .plain : .idle), item.vendor)
            XCTAssertNil(item.cue)
        }
        let meter = try XCTUnwrap(agents.meterItems.first { AgentArtwork.markKey($0.vendor) == first })
        XCTAssertEqual(meter.state, BotState(look: .question, badge: .failed), "the Simple panel is told the same")
        XCTAssertEqual(meter.cue, cue)

        // The director the island keeps, read from the store, is what every mark is told.
        let directions = MarkDirections(startedAt: Date(), clock: { Date() })
        directions.read(store.queueMarks)
        let told = ScreenAgents(store: store, listed: [], mode: .logos, directions: directions.current).queueItems
        for item in told {
            XCTAssertEqual(item.state, directions.current[AgentArtwork.markKey(item.vendor)]?.state, item.vendor)
        }
    }

    func testACardsAttentionGoesOnTopOfTheLook() {
        let working = BotState(look: .tool(.edit), badge: .limited)
        XCTAssertEqual(MarkAttention.none.state(of: working), working)
        XCTAssertEqual(MarkAttention.opened.state(of: working), BotState(look: .tool(.edit), badge: .limited, hovered: true),
                       "the pointer goes on top of the look; it no longer pins the mark to an idle bot")
        XCTAssertEqual(MarkAttention.facesCard(.up).state(of: BotState(look: .plan)), BotState(look: .plan, facing: .up))
        var item = LogoQueueItem(vendor: "Claude", isWorking: true)
        XCTAssertEqual(item.state, BotState(look: .plain), "with no director, plain work")
        item.attention = .opened
        XCTAssertEqual(item.shown, BotState(look: .plain, hovered: true))
    }

    func testTheDockActsOutEachMarksStateAndPlaysAFreshEventOnce() throws {
        let (view, window) = hosted()
        defer { BotClock.shared.drop(view); window.close() }
        var thinking = LogoQueueItem(vendor: "Grok Bot", isWorking: true, state: BotState(look: .thinking))
        var waiting = LogoQueueItem(vendor: "Codex", isWorking: true, state: BotState(look: .question))
        waiting.attention = .opened
        view.apply(config: queue([thinking, waiting]), light: false, scale: 2, animates: true)
        XCTAssertEqual(view.actors[0]?.state, BotState(look: .thinking))
        XCTAssertEqual(view.actors[1]?.state, BotState(look: .question, hovered: true))

        thinking.cue = BotCue(event: .failed, at: Date())
        view.apply(config: queue([thinking, waiting]), light: false, scale: 2, animates: true)
        let actor = try XCTUnwrap(view.actors[0])
        var now = CACurrentMediaTime(), saw = false
        for _ in 0..<40 {
            now += 1.0 / 24
            _ = view.stepBots(to: now)
            saw = saw || actor.playing == .failed
        }
        XCTAssertTrue(saw, "a fresh event plays")
        // Told again, or told late, it does not.
        view.apply(config: queue([thinking, waiting], bots: true), light: true, scale: 2, animates: true)
        for _ in 0..<60 {
            now += 1.0 / 24
            _ = view.stepBots(to: now)
            XCTAssertNil(view.actors[0]?.playing, "a mark built again never plays an event twice")
        }
        thinking.cue = BotCue(event: .limitHit, at: Date().addingTimeInterval(-10))
        view.apply(config: queue([thinking, waiting]), light: true, scale: 2, animates: true)
        for _ in 0..<60 {
            now += 1.0 / 24
            _ = view.stepBots(to: now)
            XCTAssertNil(view.actors[0]?.playing, "an event heard of too late is let go")
        }
    }

    func testWithMotionReducedOrBotsOffAMarkWearsItsSignStill() throws {
        let (view, window) = hosted()
        defer { BotClock.shared.drop(view); window.close() }
        let items = [LogoQueueItem(vendor: "Claude", isWorking: true, state: BotState(look: .permission)),
                     LogoQueueItem(vendor: "Codex", isWorking: false, state: BotState(look: .idle, badge: .failed)),
                     LogoQueueItem(vendor: "Cursor", isWorking: true, state: BotState(look: .thinking))]
        for (bots, animates) in [(true, false), (false, true)] {
            view.apply(config: queue(items, bots: bots), light: false, scale: 2, animates: animates)
            let request = try XCTUnwrap(view.signs[0], "a waiting request's !")
            XCTAssertNotNil(view.signs[1], "a badge's ×")
            XCTAssertNil(view.signs[2], "thinking's dots belong to a moving bot")
            XCTAssertNotNil(request.contents)
            XCTAssertTrue(request.actions?["contents"] is NSNull && request.actions?["position"] is NSNull, "never animated")
            XCTAssertNil(request.animationKeys())
            // It stands over its own mark, where the bot's would.
            XCTAssertGreaterThan(request.frame.midX, view.marks[0].frame.midX)
            XCTAssertLessThan(request.frame.minY, view.marks[0].frame.midY)
            XCTAssertLessThan(request.frame.maxX, view.marks[1].frame.minX)
        }
        // With bots out, a bot carries its own sign.
        view.apply(config: queue(items), light: false, scale: 2, animates: true)
        XCTAssertTrue(view.signs.allSatisfy { $0 == nil })
        XCTAssertEqual(view.layer?.sublayers?.filter { layer in
            !view.marks.contains(layer) && !view.actors.contains { $0?.layer === layer }
        }.count, 0)
    }

    func testEveryDockStaysUpBesideARequestsCardAndTheSendersMarkTurnsToIt() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        let request = PermissionRequest(id: "direction", source: .claude, sessionID: "direction", toolName: "Bash",
                                        summary: "ls", detail: "ls", cwd: "/tmp", at: Date())
        for placement in [ScreenPlacement(mode: .logos, edge: .right, separatesAgents: true),
                          ScreenPlacement(mode: .logos, edge: .bottom)] {
            let name = "\(placement.edge) separated: \(placement.separatesAgents)"
            let settings = try settings(placement, key: key)
            let store = store(settings)
            let directions = MarkDirections(startedAt: Date(), clock: { Date() })
            directions.read(store.queueMarks)
            let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings,
                                mouseLocation: { CGPoint(x: -10_000, y: -10_000) }, directions: directions)
            defer { hud.close() }
            let first = try XCTUnwrap(hud.island.rootView.logoQueue?.items.first)
            XCTAssertEqual(first.state, directions.current[AgentArtwork.markKey(first.vendor)]?.state, "\(name): the dock is fed")

            hud.present(.permission(request))
            XCTAssertEqual(hud.island.rootView.focusVendor.map(AgentArtwork.markKey), "Claude",
                           "\(name): the card comes out of its sender's mark")
            let marks = try XCTUnwrap(hud.island.rootView.logoQueue?.items, "\(name): the dock stays up")
            let claude = try XCTUnwrap(marks.first { AgentArtwork.markKey($0.vendor) == "Claude" })
            XCTAssertEqual(claude.attention, .facesCard(BotFacing(cardFrom: placement.edge)), name)
            if placement.edge == .right {
                XCTAssertEqual(claude.attention, .facesCard(.left), "the card opens on the dock's inner side, to the left")
            }
            XCTAssertTrue(marks.filter { $0.id != claude.id }.allSatisfy { $0.attention == .none }, name)
            hud.withdraw(requestID: request.id)
            XCTAssertNil(hud.island.rootView.alert, name)
            XCTAssertNil(hud.island.rootView.focusVendor, name)
        }
    }
}
