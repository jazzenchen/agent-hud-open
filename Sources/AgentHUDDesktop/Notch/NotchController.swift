import AppKit
import AgentHUDCore

/// Coordinates the glow window, the island window and the hover state machine.
@MainActor
final class NotchController {
    static let expandedWidth: CGFloat = 540
    static let defaultPanelHeight: CGFloat = 326
    static let expandedRadius: CGFloat = 26
    static let alertWingWidth: CGFloat = 112
    static let alertSidePadding: CGFloat = 16
    static let alertDetailWidth: CGFloat = 400

    /// Height of the open panel; follows the content reported by `IslandRootView`.
    private var panelHeight: CGFloat = NotchController.defaultPanelHeight
    private var alertDetailHeight: CGFloat = 300
    private var expandedSize: CGSize { CGSize(width: Self.expandedWidth, height: panelHeight) }

    private let store: UsageStore
    private let settings: SettingsStore
    private(set) var geometry: NotchGeometry
    private let glow: GlowWindowController
    private let island: IslandWindowController
    private var machine = HoverMachine()
    private var timer: Timer?
    private var shrinkTask: Task<Void, Never>?
    private var targetWindowFrame: CGRect?
    private var systemIsLight = SystemAppearance.isLight
    private var observers: [Any] = []
    // Whether the user was already reading the normal panel when this event arrived.
    private var presentedAlert: (event: IslandAlert, inUsagePanel: Bool)?
    private var activeAlert: IslandAlert? { presentedAlert?.event }
    private var showsAlertDetails: Bool { machine.isOpen && presentedAlert?.inUsagePanel == false }
    private var pendingAlerts: [IslandAlert] = []
    private var alertDismissTask: Task<Void, Never>?
    private var pointerInside = false

    var onOpenStats: (() -> Void)?
    var onOpenSettings: (() -> Void)?

    init(store: UsageStore, settings: SettingsStore) {
        self.store = store
        self.settings = settings
        let geometry = NotchGeometry.detect()
        self.geometry = geometry
        glow = GlowWindowController(geometry: geometry)
        island = IslandWindowController(frame: geometry.islandFrame, rootView: IslandRootView.placeholder)
        island.onPointerChange = { [weak self] inside in
            self?.pointer(inside: inside)
        }
        NSLog("[AgentHUD] notch=%@ rect=%@", geometry.hasNotch ? "yes" : "no", NSStringFromRect(geometry.rect))

        observers.append(NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.relayout() }
        })
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("AppleInterfaceThemeChangedNotification"), object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.systemIsLight = SystemAppearance.isLight
                self?.apply(animated: false)
            }
        })
        observeChanges({ [weak self] in
            guard let self else { return }
            _ = self.store.rows
            _ = self.store.sessions
            _ = self.store.pausedUntil
            _ = self.store.glowHidden
            _ = self.store.now
            _ = self.settings.settings
            _ = self.settings.agents
        }, onChange: { [weak self] in
            self?.apply(animated: true)
        })

        apply(animated: false)
        island.show()
    }

    var isOpen: Bool { machine.isOpen }

    // MARK: Hover

    func pointer(inside: Bool) {
        pointerInside = inside
        if activeAlert != nil {
            if inside { alertDismissTask?.cancel() }
            else { scheduleAlertDismissal() }
        }
        let now = Date()
        let event: HoverMachine.Event = inside ? .pointerEntered(at: now) : .pointerExited(at: now)
        transition(machine.reduce(event, config: config))
    }

    func forceOpen() {
        transition(machine.reduce(.forceOpen, config: config))
    }

    func forceCollapse() {
        transition(machine.reduce(.forceCollapse, config: config))
    }

    // MARK: Quota events

    func present(_ alert: QuotaAlert) { present(.quota(alert)) }

    func present(_ alert: IslandAlert) {
        guard alert.isPreview || (!store.glowHidden && !store.isPaused) else { return }
        if activeAlert != nil && !alert.isPreview {
            pendingAlerts.append(alert)
            return
        }
        presentedAlert = (alert, presentedAlert?.inUsagePanel ?? machine.isOpen)
        // An event owns the brief expansion; a pending hover must not open the full panel underneath it.
        timer?.invalidate()
        timer = nil
        if !machine.isOpen { machine = HoverMachine() }
        if pointerInside {
            transition(machine.reduce(.pointerEntered(at: Date()), config: config))
        }
        apply(animated: true)
        island.show()
        scheduleAlertDismissal()
    }

    private func scheduleAlertDismissal() {
        alertDismissTask?.cancel()
        guard !pointerInside else { return }
        alertDismissTask = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(4)) } catch { return }
            self?.dismissAlert()
        }
    }

    private func dismissAlert() {
        alertDismissTask?.cancel()
        presentedAlert = nil
        if !pendingAlerts.isEmpty {
            let next = pendingAlerts.removeFirst()
            present(next)
        } else {
            if !pointerInside { machine = HoverMachine() }
            apply(animated: true)
        }
    }

    private func openAlert() {
        guard let alert = activeAlert else { return }
        if case .quota(let event) = alert, store.rows.contains(where: { $0.id == event.agent.id }) {
            store.selectedQuotaId = event.agent.id
        }
        dismissAlert()
        onOpenStats?()
    }

    private var config: HoverMachine.Config {
        HoverMachine.Config(hoverDelay: settings.settings.hoverDelay, collapseDelay: settings.settings.collapseDelay)
    }

    private func transition(_ transition: HoverMachine.Transition) {
        let wasOpen = machine.isOpen
        machine = transition.machine
        timer?.invalidate()
        timer = nil
        if let deadline = transition.deadline {
            let interval = max(0.001, deadline.timeIntervalSinceNow)
            timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: false) { [weak self] _ in
                Task { @MainActor in self?.timerFired() }
            }
        }
        if wasOpen != machine.isOpen { apply(animated: true) }
    }

    private func timerFired() {
        transition(machine.reduce(.timerFired(at: Date()), config: config))
    }

    // MARK: Layout

    /// Resizes the open panel to its content (rows come and go as windows are discovered).
    private func updatePanelHeight(_ height: CGFloat) {
        let clamped = max(80, min(height.rounded(), geometry.screenFrame.height - 80))
        if showsAlertDetails {
            guard abs(clamped - alertDetailHeight) >= 1 else { return }
            alertDetailHeight = clamped
            apply(animated: true)
            return
        }
        guard clamped > 0, abs(clamped - panelHeight) >= 1 else { return }
        panelHeight = clamped
        if machine.isOpen { apply(animated: true) }
    }

    func relayout() {
        geometry = NotchGeometry.detect()
        apply(animated: false)
    }

    func apply(animated: Bool) {
        let open = machine.isOpen
        let animated = animated && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        var root = IslandRootView(
            store: store,
            isOpen: open,
            collapsedSize: geometry.islandFrame.size,
            collapsedTopRadius: NotchGeometry.collapsedTopRadius,
            collapsedBottomRadius: geometry.cornerRadius,
            lightBorder: systemIsLight,
            onOpenStats: { [weak self] in self?.onOpenStats?() },
            onOpenSettings: { [weak self] in self?.onOpenSettings?() },
            alert: activeAlert,
            onOpenAlert: { [weak self] in self?.openAlert() },
            showsAlertDetails: showsAlertDetails,
            animatesGeometry: animated
        )
        if open {
            let height = max(80, min(island.contentHeight(for: root).rounded(), geometry.screenFrame.height - 80))
            if showsAlertDetails { alertDetailHeight = height }
            else { panelHeight = height }
        }
        let expanded = open || activeAlert != nil
        let compactSize = CGSize(width: geometry.rect.width + 2 * (Self.alertWingWidth + Self.alertSidePadding),
                                 height: max(38, geometry.rect.height))
        let size = open ? (showsAlertDetails ? CGSize(width: Self.alertDetailWidth, height: alertDetailHeight) : expandedSize) : compactSize
        // Core frame drives the glow/shadow; the window frame adds the flared top corners.
        let islandFrame = expanded ? geometry.expandedFrame(size: size) : geometry.rect
        let flare = open ? NotchGeometry.expandedTopRadius : NotchGeometry.collapsedTopRadius
        let windowFrame = expanded ? islandFrame.insetBy(dx: -flare, dy: 0) : geometry.islandFrame
        let radius = open ? Self.expandedRadius : max(geometry.cornerRadius, activeAlert == nil ? 0 : 14)
        let current = settings.settings
        let glowGeometry = GlowGeometry.compute(
            islandWidth: islandFrame.width,
            islandHeight: islandFrame.height,
            islandRadius: radius,
            range: current.glowRange,
            blur: current.glowBlur
        )
        let appearance = store.glowAppearance(light: systemIsLight)

        if !animated || targetWindowFrame != windowFrame {
            shrinkTask?.cancel()
            targetWindowFrame = windowFrame
            if animated {
                // Keep a canvas large enough for both shapes while the sides and bottom move independently.
                let width = max(island.panel.frame.width, windowFrame.width)
                let height = max(island.panel.frame.height, windowFrame.height)
                island.setFrame(CGRect(x: geometry.centerX - width / 2, y: geometry.top - height, width: width, height: height))
                island.setVisibleSize(windowFrame.size)
                shrinkTask = Task { [weak self] in
                    do { try await Task.sleep(for: .seconds(IslandAnimation.duration)) } catch { return }
                    self?.island.setFrame(windowFrame)
                    self?.shrinkTask = nil
                }
            } else {
                shrinkTask = nil
                island.setFrame(windowFrame)
                island.setVisibleSize(windowFrame.size)
            }
        }
        // Render changed bitmaps before starting SwiftUI; expensive blur work must not consume animation frames.
        glow.update(
            geometry: geometry,
            island: islandFrame,
            islandRadius: radius,
            glow: glowGeometry,
            outwardOnly: current.glowOutwardOnly,
            appearance: appearance,
            animated: animated,
            alert: activeAlert,
            quotaVendors: store.rows.filter { $0.level != nil }.map { $0.agent.vendor }
        )
        root.presentationSize = windowFrame.size
        root.onContentHeight = { [weak self] height in self?.updatePanelHeight(height) }
        island.setRootView(root)
    }
}

enum SystemAppearance {
    /// The menu bar follows the system setting even when the app forces its own appearance.
    static var isLight: Bool {
        UserDefaults.standard.string(forKey: "AppleInterfaceStyle")?.lowercased() != "dark"
    }
}
