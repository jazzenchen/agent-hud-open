import AgentHUDSupport
import AppKit
import SwiftUI
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// A screen's resting HUD under an agent's style draft, and the open panel at the width its layout sets. A draft
/// re-applies that screen's HUD at once on the path a person's change in Settings takes; Save writes it into the
/// settings; a person's drag drops it first. The panel's width follows the layout on the notch and on every dock edge.
@MainActor
final class ScreenStyleHUDTests: XCTestCase {
    private var changes = 0

    /// The demo over settings of their own, with the main display in `placement`.
    private func make(_ placement: ScreenPlacement) throws -> (store: UsageStore, settings: SettingsStore, key: String, screen: NSScreen) {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        let domain = "app.agenthud.tests.screen-style.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        addTeardownBlock { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        settings.update {
            $0.screens[key] = placement
            $0.collapseDelayMs = 5000
        }
        settings.panelLayout.setSwitches { $0.tokenUsage = false; $0.sessionList = false }
        settings.onChange = { [weak self] _ in self?.changes += 1 }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, accessAllowed: { false })
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        return (store, settings, key, screen)
    }

    /// Waits for the main run loop to bring `condition` about, for up to two seconds.
    private func poll(_ what: String, line: UInt = #line, until condition: () -> Bool) {
        let deadline = Date().addingTimeInterval(2)
        while !condition(), Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(condition(), what, line: line)
    }

    /// The open card's frame on screen, from its place in the canvas.
    private func card(_ hud: ScreenHUD) throws -> CGRect {
        let local = try XCTUnwrap(hud.island.rootView.presentationFrame)
        let canvas = hud.island.panel.frame
        return CGRect(x: canvas.minX + local.minX, y: canvas.maxY - local.maxY, width: local.width, height: local.height)
    }

    // MARK: A style draft

    /// A draft re-applies the HUD through the controller that watches the settings and the layouts: a new mode, a new
    /// edge and larger logos each show at once, with nothing opened and nothing written; discarding brings it back.
    func testADraftReappliesTheHUDWithItsModeEdgeAndSizes() throws {
        let (store, settings, key, screen) = try make(ScreenPlacement(mode: .logos, edge: .top, logoSize: 20))
        let controller = IslandController(store: store, settings: settings)
        defer { controller.rebuild(keys: [], screens: []) }
        controller.rebuild(keys: [key], screens: [screen])
        let hud = try XCTUnwrap(controller.huds[key])
        let layout = settings.panelLayout
        let saved = hud.geometry.rect
        changes = 0

        layout.proposeStyle(HUDStyle(logoSize: 32), for: key, hasNotch: false)
        poll("larger logos make a thicker strip") { hud.geometry.rect.height > saved.height + 10 }
        XCTAssertEqual(hud.island.rootView.logoQueue?.logo, 32)

        layout.proposeStyle(HUDStyle(edge: .left), for: key, hasNotch: false)
        poll("the dock moves to the left edge") { hud.geometry.edge == .left }
        XCTAssertEqual(hud.geometry.rect.minX, screen.frame.minX, accuracy: 1)
        XCTAssertEqual(hud.island.rootView.edge, .left)

        layout.proposeStyle(HUDStyle(mode: .simple, simpleSize: 30), for: key, hasNotch: false)
        poll("the dock turns into a Simple panel") { hud.geometry.mode == .simple }
        XCTAssertNotNil(hud.island.rootView.simpleMeter)
        XCTAssertNil(hud.island.rootView.logoQueue)
        XCTAssertEqual(hud.island.rootView.simpleMeter?.size, 30)
        XCTAssertFalse(hud.island.rootView.isOpen, "nothing opens the panel")
        XCTAssertEqual(changes, 0, "and nothing is written")
        XCTAssertEqual(settings.settings.screens[key]?.mode, .logos)

        layout.discardPanel(on: key)
        poll("discarding brings the saved dock back") { hud.geometry.mode == .logos && hud.geometry.edge == .top }
        XCTAssertEqual(hud.geometry.rect, saved)
    }

    /// The glow follows a draft too: the screen's glow is its settings with the draft drawn over them, its brightness
    /// in the appearance it is drawn with, and no other screen's glow changes.
    func testTheGlowFollowsADraft() throws {
        let (_, settings, key, screen) = try make(ScreenPlacement(mode: .notch))
        // A store that may read, so its glow is not the paused one.
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
        defer { hud.close() }
        let before = hud.agents.glowAppearance(light: false, on: key)
        settings.panelLayout.proposeStyle(HUDStyle(glow: HUDStyle.Glow(style: .dots, brightness: 0.4)), for: key, hasNotch: true)
        XCTAssertEqual(settings.effectiveGlow(on: key).style, .dots)
        let drafted = hud.agents.glowAppearance(light: false, on: key)
        XCTAssertLessThan(drafted.peakOpacity, before.peakOpacity, "a dimmer glow")
        XCTAssertEqual(store.glowAppearance(light: false, on: "screen:other"), store.glowAppearance(light: false, on: "screen:another"))
        settings.panelLayout.discardStyle(key)
        XCTAssertEqual(hud.agents.glowAppearance(light: false, on: key), before)
    }

    /// Save writes the draft into the settings with one change; the HUD keeps the look it showed.
    func testSaveWritesTheSettingsAndTheHUDKeepsItsLook() throws {
        let (store, settings, key, screen) = try make(ScreenPlacement(mode: .logos, edge: .top))
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
        defer { hud.close() }
        settings.panelLayout.proposeStyle(HUDStyle(edge: .bottom, logoSize: 28), for: key, hasNotch: false)
        hud.apply(animated: false)
        let drafted = hud.geometry
        XCTAssertEqual(drafted.edge, .bottom)
        changes = 0
        settings.panelLayout.savePanel(on: key)
        XCTAssertEqual(changes, 1)
        XCTAssertEqual(settings.settings.screens[key]?.edge, .bottom)
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 28)
        hud.apply(animated: false)
        XCTAssertEqual(hud.geometry, drafted)
        settings.panelLayout.revert(.screen(key))
        hud.apply(animated: false)
        XCTAssertEqual(hud.geometry.edge, .top, "Revert to Previous takes the style back")
    }

    /// A person who drags the HUD wins: the draft is dropped first, the drag starts from the saved placement, and the
    /// drop writes only what the drag changed.
    func testADragDropsTheDraftFirst() throws {
        let (store, settings, key, screen) = try make(ScreenPlacement(mode: .logos, edge: .top, offset: 0.5, logoSize: 20))
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
        defer { hud.close() }
        settings.panelLayout.proposeStyle(HUDStyle(logoSize: 34, glow: HUDStyle.Glow(style: .ascii)), for: key, hasNotch: false)
        hud.apply(animated: false)
        XCTAssertEqual(hud.island.rootView.logoQueue?.logo, 34)
        hud.beginMoving()
        XCTAssertNil(settings.panelLayout.styleDraft(of: key), "the draft is dropped as the drag begins")
        XCTAssertEqual(hud.island.rootView.logoQueue?.logo, 20, "the drag starts from what is saved")
        hud.move(to: CGPoint(x: screen.frame.minX + screen.frame.width * 0.25, y: screen.frame.maxY - 4))
        hud.finishMoving()
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 20)
        XCTAssertNil(settings.settings.screenGlow[key], "nothing of the draft is kept")
        XCTAssertFalse(settings.panelLayout.canRevert(.screen(key)), "a drag is no version")
    }

    // MARK: The panel's width

    /// The open panel is as wide as its layout says, at 480 and at 720, beside the notch, centred as today, and on every
    /// dock edge; the content is laid out at that width, and the default is 540.
    func testTheOpenPanelTakesTheLayoutsWidthOnTheNotchAndEveryDockEdge() throws {
        let placements = [ScreenPlacement(mode: .notch)] + HUDEdge.allCases.map { ScreenPlacement(mode: .logos, edge: $0) }
        for placement in placements {
            for width in [480, 540, 720] {
                let (store, settings, key, screen) = try make(placement)
                if width != 540 {
                    settings.panelLayout.propose(.own(PanelLayout(style: PanelStyle(width: width))), for: key)
                }
                let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
                defer { hud.close() }
                hud.forceOpen()
                let name = "\(placement.mode) \(placement.edge) at \(width)"
                XCTAssertEqual(hud.island.rootView.panelWidth, CGFloat(width), name)
                let card = try card(hud)
                let flare = NotchGeometry.expandedTopRadius
                let expected = CGFloat(width) + (placement.mode == .notch || placement.edge.isHorizontal ? 2 * flare : 0)
                XCTAssertEqual(card.width, expected, accuracy: 0.5, name)
                if placement.mode == .notch {
                    XCTAssertEqual(card.midX, hud.geometry.rect.midX, accuracy: 0.5, "\(name): centred under the notch")
                }
                XCTAssertTrue(screen.frame.insetBy(dx: -1, dy: -1).contains(card), name)
                // The island is measured from a fresh view tree at the same width.
                let measured = hud.island.contentHeight(for: hud.island.rootView)
                let drawn = IslandWindowController.naturalHeight(of: HoverPanelView(store: store, onOpenStats: {},
                                                                                    insets: hud.panelDisplay.insets, screen: key)
                    .frame(width: CGFloat(width)))
                XCTAssertEqual(measured, drawn, accuracy: 0.5, name)
            }
        }
    }

    /// A dock on a side edge keeps its strip clear inside the panel, so where that would leave the content narrower than
    /// the narrowest panel's, 404 points, the panel is widened to it; the island measures it at that width, and its
    /// picture is drawn at it. A default dock at the default width keeps the width it had.
    func testASideDockKeepsItsContentAtLeastAsWideAsTheNarrowestPanel() throws {
        let narrowest = HoverPanelView.narrowestContentWidth
        XCTAssertEqual(narrowest, 404)
        let placements = [ScreenPlacement(mode: .simple, edge: .left, simpleSize: ScreenPlacement.simpleSizeRange.upperBound),
                          ScreenPlacement(mode: .simple, edge: .right),
                          ScreenPlacement(mode: .logos, edge: .left, logoSize: 36, logoEdgeDistance: 20),
                          ScreenPlacement(mode: .logos, edge: .right)]
        for placement in placements {
            for width in [480, 540, 720] {
                let (store, settings, key, screen) = try make(placement)
                if width != 540 {
                    settings.panelLayout.propose(.own(PanelLayout(style: PanelStyle(width: width))), for: key)
                }
                let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
                defer { hud.close() }
                hud.forceOpen()
                let name = "\(placement.mode) \(placement.edge) size \(placement.simpleSize)/\(placement.logoSize) at \(width)"
                let insets = hud.panelDisplay.insets
                let drawn = max(CGFloat(width), narrowest + insets.leading + insets.trailing)
                XCTAssertEqual(hud.island.rootView.panelWidth, drawn, name)
                XCTAssertEqual(try card(hud).width, drawn, accuracy: 0.5, name)
                XCTAssertGreaterThanOrEqual(drawn - insets.leading - insets.trailing, narrowest, name)
                XCTAssertEqual(PanelPictureRenderer(store: store, display: hud.panelDisplay, screen: key).width, drawn, name)
                let measured = hud.island.contentHeight(for: hud.island.rootView)
                let natural = IslandWindowController.naturalHeight(of: HoverPanelView(store: store, onOpenStats: {}, insets: insets,
                                                                                      screen: key).frame(width: drawn))
                XCTAssertEqual(measured, natural, accuracy: 0.5, name)
                if width == 540, placement.simpleSize == ScreenPlacement().simpleSize, placement.logoSize == ScreenPlacement().logoSize {
                    XCTAssertEqual(drawn, 540, "\(name): a default dock keeps the width it had")
                }
            }
        }
    }

    /// A proposal's answer measures the panel as its screen will show it: with the insets of the mode and edge the draft
    /// gives, before the HUD has re-applied it, the same height `get_hud` reads once it has.
    func testAProposalIsMeasuredWithTheInsetsItsDraftGives() throws {
        let (store, settings, key, screen) = try make(ScreenPlacement(mode: .simple, edge: .left))
        let controller = IslandController(store: store, settings: settings)
        defer { controller.rebuild(keys: [], screens: []) }
        controller.rebuild(keys: [key], screens: [screen])
        let hud = try XCTUnwrap(controller.huds[key])
        let tools = HUDTools(store: store, screens: { controller.panelScreens }, fit: { scope in
            PanelPictureRenderer(store: store, display: controller.panelDisplay(for: scope.screenKey), screen: scope.screenKey).fit()
        })
        func height(_ tool: String, _ arguments: [String: JSONValue]) throws -> JSONValue? {
            guard case .result(let result, _) = tools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
                XCTFail("\(tool) was refused")
                throw CancellationError()
            }
            return result["height"]
        }
        let before = try height(MCPTools.getHUD, ["screen": .string(key)])
        let proposed = try height(MCPTools.draftHUD, ["screen": .string(key), "hud": .object([
            "mode": .string("dynamic"), "edge": .string("top"), "logoSize": .integer(36), "edgeDistance": .integer(20),
        ])])
        XCTAssertNotEqual(proposed, before, "a dock on the top edge keeps its strip clear above the content")
        poll("the HUD re-applies the draft") { hud.geometry.mode == .logos && hud.geometry.edge == .top }
        XCTAssertEqual(proposed, try height(MCPTools.getHUD, ["screen": .string(key)]))
    }

    /// One agent's own panel on a dock that separates its agents is 100 points narrower, and never below 440.
    func testAnAgentsOwnPanelIsAHundredPointsNarrower() throws {
        for (width, agent) in [(480, 440), (540, 440), (720, 620)] {
            let (store, settings, key, screen) = try make(ScreenPlacement(mode: .logos, edge: .top, separatesAgents: true))
            settings.panelLayout.propose(.own(PanelLayout(style: PanelStyle(width: width))), for: key)
            var pointer = CGPoint(x: -10_000, y: -10_000)
            let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings, mouseLocation: { pointer })
            defer { hud.close() }
            let strip = hud.geometry.rect
            pointer = CGPoint(x: strip.minX + 6, y: strip.midY)
            hud.samplePointer()
            hud.forceOpen()
            hud.pinAgent()
            XCTAssertTrue(hud.island.rootView.pinned, "\(width)")
            XCTAssertEqual(hud.island.rootView.agentPanelWidth, CGFloat(agent), "\(width)")
            XCTAssertEqual(try card(hud).width, CGFloat(agent), "\(width)")
        }
    }

    /// Beside the notch the glow rims the open panel too, and its window holds the farthest field any setting draws round
    /// the widest panel: a grid's deepest rows round a 720-point panel are not cut at the window's sides.
    func testTheGlowWindowHoldsTheDeepestGlowRoundTheWidestPanel() throws {
        let (_, settings, key, screen) = try make(ScreenPlacement(mode: .notch))
        var glow = GlowSettings()
        glow.style = .dots
        glow.gridPitch = GlowSettings.gridPitchRange.upperBound
        glow.gridCore = GlowSettings.gridCoreRange.upperBound
        glow.gridFade = GlowSettings.gridFadeRange.upperBound
        settings.update { $0.screenGlow[key] = glow }
        // A store that may read, so its glow is not the paused one, which draws nothing.
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        settings.panelLayout.propose(.own(PanelLayout(style: PanelStyle(width: PanelStyle.widthRange.upperBound))), for: key)
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
        defer { hud.close() }
        hud.forceOpen()
        poll("the open panel's glow is drawn") { hud.glow.glowLayer.frame.width > CGFloat(PanelStyle.widthRange.upperBound) }
        let field = hud.glow.glowLayer.frame, window = hud.glow.panel.frame
        XCTAssertGreaterThanOrEqual(field.minX, 0, "\(field) in a window \(window.width) wide")
        XCTAssertLessThanOrEqual(field.maxX, window.width, "\(field) in a window \(window.width) wide")
    }

    /// A width proposed while the panel is open resizes it at once, through the controller that watches the layouts.
    func testAWidthDraftResizesAnOpenPanel() throws {
        let (store, settings, key, screen) = try make(ScreenPlacement(mode: .notch))
        let controller = IslandController(store: store, settings: settings)
        defer { controller.rebuild(keys: [], screens: []) }
        controller.rebuild(keys: [key], screens: [screen])
        let hud = try XCTUnwrap(controller.huds[key])
        hud.forceOpen()
        let flare = 2 * NotchGeometry.expandedTopRadius
        XCTAssertEqual(try card(hud).width, 540 + flare, accuracy: 0.5)
        settings.panelLayout.propose(PanelLayout(style: PanelStyle(width: 700)))
        poll("the open panel widens") { (try? card(hud).width).map { abs($0 - 700 - flare) < 0.5 } ?? false }
        settings.panelLayout.discard()
        poll("and narrows back") { (try? card(hud).width).map { abs($0 - 540 - flare) < 0.5 } ?? false }
    }
}
