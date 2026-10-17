import AppKit
import XCTest
import AgentHUDCore
@testable import AgentHUDDesktop

/// On a Simple dock that separates its agents, the panel's glass and the hover line's are one piece of Liquid Glass
/// (`JoinedGlass`), and a detached card's shadow waits for the card to arrive.
@MainActor
final class JoinedGlassTests: XCTestCase {
    private let strip = CGRect(x: 400, y: 100, width: 35, height: 120)

    private func root(look: HUDLook, separates: Bool = true, simple: Bool = true, open: Bool = true,
                      focus: String? = "Claude", style: AgentPeek.Style = .line, pinned: Bool = false,
                      alert: IslandAlert? = nil, placed: Bool = true) -> IslandRootView {
        var root = IslandRootView(store: nil, isOpen: open, collapsedSize: strip.size,
                                  collapsedTopRadius: 10, collapsedBottomRadius: 10,
                                  look: look, lightBorder: false, onOpenStats: {})
        let items = [SimpleMeterItem(vendor: "Claude", usedPct: 62, level: .ok),
                     SimpleMeterItem(vendor: "Codex", usedPct: 18, level: .ok)]
        if simple {
            root.simpleMeter = SimpleMeterConfig(items: items, placement: ScreenPlacement(mode: .simple, edge: .right))
        } else {
            root.logoQueue = LogoQueueConfig(items: items.map { LogoQueueItem(vendor: $0.vendor, isWorking: false) },
                                             placement: ScreenPlacement(edge: .right), settings: Settings())
        }
        root.separatesAgents = separates
        root.hidesSilhouette = true
        root.edge = .right
        root.alert = alert
        root.focusVendor = focus
        root.pinned = pinned
        if let focus {
            root.peek = AgentPeek(vendor: focus, lines: [], running: 0, style: style)
        }
        if placed { root.logoQueueFrame = strip }
        root.presentationFrame = CGRect(x: 300, y: 140, width: 120, height: 26)
        return root
    }

    private var reply: IslandAlert {
        .completion(SessionCompletion(sessionID: "joined", vendor: "Claude", turnID: "reply", task: "New reply",
                                      model: "test", startedAt: nil, completedAt: Date()))
    }

    /// Liquid Glass, clear or regular, joins the panel and the line; every other way of drawing keeps each surface its
    /// own, as it always was. Checked with glass both Liquid Glass and frosted, whatever this Mac and its build draw.
    func testOnlyLiquidGlassJoinsThePanelAndTheLine() {
        for liquid in [true, false] {
            for appearance in [HUDLook.Appearance.dark, .light] {
                for style in [HUDGlassStyle.clear, .regular] {
                    let glass = HUDLook(appearance: appearance, material: .glass, transparency: 0.5, glassStyle: style)
                    let name = "\(appearance) \(style), \(liquid ? "Liquid Glass" : "frosted")"
                    XCTAssertEqual(root(look: glass).joinsPanelGlass(standIn: false, liquid: liquid), liquid, name)
                    XCTAssertEqual(root(look: glass).cardJoinsPanel(standIn: false, liquid: liquid), liquid, name)
                    XCTAssertFalse(root(look: glass).joinsPanelGlass(standIn: true, liquid: liquid), "\(name): a picture")
                }
                let others = [HUDLook(appearance: appearance), HUDLook(appearance: appearance, transparency: 0.4),
                              HUDLook(appearance: appearance, material: .glass, transparency: 0.5, glassStyle: .material)]
                for look in others {
                    XCTAssertFalse(root(look: look).joinsPanelGlass(standIn: false, liquid: liquid), "\(look)")
                    XCTAssertFalse(root(look: look).cardJoinsPanel(standIn: false, liquid: liquid), "\(look)")
                }
            }
            XCTAssertFalse(root(look: .standard).joinsPanelGlass(standIn: false, liquid: liquid))
        }
        // Unless told otherwise, the glass is what this Mac and its build draw.
        let glass = HUDLook(appearance: .dark, material: .glass, transparency: 0.5)
        XCTAssertEqual(root(look: glass).joinsPanelGlass(standIn: false), HUDGlass.isLiquid)
        XCTAssertEqual(root(look: glass).cardJoinsPanel(standIn: false), HUDGlass.isLiquid)
    }

    /// Only the line flows into the panel. A full card, a reminder or a request sits 6 points off it and keeps a surface
    /// of its own; a dock that does not separate its agents, a Dynamic dock, and the island grown over the panel for an
    /// event from an agent with no mark here draw as they always have.
    func testOnlyTheLineJoinsAndOnlyOnASeparatingSimpleDock() {
        let glass = HUDLook(appearance: .dark, material: .glass, transparency: 0.5)
        for liquid in [true, false] {
            func joins(_ view: IslandRootView) -> Bool { view.joinsPanelGlass(standIn: false, liquid: liquid) }
            func cardJoins(_ view: IslandRootView) -> Bool { view.cardJoinsPanel(standIn: false, liquid: liquid) }
            let glassName = liquid ? "Liquid Glass" : "frosted"
            XCTAssertEqual(joins(root(look: glass)), liquid, "\(glassName): the panel beside the line")
            XCTAssertEqual(cardJoins(root(look: glass)), liquid, "\(glassName): the line")
            XCTAssertEqual(joins(root(look: glass, pinned: true)), liquid, "\(glassName): the panel beside a full card")
            XCTAssertFalse(cardJoins(root(look: glass, pinned: true)), "\(glassName): a full card")
            XCTAssertEqual(joins(root(look: glass, open: false, alert: reply)), liquid, glassName)
            XCTAssertFalse(cardJoins(root(look: glass, open: false, alert: reply)), "\(glassName): a reminder")
            XCTAssertFalse(cardJoins(root(look: glass, alert: reply)), "\(glassName): an event's card")
            XCTAssertEqual(joins(root(look: glass, open: false, focus: nil)), liquid, "\(glassName): the panel alone")
            XCTAssertFalse(cardJoins(root(look: glass, open: false, focus: nil)), glassName)
            XCTAssertFalse(joins(root(look: glass, open: false, focus: nil, alert: reply)),
                           "\(glassName): the island grown for an agent with no mark")
            XCTAssertFalse(joins(root(look: glass, separates: false)), glassName)
            XCTAssertFalse(joins(root(look: glass, simple: false, style: .bubble)), glassName)
            XCTAssertFalse(joins(root(look: glass, placed: false)), "\(glassName): no panel placed yet")
        }
    }

    /// While its glass is drawn with the line's, the panel draws its figures alone.
    func testThePanelDrawsNoSurfaceOfItsOwnWhileItsGlassIsJoined() {
        let config = SimpleMeterConfig(items: SimpleMeterItem.samples, placement: ScreenPlacement(mode: .simple))
        XCTAssertTrue(SimpleMeterView(config: config).drawsSurface, "a preview with no island")
        XCTAssertFalse(SimpleMeterView(config: config, onIsland: true).drawsSurface)
        XCTAssertFalse(SimpleMeterView(config: config, glassJoined: true).drawsSurface)
    }

    // MARK: The card's shadow

    /// A HUD on the main display in `placement`, which sees Reduce Motion as `reduceMotion` says whatever this Mac is set
    /// to, and whose resting HUD shows `agents` alone, or every agent.
    private func hud(_ placement: ScreenPlacement, reduceMotion: Bool = false, agents: [String]? = nil,
                     pointer: @escaping @MainActor () -> CGPoint) throws -> (ScreenHUD, () -> Void) {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        let domain = "app.agenthud.tests.joined.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
        settings.update { $0.screens[key] = placement }
        if let agents {
            settings.panelLayout.propose(.own(PanelLayout(hud: PanelLayout.HUD(agents: agents))), for: key)
            settings.panelLayout.save(.screen(key))
        }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours,
                                                       now: Date()))
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings, mouseLocation: pointer,
                            reduceMotion: { reduceMotion })
        return (hud, { hud.close(); defaults.removePersistentDomain(forName: domain) })
    }

    /// Runs the main run loop until `condition` holds, for up to two seconds.
    private func wait(until condition: () -> Bool) {
        let deadline = Date().addingTimeInterval(2)
        while !condition(), Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
    }

    /// The glow's shadow of the surface, once it is drawn at `opacity`: after the materials, where the glow waits for
    /// them.
    private func surfaceShadow(of hud: ScreenHUD, settlesAt opacity: Float) -> CALayer {
        let layer = hud.glow.shellShadowLayer
        wait { !layer.isHidden && layer.opacity == opacity }
        return layer
    }

    /// The line's shadow, cut out where the line ends up, waits until the line has grown out of the panel, and the
    /// panel's keeps its own shadow meanwhile; then it fades in. Once shown it follows the line to the next agent, and
    /// it goes at once when the line goes back. Under Reduce Motion the line and its shadow come at once. Both ways run
    /// on any Mac, whatever it is set to.
    func testTheCardsShadowWaitsForTheCardToArrive() throws {
        for reduceMotion in [false, true] {
            for edge in [HUDEdge.right, .top] {
                let name = "\(edge)\(reduceMotion ? ", Reduce Motion" : "")"
                var pointer = CGPoint(x: -10_000, y: -10_000)
                let placement = ScreenPlacement(mode: .simple, edge: edge, separatesAgents: true, appearance: .light)
                let (hud, close) = try hud(placement, reduceMotion: reduceMotion) { pointer }
                defer { close() }
                let strip = hud.geometry.rect
                XCTAssertNil(hud.glowShells?.surface, "\(name): the dock alone")
                XCTAssertNotNil(hud.glowShells?.dock, name)
                RunLoop.main.run(until: Date().addingTimeInterval(0.05))

                pointer = edge.isHorizontal ? CGPoint(x: strip.minX + 6, y: strip.midY)
                    : CGPoint(x: strip.midX, y: strip.maxY - 6)
                hud.samplePointer()
                let opened = Date()
                hud.forceOpen()
                let card = try XCTUnwrap(hud.island.rootView.cardScreenFrame, name)
                XCTAssertEqual(hud.island.rootView.peek?.style, .line, name)
                XCTAssertNotNil(hud.glowShells?.dock, "\(name): the panel keeps its shadow")
                if reduceMotion {
                    XCTAssertEqual(hud.glowShells?.surface?.frame, card, "\(name): a line placed at once has its shadow at once")
                    let shadow = surfaceShadow(of: hud, settlesAt: 1)
                    XCTAssertEqual(shadow.opacity, 1, name)
                    XCTAssertNil(shadow.animation(forKey: "opacity"), "\(name): unfaded")
                } else {
                    XCTAssertNil(hud.glowShells?.surface, "\(name): no shadow while the line grows")
                    hud.apply(animated: true)
                    XCTAssertNil(hud.glowShells?.surface, "\(name): nor after another pass")
                    wait { hud.glowShells?.surface != nil }
                    XCTAssertGreaterThan(Date().timeIntervalSince(opened), IslandAnimation.duration - 0.05,
                                         "\(name): the shadow waits for the line")
                    let shadow = hud.glow.shellShadowLayer
                    XCTAssertEqual(shadow.opacity, 1, name)
                    XCTAssertNotNil(shadow.animation(forKey: "opacity"), "\(name): and then fades in")
                }
                RunLoop.main.run(until: Date().addingTimeInterval(IslandAnimation.duration + 0.2))
                XCTAssertEqual(hud.glowShells?.surface?.frame, card, "\(name): the line has arrived")
                hud.apply(animated: true)
                XCTAssertEqual(hud.glowShells?.surface?.frame, card, "\(name): later passes keep it")

                // To the far end of the dock: the shadow follows the line.
                pointer = edge.isHorizontal ? CGPoint(x: strip.maxX - 6, y: strip.midY)
                    : CGPoint(x: strip.midX, y: strip.minY + 6)
                hud.samplePointer()
                let moved = try XCTUnwrap(hud.island.rootView.cardScreenFrame, name)
                XCTAssertNotEqual(moved, card, name)
                XCTAssertEqual(hud.glowShells?.surface?.frame, moved, "\(name): a shown shadow follows its card")
                RunLoop.main.run(until: Date().addingTimeInterval(IslandAnimation.duration + 0.2))

                // Back into the dock: the line's shadow goes at once, never fading round the shrinking line.
                hud.glow.shellShadowLayer.removeAllAnimations()
                hud.forceCollapse()
                XCTAssertNil(hud.glowShells?.surface, "\(name): gone with the line")
                XCTAssertNotNil(hud.glowShells?.dock, name)
                let shadow = surfaceShadow(of: hud, settlesAt: 0)
                XCTAssertEqual(shadow.opacity, 0, name)
                XCTAssertNil(shadow.animation(forKey: "opacity"), "\(name): at once")
            }
        }
    }

    /// An event from an agent with no mark here is a card like any other's, out of the middle of the dock, separated or
    /// not: its shadow waits for it to arrive, the Simple panel keeps its own beside it, and the card's goes at once
    /// when the card goes back, on a Simple dock and a Dynamic one.
    func testACardForAnAgentWithNoMarkComesOutOfTheDocksMiddleWithItsShadow() throws {
        for placement in [ScreenPlacement(mode: .simple, edge: .right, separatesAgents: true, appearance: .light),
                          ScreenPlacement(mode: .simple, edge: .left, appearance: .light),
                          ScreenPlacement(mode: .logos, edge: .bottom, separatesAgents: true, appearance: .light),
                          ScreenPlacement(mode: .logos, edge: .top, appearance: .light)] {
            let name = "\(placement.mode) \(placement.edge)"
            let (hud, close) = try hud(placement, agents: ["Cursor", "Codex"]) { CGPoint(x: -10_000, y: -10_000) }
            defer { close() }
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
            hud.present(.completion(SessionCompletion(sessionID: "markless", vendor: "Claude", turnID: "reply",
                                                      task: "New reply", model: "test", startedAt: nil,
                                                      completedAt: Date())))
            XCTAssertNotNil(hud.island.rootView.alert, name)
            XCTAssertEqual(hud.island.rootView.focusVendor, "Claude", "\(name): out of the dock's middle")
            XCTAssertNil(hud.glowShells?.surface, "\(name): the card's shadow waits for it")
            if placement.mode == .simple { XCTAssertNotNil(hud.glowShells?.dock, "\(name): the panel keeps its shadow") }
            let card = try XCTUnwrap(hud.island.rootView.cardScreenFrame, name)
            let panel = hud.glowShells?.dock
            wait { hud.glowShells?.surface != nil }
            XCTAssertEqual(hud.glowShells?.surface?.frame, card, "\(name): then stands under it")
            RunLoop.main.run(until: Date().addingTimeInterval(IslandAnimation.duration + 0.2))

            for layer in [hud.glow.shellShadowLayer, hud.glow.dockShadowLayer] { layer.removeAllAnimations() }
            hud.dismissCompletionAlert()
            XCTAssertNil(hud.island.rootView.alert, name)
            if placement.mode == .simple && !placement.separatesAgents {
                // The panel is the island again, and takes its own shadow back where it stands.
                XCTAssertEqual(hud.glowShells?.surface, panel, name)
                XCTAssertNil(hud.glowShells?.dock, name)
                let docked = hud.glow.dockShadowLayer
                wait { docked.opacity == 0 }
                XCTAssertNil(docked.animation(forKey: "opacity"), "\(name): never two shadows under the panel")
                let shadow = surfaceShadow(of: hud, settlesAt: 1)
                XCTAssertNil(shadow.animation(forKey: "position"), "\(name): nothing slides in from the card")
                XCTAssertNil(shadow.animation(forKey: "opacity"), name)
            } else {
                XCTAssertTrue(hud.glowShells?.surface?.bare ?? true, "\(name): the card's shadow is gone")
                let shadow = surfaceShadow(of: hud, settlesAt: 0)
                XCTAssertEqual(shadow.opacity, 0, name)
                XCTAssertNil(shadow.animation(forKey: "opacity"), "\(name): at once")
            }
        }
    }

    /// A HUD with no detached card gives its shells their shadows as it always has.
    func testShellsWithNoDetachedCardAreCastAtOnce() throws {
        for placement in [ScreenPlacement(mode: .simple, edge: .right), ScreenPlacement(mode: .logos, edge: .bottom)] {
            var pointer = CGPoint(x: -10_000, y: -10_000)
            let (hud, close) = try hud(placement) { pointer }
            defer { close() }
            let strip = hud.geometry.rect
            pointer = CGPoint(x: strip.midX, y: strip.midY)
            hud.samplePointer()
            hud.forceOpen()
            XCTAssertNil(hud.island.rootView.focusVendor, "\(placement.mode)")
            XCTAssertNotNil(hud.glowShells?.surface, "\(placement.mode): the grown panel's shell is there at once")
            XCTAssertNil(hud.glowShells?.dock, "\(placement.mode)")
        }
    }
}
