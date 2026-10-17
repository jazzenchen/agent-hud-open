import AppKit
import SwiftUI
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// Each screen draws its own layout: its hover panel, and which agents its resting HUD shows, in what order. A second
/// HUD on a key no display has is a Dynamic dock, the default of a display without a notch.
@MainActor
final class ScreenLayoutHUDTests: XCTestCase {
    private let second = "screen:second"

    /// The demo with every agent, over settings of their own, with the main display in `mode`.
    private func make(_ mode: HUDMode = .logos, separates: Bool = false) throws -> (store: UsageStore, settings: SettingsStore, key: String, screen: NSScreen) {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        let domain = "app.agenthud.tests.screen-layouts.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        addTeardownBlock { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
        settings.update { $0.screens[key] = ScreenPlacement(mode: mode, edge: .top, separatesAgents: separates) }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        return (store, settings, key, screen)
    }

    private func hud(_ key: String, _ screen: NSScreen?, _ store: UsageStore, _ settings: SettingsStore,
                     pointer: @escaping @MainActor () -> CGPoint = { CGPoint(x: -10_000, y: -10_000) }) -> ScreenHUD {
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings, mouseLocation: pointer)
        addTeardownBlock { hud.close() }
        return hud
    }

    private func keep(_ layout: PanelLayout, for screen: String, in settings: SettingsStore) {
        settings.panelLayout.propose(.own(layout), for: screen)
        settings.panelLayout.save(.screen(screen))
    }

    private func marks(_ hud: ScreenHUD) -> [String] {
        (hud.island.rootView.logoQueue?.items.map(\.vendor) ?? hud.island.rootView.simpleMeter?.items.map(\.vendor) ?? [])
            .map(AgentArtwork.markKey)
    }

    /// Waits for the main run loop to bring `condition` about, for up to two seconds.
    private func poll(_ what: String, until condition: () -> Bool) {
        let deadline = Date().addingTimeInterval(2)
        while !condition(), Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(condition(), what)
    }

    // MARK: Without a layout

    /// No layout at all draws today's HUD: every agent as the store gives them, in the dock, the Simple panel and the
    /// glow, and the panel the shared layout draws.
    func testWithoutALayoutTheHUDShowsEveryAgentAsItComes() throws {
        let (store, settings, key, screen) = try make()
        let dock = hud(key, screen, store, settings)
        let agents = dock.agents
        XCTAssertFalse(agents.order.narrows)
        XCTAssertEqual(agents.queueItems, LogoQueueItem.queue(rows: store.queueVendors))
        XCTAssertEqual(agents.meterItems, SimpleMeterItem.items(rows: store.rows, balances: SimpleMeterItem.balances(store),
                                                                working: store.queueVendors.filter(\.isWorking).map(\.vendor)))
        XCTAssertEqual(agents.pulseVendors, store.alertPulseVendors)
        XCTAssertEqual(agents.glowAppearance(light: false, on: key), store.glowAppearance(light: false, on: key))
        XCTAssertEqual(agents.anyAgentActive, store.hasLiveSession)
        XCTAssertEqual(dock.island.rootView.logoQueue?.items, LogoQueueItem.queue(rows: store.queueVendors))
        XCTAssertGreaterThan(marks(dock).count, 4)
        XCTAssertEqual(dock.island.rootView.panelScreen, key)
        XCTAssertEqual(IslandWindowController.naturalHeight(of: HoverPanelView(store: store, onOpenStats: {}, screen: key)
                .frame(width: IslandController.expandedWidth)),
                       IslandWindowController.naturalHeight(of: HoverPanelView(store: store, onOpenStats: {})
                .frame(width: IslandController.expandedWidth)), "the screen's panel is the shared panel")
    }

    // MARK: Two screens

    func testTwoScreensWithTheirOwnLayoutsDrawDifferentPanelsAndRestingHUDs() throws {
        let (store, settings, key, screen) = try make(.notch)
        keep(PanelLayout(hiddenSections: [.tokens], hud: PanelLayout.HUD(agents: ["OpenClaw", "Claude"])), for: key, in: settings)
        keep(PanelLayout(hud: PanelLayout.HUD(agents: ["Kimi", "Codex", "Claude"])), for: second, in: settings)
        let notch = hud(key, screen, store, settings), dock = hud(second, nil, store, settings)
        XCTAssertEqual(notch.geometry.mode, .notch)
        XCTAssertEqual(dock.geometry.mode, .logos, "a key no display has gets a display without a notch's default")

        XCTAssertEqual(marks(dock), ["Kimi", "ChatGPT", "Claude"], "the dock's marks, narrowed and in the list's order")
        let glow = notch.agents.glowSegments.map { AgentArtwork.markKey($0.vendor) }
        XCTAssertEqual(Set(glow), ["OpenClaw", "Claude"], "the notch's glow has only its agents' colours")
        XCTAssertEqual(glow.first, "OpenClaw", "in the list's order")
        XCTAssertEqual(notch.agents.pulseVendors, notch.agents.glowSegments.map(\.vendor), "the alert pulse is placed in the same list")
        XCTAssertEqual(notch.agents.glowAppearance(light: false, on: key).stops,
                       GlowGradient.stops(levels: notch.agents.glowSegments.map(\.level), light: false))
        XCTAssertLessThan(notch.agents.glowSegments.count, store.view.glowSegments.count)
        XCTAssertEqual(Set(dock.agents.glowSegments.map { AgentArtwork.markKey($0.vendor) }), ["Kimi", "ChatGPT", "Claude"],
                       "the dock's backdrop follows its own list")

        // Each panel draws its own screen's layout, in the island and in the fresh view tree it is measured from.
        notch.forceOpen()
        dock.forceOpen()
        XCTAssertEqual(notch.island.rootView.panelScreen, key)
        XCTAssertEqual(dock.island.rootView.panelScreen, second)
        func measured(_ hud: ScreenHUD) -> CGFloat { hud.island.contentHeight(for: hud.island.rootView) }
        func drawn(_ hud: ScreenHUD, _ screen: String?) -> CGFloat {
            IslandWindowController.naturalHeight(of: HoverPanelView(store: store, onOpenStats: {}, insets: hud.panelDisplay.insets,
                                                                    screen: screen).frame(width: IslandController.expandedWidth))
        }
        XCTAssertEqual(measured(dock), drawn(dock, second), accuracy: 0.5)
        XCTAssertEqual(measured(dock), drawn(dock, nil), accuracy: 0.5, "the dock's list leaves its panel the shared one")
        XCTAssertEqual(measured(notch), drawn(notch, key), accuracy: 0.5)
        XCTAssertLessThan(measured(notch), drawn(notch, nil) - 40, "the notch's own layout hides the token chart")

        // What one screen lists changes nothing on the other, and the store's own lists stay whole.
        XCTAssertEqual(store.alertPulseVendors.count, store.view.glowSegments.count)
        XCTAssertEqual(LogoQueueItem.queue(rows: store.queueVendors).count, ScreenAgents.available(store: store, mode: .logos).count)
    }

    /// A list that names no agent the screen shows now leaves the screen showing every agent, never an empty HUD.
    func testAListThatNamesNoAgentPresentShowsEveryAgent() throws {
        let (store, settings, key, screen) = try make(.simple)
        keep(PanelLayout(hud: PanelLayout.HUD(agents: ["Nobody", "Zed"])), for: key, in: settings)
        let panel = hud(key, screen, store, settings)
        XCTAssertFalse(panel.agents.order.narrows)
        XCTAssertEqual(panel.island.rootView.simpleMeter?.items.map(\.vendor),
                       SimpleMeterItem.items(rows: store.rows, balances: SimpleMeterItem.balances(store),
                                             working: store.queueVendors.filter(\.isWorking).map(\.vendor)).map(\.vendor))
        keep(PanelLayout(hud: PanelLayout.HUD(agents: ["Zed", "Codex"])), for: key, in: settings)
        panel.apply(animated: false)
        XCTAssertEqual(marks(panel), ["ChatGPT"], "an agent listed and away is passed over")
    }

    /// The glow breathes at the working pace only for a shown agent's session in flight.
    func testTheGlowsPaceCountsOnlyTheShownAgentsSessions() throws {
        let (store, settings, key, screen) = try make(.notch)
        let live = try XCTUnwrap(store.view.liveSessions.first?.source, "the sample has a session in flight")
        let vendor = try XCTUnwrap(live.agentVendor ?? live.vendor)
        let idle = try XCTUnwrap(store.view.glowSegments.map(\.vendor).first { vendor in
            !store.view.liveSessions.contains { ($0.source.agentVendor ?? $0.source.vendor).map(AgentArtwork.markKey) == AgentArtwork.markKey(vendor) }
        }, "and an agent with a level and nothing running")
        keep(PanelLayout(hud: PanelLayout.HUD(agents: [idle])), for: key, in: settings)
        let notch = hud(key, screen, store, settings)
        XCTAssertTrue(store.hasLiveSession)
        XCTAssertTrue(notch.agents.order.narrows)
        XCTAssertFalse(notch.agents.anyAgentActive)
        XCTAssertEqual(notch.agents.glowAppearance(light: false, on: key).breathSeconds,
                       settings.settings.glow(on: key).idleBreathSeconds, accuracy: 0.001)
        keep(PanelLayout(hud: PanelLayout.HUD(agents: [idle, vendor])), for: key, in: settings)
        XCTAssertTrue(notch.agents.anyAgentActive)
    }

    // MARK: A draft

    /// An agent's draft re-applies that screen's closed HUD at once, through the controller that watches the layouts,
    /// and leaves the other screen alone; discarding it brings the HUD back.
    func testADraftReappliesTheClosedHUDAtOnce() throws {
        let (store, settings, key, screen) = try make()
        let controller = IslandController(store: store, settings: settings)
        defer { controller.rebuild(keys: [], screens: []) }
        controller.rebuild(keys: [key, second], screens: [screen])
        let dock = try XCTUnwrap(controller.huds[key]), other = try XCTUnwrap(controller.huds[second])
        let every = marks(dock)
        XCTAssertEqual(marks(other), every)

        settings.panelLayout.propose(.own(PanelLayout(hud: PanelLayout.HUD(agents: ["Cursor", "Claude"]))), for: key)
        poll("the dock shows the draft's agents") { marks(dock) == ["Cursor", "Claude"] }
        XCTAssertFalse(dock.island.rootView.isOpen, "nothing opens the panel")
        XCTAssertEqual(marks(other), every, "the other screen is left alone")

        settings.panelLayout.discard(.screen(key))
        poll("discarding brings every agent back") { marks(dock) == every }

        // A shared draft reaches every screen that uses the shared layout.
        settings.panelLayout.propose(PanelLayout(hud: PanelLayout.HUD(agents: ["Codex"])))
        poll("a shared draft re-applies the dock") { marks(dock) == ["ChatGPT"] }
        poll("and the other screen") { marks(other) == ["ChatGPT"] }
    }

    // MARK: What the tools are told

    /// The tools list every attached display by its key and its role, never by the name the system or its owner gave
    /// it, with its mode and what its resting HUD can show there.
    func testTheToolsListEveryAttachedDisplayByRole() throws {
        let (store, settings, key, _) = try make(.simple)
        let controller = IslandController(store: store, settings: settings)
        defer { controller.rebuild(keys: [], screens: []) }
        let listed = controller.panelScreens
        XCTAssertEqual(listed.map(\.key), NSScreen.screens.map { ScreenIdentity.key(for: $0) })
        for (index, (screen, entry)) in zip(NSScreen.screens, listed).enumerated() {
            let roles = ["Built-in display", index == 0 ? "Main display" : "Display \(index + 1)"]
            XCTAssertTrue(roles.contains(entry.name), entry.name)
            XCTAssertNotEqual(entry.name, screen.localizedName, "never the display's own name")
            let placement = ScreenIdentity.placement(for: screen, in: settings.settings)
            XCTAssertEqual(entry.mode, placement.mode)
            XCTAssertEqual(entry.hasNotch, ScreenIdentity.hasNotch(screen))
            XCTAssertEqual(entry.agents, ScreenAgents.available(store: store, mode: placement.mode))
        }
        let main = try XCTUnwrap(listed.first { $0.key == key })
        XCTAssertEqual(main.mode, .simple)
        XCTAssertEqual(main.agents, ScreenAgents.available(store: store, mode: .simple), "the Simple panel's items")
        XCTAssertNotEqual(main.agents, ScreenAgents.available(store: store, mode: .notch), "which differ from the glow's parts")
    }

    // MARK: A dock that separates its agents

    /// On a dock that separates its agents the marks follow the list, and so does what hovering one opens. An event
    /// from an agent the screen does not show has no mark here, so its card comes out of the middle of the dock.
    func testASeparatedDocksMarksFocusAndPeeksFollowTheList() throws {
        let (store, settings, key, screen) = try make(.logos, separates: true)
        keep(PanelLayout(hud: PanelLayout.HUD(agents: ["Cursor", "Codex"])), for: key, in: settings)
        let pointer = Pointer()
        let dock = hud(key, screen, store, settings) { pointer.at }
        XCTAssertEqual(marks(dock), ["Cursor", "ChatGPT"])
        let strip = dock.geometry.rect
        pointer.at = CGPoint(x: strip.minX + 6, y: strip.midY)
        dock.samplePointer()
        dock.forceOpen()
        XCTAssertEqual(dock.island.rootView.focusVendor, "Cursor", "the first mark is the list's first agent")
        XCTAssertEqual(dock.island.rootView.peek?.vendor, "Cursor")
        pointer.at = CGPoint(x: strip.maxX - 6, y: strip.midY)
        dock.samplePointer()
        XCTAssertEqual(dock.island.rootView.focusVendor.map(AgentArtwork.markKey), "ChatGPT")
        dock.forceCollapse()

        pointer.at = CGPoint(x: -10_000, y: -10_000)
        dock.samplePointer()
        dock.present(.completion(SessionCompletion(sessionID: "hidden", vendor: "Claude", turnID: "reply", task: "New reply",
                                                   model: "test", startedAt: nil, completedAt: Date())))
        XCTAssertNotNil(dock.island.rootView.alert, "an agent the screen does not show still reaches it")
        XCTAssertEqual(dock.island.rootView.focusVendor, "Claude", "with no mark here, its card comes out of the dock's middle")
    }
}

/// Where a test puts the pointer, which the HUD reads each time it samples it.
@MainActor
private final class Pointer {
    var at = CGPoint(x: -10_000, y: -10_000)
}
