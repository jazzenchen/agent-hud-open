import AppKit
import AgentHUDCore
import SwiftUI

/// Owns one `ScreenHUD` per display and everything they share.
///
/// A HUD belongs to a screen: two displays can run different modes, be hovered independently and show
/// different things. What cannot be split lives here — the store and settings they all read, the system
/// appearance, the pointer, and the rule that an alert belongs on the screen the pointer is on.
@MainActor
final class IslandController {
    static let expandedWidth: CGFloat = 540
    static let defaultPanelHeight: CGFloat = 326
    static let expandedRadius: CGFloat = 26
    static let alertWingWidth: CGFloat = 112
    static let alertSidePadding: CGFloat = 16
    static let alertDetailWidth: CGFloat = 400

    private let store: UsageStore
    private let settings: SettingsStore
    private let additionalHUDControls: @MainActor (@escaping @MainActor () -> Void) -> AnyView
    private(set) var huds: [String: ScreenHUD] = [:]
    /// Display order, so the primary HUD is stable rather than whatever the dictionary yields.
    private var order: [String] = []
    private var systemIsLight = SystemAppearance.isLight
    private var observers: [Any] = []
    private var pointerMonitors: [Any] = []

    var onOpenStats: (() -> Void)? { didSet { huds.values.forEach { $0.onOpenStats = onOpenStats } } }
    var onOpenSettings: (() -> Void)? { didSet { huds.values.forEach { $0.onOpenSettings = onOpenSettings } } }

    init(store: UsageStore, settings: SettingsStore,
         additionalHUDControls: @escaping @MainActor (@escaping @MainActor () -> Void) -> AnyView = { _ in AnyView(EmptyView()) }) {
        self.store = store
        self.settings = settings
        self.additionalHUDControls = additionalHUDControls
        rebuild()

        observers.append(NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.rebuild() }
        })
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("AppleInterfaceThemeChangedNotification"), object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                self.systemIsLight = SystemAppearance.isLight
                self.huds.values.forEach { $0.systemIsLight = self.systemIsLight }
                self.apply(animated: false)
            }
        })
        trackChanges({ [weak self] in self?.inputs }, onChange: { [weak self] in
            self?.apply(animated: true)
        })
        startPointerMonitors()
    }

    /// What every HUD's frame and glow are computed from. Pause expiry and session liveness read the store's
    /// clock, which ticks every ten seconds, so the values are compared and a tick that changes none of them
    /// leaves the HUDs alone; the panel's countdowns observe the clock themselves.
    private struct Inputs: Equatable {
        let rows: [AgentRow]
        let sessions: [LiveSession]
        let isPaused: Bool
        let glowHidden: Bool
        let appearance: GlowAppearance
        let settings: AgentHUDCore.Settings
        let agents: [AgentDescriptor]
    }

    private var inputs: Inputs {
        Inputs(rows: store.rows, sessions: store.sessions, isPaused: store.isPaused, glowHidden: store.glowHidden,
               appearance: store.glowAppearance(light: systemIsLight), settings: settings.settings, agents: settings.agents)
    }

    // MARK: Screens

    /// Builds a HUD for every attached display and drops the ones whose display is gone. A machine with no
    /// screen at all still gets one, so the app has somewhere to draw the moment a display appears.
    private func rebuild() {
        ScreenIdentity.forgetKeys()
        let screens = NSScreen.screens
        rebuild(keys: screens.isEmpty ? ["screen:none"] : screens.map { ScreenIdentity.key(for: $0) }, screens: screens)
    }

    /// One HUD per key, in display order; `screens` holds the display behind each key that has one. The requests a
    /// dropped HUD held are shown again: their clients are still waiting, whatever happened to the display.
    func rebuild(keys: [String], screens: [NSScreen]) {
        var questions: [IslandAlert] = []
        for key in huds.keys where !keys.contains(key) {
            guard let hud = huds.removeValue(forKey: key) else { continue }
            questions += hud.questions
            hud.close()
        }
        for (index, key) in keys.enumerated() where huds[key] == nil {
            let hud = ScreenHUD(key: key, screen: screens.indices.contains(index) ? screens[index] : nil,
                                store: store, settings: settings, additionalHUDControls: additionalHUDControls)
            hud.systemIsLight = systemIsLight
            hud.onOpenStats = { [weak self] in self?.onOpenStats?() }
            hud.onOpenSettings = { [weak self] in self?.onOpenSettings?() }
            hud.onClaimRequest = { [weak self, weak hud] id in
                guard let self, let hud else { return }
                self.move(requestID: id, to: hud)
            }
            huds[key] = hud
        }
        order = keys
        // Every glow shares one budget, so attaching a display costs frames rather than CPU.
        GlowAnimator.activeGlows = huds.count
        apply(animated: false)
        for question in questions.sorted(by: { ($0.waitingSince ?? .distantFuture) < ($1.waitingSince ?? .distantFuture) }) {
            present(question)
        }
    }

    /// A request picked from the waiting list on another screen than the one it arrived on moves to the screen where
    /// it was picked, to be answered there.
    private func move(requestID id: String, to target: ScreenHUD) {
        for hud in huds.values where hud !== target {
            guard let request = hud.take(requestID: id) else { continue }
            target.present(request)
            return
        }
    }

    /// The HUD the tests and the menu act on when no screen is named: the main display's.
    var primary: ScreenHUD? {
        NSScreen.main.map { ScreenIdentity.key(for: $0) }.flatMap { huds[$0] } ?? order.first.flatMap { huds[$0] }
    }

    /// The HUD on the screen the pointer is on, which is where an alert belongs and which hover acts on.
    private var underPointer: ScreenHUD? {
        let point = NSEvent.mouseLocation
        let key = NSScreen.screens.first { $0.frame.contains(point) }.map { ScreenIdentity.key(for: $0) }
        return key.flatMap { huds[$0] } ?? primary
    }

    var island: IslandWindowController { primary!.island }
    var glow: GlowWindowController { primary!.glow }
    var geometry: NotchGeometry { primary!.geometry }

    // MARK: Pointer

    /// One monitor for every screen. A collapsed logo queue takes no mouse events so that clicks pass
    /// through it, which also costs it its tracking; this is what tells each HUD where the pointer is.
    private func startPointerMonitors() {
        let matching: NSEvent.EventTypeMask = [.mouseMoved, .leftMouseDragged, .rightMouseDragged]
        if let global = NSEvent.addGlobalMonitorForEvents(matching: matching, handler: { [weak self] _ in
            Task { @MainActor in self?.huds.values.forEach { $0.samplePointer() } }
        }) {
            pointerMonitors.append(global)
        }
        if let local = NSEvent.addLocalMonitorForEvents(matching: matching, handler: { [weak self] event in
            Task { @MainActor in self?.huds.values.forEach { $0.samplePointer() } }
            return event
        }) {
            pointerMonitors.append(local)
        }
    }

    // MARK: Forwarding

    func apply(animated: Bool) { huds.values.forEach { $0.apply(animated: animated) } }

    func forceOpen() { underPointer?.forceOpen() }

    func forceCollapse() { huds.values.forEach { $0.forceCollapse() } }

    func pointer(inside: Bool) { underPointer?.pointer(inside: inside) }

    func present(_ alert: QuotaAlert) { present(.quota(alert)) }

    /// An event is shown once, on the screen being looked at. Repeating it on every display would mean
    /// dismissing the same thing several times, and a screen nobody is facing is not where news belongs. A request
    /// one screen already holds stays there.
    func present(_ alert: IslandAlert) {
        guard !alert.isPersistent || !huds.values.contains(where: { $0.holds(alert.id) }) else { return }
        underPointer?.present(alert)
    }

    /// A withdrawn request is taken off whichever screen ended up showing it.
    func withdraw(requestID: String) { huds.values.forEach { $0.withdraw(requestID: requestID) } }
}
