import AppKit
import AgentHUDCore

/// One screen's HUD: its glow window, its island window and its own hover state machine.
///
/// Everything here belongs to a single display, because two displays can be in different modes, be hovered
/// independently and show different things. What is shared — the store, the settings, the system appearance,
/// the pointer — is handed down by `IslandController`, which owns one of these per screen.
@MainActor
final class ScreenHUD {
    /// Height of the open panel; follows the content reported by `IslandRootView`.
    private var panelHeight: CGFloat = IslandController.defaultPanelHeight
    private var alertDetailHeight: CGFloat = 300
    /// The tallest the open card has been while the pointer has stayed on it; zero once it leaves.
    private var alertHoverFloor: CGFloat = 0
    private var expandedSize: CGSize { CGSize(width: IslandController.expandedWidth, height: panelHeight) }

    /// The display this HUD lives on, looked up again each time: `NSScreen` instances are replaced when
    /// displays change, while the key outlives them.
    let key: String
    private let store: UsageStore
    private let settings: SettingsStore
    private(set) var geometry: NotchGeometry
    /// Set by the coordinator, which watches the system appearance once for every screen.
    var systemIsLight = SystemAppearance.isLight
    let glow: GlowWindowController
    let island: IslandWindowController
    private var machine = HoverMachine()
    private var timer: Timer?
    private var shrinkTask: Task<Void, Never>?
    private var targetWindowFrame: CGRect?
    /// The island's own shape while an event is showing: wider than the silhouette by the two wings it grew.
    private var alertFrame: CGRect?
    private let alerts = IslandAlertQueue()
    private var activeAlert: IslandAlert? { alerts.current?.alert }
    private var showsAlertDetails: Bool { machine.isOpen && alerts.current?.inUsagePanel == false }
    private var pointerInside = false
    /// Whether the hover currently counts as one that opens the panel; see `reevaluateHover`.
    private var hoverOpens = false
    /// The user is typing into the island. It stays open under their hands, wherever the pointer goes, until they stop.
    private var typing = false
    private var modifierWatch: Timer?

    var onOpenStats: (() -> Void)?
    var onOpenSettings: (() -> Void)?

    init(key: String, screen: NSScreen?, store: UsageStore, settings: SettingsStore) {
        self.key = key
        self.store = store
        self.settings = settings
        // The stored placement decides notch or queue before the first frame, so the HUD never flashes
        // the wrong shape on launch.
        let placement = screen.map { ScreenIdentity.placement(for: $0, in: settings.settings) }
            ?? .default(hasNotch: false)
        let geometry = NotchGeometry.detect(screen: screen, placement: placement)
        self.geometry = geometry
        glow = GlowWindowController(geometry: geometry)
        island = IslandWindowController(frame: geometry.islandFrame, rootView: IslandRootView.placeholder)
        island.onPointerChange = { [weak self] inside in
            self?.pointer(inside: inside)
        }
        alerts.onExpire = { [weak self] in self?.dismissAlert() }

        apply(animated: false)
        island.show()
    }

    // MARK: Hover

    func pointer(inside: Bool) {
        guard pointerInside != inside else { return }
        pointerInside = inside
        alerts.hold(inside)
        // Whether Option is down can change without the pointer moving, so while it is over the HUD the
        // modifier is watched. A global keyboard monitor would ask for accessibility; this does not.
        modifierWatch?.invalidate()
        modifierWatch = nil
        if inside, settings.settings.requiresOptionToOpen {
            modifierWatch = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
                Task { @MainActor in self?.reevaluateHover() }
            }
        }
        reevaluateHover()
    }

    /// A collapsed logo queue must not swallow clicks: it sits over the menu bar and whatever window is
    /// under it, and nothing about a row of marks says "target". The panel stops taking mouse events, which
    /// also costs it its tracking, so the pointer is followed with an event monitor instead. An open panel
    /// has buttons and takes its events back.
    private func updateClickThrough(_ passes: Bool) {
        guard island.panel.ignoresMouseEvents != passes else { return }
        island.panel.ignoresMouseEvents = passes
    }

    /// The region that counts as hovering: the marks while collapsed, the wings an event grew, the panel once it is
    /// open. The target frame rather than the window's, which is briefly grown into a canvas for the opening
    /// animation. An event is the only thing on screen at that moment, so everything it draws is part of it — a
    /// reminder you cannot point at is a reminder you cannot answer.
    func samplePointer() {
        pointer(inside: hoverRegion.contains(NSEvent.mouseLocation))
    }

    private var hoverRegion: CGRect {
        ScreenHUD.hoverRegion(open: machine.isOpen, panel: targetWindowFrame ?? island.panel.frame,
                              alert: alertFrame, marks: geometry.rect)
    }

    /// Which shape the pointer has to be inside to count as hovering this HUD.
    static func hoverRegion(open: Bool, panel: CGRect, alert: CGRect?, marks: CGRect) -> CGRect {
        if open { return panel }
        return alert ?? marks
    }

    /// Hovering opens the panel, unless the user asked for Option as well. Typing keeps it open either way.
    private func reevaluateHover() {
        let opens = typing || pointerInside
            && (!settings.settings.requiresOptionToOpen || NSEvent.modifierFlags.contains(.option))
        guard opens != hoverOpens else { return }
        hoverOpens = opens
        let now = Date()
        transition(machine.reduce(opens ? .pointerEntered(at: now) : .pointerExited(at: now), config: config))
    }

    func forceOpen() {
        transition(machine.reduce(.forceOpen, config: config))
    }

    func forceCollapse() {
        transition(machine.reduce(.forceCollapse, config: config))
    }

    // MARK: Quota events

    func present(_ alert: QuotaAlert) { present(.quota(alert)) }

    /// `inUsagePanel` is passed on when one request hands over to the next: the surface the user is looking at is
    /// theirs, and answering a card must not move the queue into the usage panel underneath it.
    func present(_ alert: IslandAlert, inUsagePanel: Bool? = nil) {
        // A hidden or paused glow silences news. A client waiting for an answer is not news: it is a question that
        // was asked of this user, and hiding it would leave the session stuck with nobody knowing why.
        let silenced = (store.glowHidden || store.isPaused) && !alert.isPersistent
        guard !silenced, alerts.show(alert, inUsagePanel: inUsagePanel ?? machine.isOpen) else { return }
        // An event owns the brief expansion; a pending hover must not open the full panel underneath it.
        timer?.invalidate()
        timer = nil
        if !machine.isOpen { machine = HoverMachine() }
        if hoverOpens {
            transition(machine.reduce(.pointerEntered(at: Date()), config: config))
        }
        apply(animated: true)
        island.show()
    }

    /// The client withdrew its request: it timed out, it was answered in the terminal, or it was killed. Nothing is
    /// answered on the user's behalf — the card simply stops being a question.
    func withdraw(requestID: String) {
        let surface = alerts.current?.inUsagePanel
        let wasShowing = activeAlert?.id == requestID
        let outcome = alerts.remove(id: requestID)
        guard outcome.removed else { return }
        if wasShowing { stopTyping() }
        if let next = outcome.next {
            present(next, inUsagePanel: surface)
        } else if alerts.current == nil {
            closeAfterLastAlert(wasInUsagePanel: surface ?? false)
        }
    }

    /// Brings a stacked request to the front, so the buttons act on the card the user is looking at.
    private func selectRequest(_ id: String) {
        guard alerts.promote(id: id) else { return }
        apply(animated: true)
    }

    /// Hands the user's answer to the client that is waiting for it, and takes the card off the island.
    private func decideAlert(_ decision: PermissionDecision) {
        guard case .permission(let request)? = activeAlert else { return }
        stopTyping()
        PermissionRequests.shared.resolve(request.id, decision)
    }

    private func setTyping(_ typing: Bool) {
        guard self.typing != typing else { return }
        self.typing = typing
        reevaluateHover()
    }

    /// The card being typed into is gone: the keyboard goes back to the app it came from.
    private func stopTyping() {
        island.panel.releaseKeyboard()
        setTyping(false)
    }

    private func dismissAlert() {
        let surface = alerts.current?.inUsagePanel
        if let next = alerts.dismiss() {
            present(next, inUsagePanel: surface)
        } else {
            closeAfterLastAlert(wasInUsagePanel: surface ?? false)
        }
    }

    /// What the island does once the last card is gone. A card the user was reading in place of the panel takes the
    /// island back to where it was before it arrived: the pointer is on a button that said Deny, not on one asking
    /// for the usage panel, and sliding the panel under it would answer a question nobody put. A card that was a row
    /// inside the panel leaves the panel exactly where it was.
    private func closeAfterLastAlert(wasInUsagePanel: Bool) {
        if ScreenHUD.closesAfterLastAlert(wasInUsagePanel: wasInUsagePanel, pointerInside: pointerInside) {
            machine = HoverMachine()
            timer?.invalidate()
            timer = nil
        }
        apply(animated: true)
    }

    /// Whether the island collapses once the last card is answered.
    static func closesAfterLastAlert(wasInUsagePanel: Bool, pointerInside: Bool) -> Bool {
        !wasInUsagePanel || !pointerInside
    }

    /// Opens the statistics window on what the card was about: a finished turn's session, or a quota event's window.
    private func openAlert() {
        guard let alert = activeAlert else { return }
        store.focusedSessionID = nil
        switch alert {
        case .quota(let event) where store.rows.contains(where: { $0.id == event.agent.id }):
            store.selectedQuotaId = event.agent.id
        case .completion(let event) where store.sessions.contains(where: { $0.id == event.sessionID }):
            store.focusedSessionID = event.sessionID
        default: store.statsTab = .tokens
        }
        dismissAlert()
        handOff { onOpenStats?() }
    }

    /// A click that opens another window takes the HUD down first: the panel floats above every window and stays open
    /// while the pointer rests on it, so the window it opened would appear underneath it. The pointer has to leave and
    /// come back to open it again.
    private func handOff(_ open: () -> Void) {
        forceCollapse()
        open()
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
        if wasOpen != machine.isOpen {
            // The panel opening is someone looking at the numbers, which is reason enough to read the accounts again.
            if machine.isOpen { Task { await store.refreshAccounts() } }
            apply(animated: true)
        }
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

    /// A card grows to its content freely and shrinks only as far as the pointer allows.
    ///
    /// Opening a shorter request, or answering one and losing its row, makes the card shorter than the pointer that
    /// asked for it: the pointer ends up below the card it is still using, which reads as having left the HUD, and
    /// the island closes under the user's hand. The card keeps its height until the pointer is no longer standing
    /// in the part that would be taken away.
    /// The window's height while a card is open: the card's own, or the tallest the card has been for as long as
    /// the pointer has stayed on it. The difference is transparent — the black shape is drawn at the card's size —
    /// so a shorter request opening under the pointer leaves a surface beneath it rather than a black band, and
    /// the window returns to the card's height the moment the pointer leaves.
    static func heldWindowHeight(card: CGFloat, floor: CGFloat, pointerInside: Bool) -> CGFloat {
        pointerInside ? max(card, floor) : card
    }

    /// This HUD's own display, or nothing once it has been unplugged.
    var screen: NSScreen? {
        NSScreen.screens.first { ScreenIdentity.key(for: $0) == key }
    }

    private var placement: ScreenPlacement {
        screen.map { ScreenIdentity.placement(for: $0, in: settings.settings) } ?? .default(hasNotch: false)
    }

    /// The marks this screen shows: the watched agents and anything else run in the last day.
    private var queueItems: [LogoQueueItem] {
        LogoQueueItem.queue(rows: store.queueVendors)
    }

    /// Logo mode sizes the strip from the queue it has to hold.
    private func resolveGeometry() -> NotchGeometry {
        let screen = screen
        let placement = placement
        guard placement.mode == .logos else { return NotchGeometry.detect(screen: screen, placement: placement) }
        let config = LogoQueueConfig(items: queueItems, placement: placement, settings: settings.settings)
        // A queue with nothing in it has no strip to park; the screen falls back to its notch shape.
        guard !config.items.isEmpty else {
            return NotchGeometry.detect(screen: screen, placement: .default(hasNotch: geometry.hasNotch))
        }
        return NotchGeometry.detect(screen: screen, placement: placement, queue: config.size)
    }

    private var logoQueue: LogoQueueConfig? {
        // The geometry still measures the queue when the marks are hidden, so the backdrop keeps the place
        // and the width it had; only the drawing stops.
        guard geometry.mode == .logos, placement.showsLogos else { return nil }
        let config = LogoQueueConfig(items: queueItems, placement: placement, settings: settings.settings)
        return config.items.isEmpty ? nil : config
    }

    /// Takes this HUD's windows off screen; the display it belonged to is gone.
    func close() {
        modifierWatch?.invalidate()
        timer?.invalidate()
        shrinkTask?.cancel()
        island.panel.orderOut(nil)
        glow.panel.orderOut(nil)
    }

    func apply(animated: Bool) {
        let open = machine.isOpen
        geometry = resolveGeometry()
        let animated = animated && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        var root = IslandRootView(
            store: store,
            isOpen: open,
            collapsedSize: geometry.islandFrame.size,
            collapsedTopRadius: NotchGeometry.collapsedTopRadius,
            collapsedBottomRadius: geometry.cornerRadius,
            lightBorder: systemIsLight,
            onOpenStats: { [weak self] in self?.handOff { self?.onOpenStats?() } },
            onOpenSettings: { [weak self] in self?.handOff { self?.onOpenSettings?() } },
            alert: activeAlert,
            onOpenAlert: { [weak self] in self?.openAlert() },
            onDecideAlert: { [weak self] decision in self?.decideAlert(decision) },
            waitingRequests: PermissionRequests.shared.pending,
            onSelectRequest: { [weak self] id in self?.selectRequest(id) },
            onTyping: { [weak self] typing in self?.setTyping(typing) },
            showsAlertDetails: showsAlertDetails,
            animatesGeometry: animated
        )
        root.logoQueue = logoQueue
        // The mode decides the silhouette, not whether there are marks to draw.
        root.hidesSilhouette = geometry.mode == .logos
        if open {
            let height = max(80, min(island.contentHeight(for: root).rounded(), geometry.screenFrame.height - 80))
            if showsAlertDetails { alertDetailHeight = height }
            else { panelHeight = height }
        }
        let expanded = open || activeAlert != nil
        let compactSize = CGSize(width: geometry.rect.width + 2 * (IslandController.alertWingWidth + IslandController.alertSidePadding),
                                 height: max(38, geometry.rect.height))
        let size = open
            ? (showsAlertDetails
                ? CGSize(width: activeAlert?.detailWidth ?? IslandController.alertDetailWidth,
                         height: alertDetailHeight)
                : expandedSize)
            : compactSize
        // Core frame drives the glow/shadow; the window frame adds the flared top corners.
        let islandFrame = expanded ? geometry.expandedFrame(size: size) : geometry.rect
        alertFrame = (activeAlert != nil && !open) ? islandFrame : nil
        let flare = open ? NotchGeometry.expandedTopRadius : NotchGeometry.collapsedTopRadius
        var windowFrame = expanded ? islandFrame.insetBy(dx: -flare, dy: 0) : geometry.islandFrame
        // What the black shape fills, before the window is stretched to keep a surface under the pointer.
        let presentation = windowFrame.size
        if showsAlertDetails {
            alertHoverFloor = pointerInside ? max(alertHoverFloor, windowFrame.height) : 0
            let held = ScreenHUD.heldWindowHeight(card: windowFrame.height, floor: alertHoverFloor,
                                                  pointerInside: pointerInside)
            windowFrame.origin.y -= held - windowFrame.height
            windowFrame.size.height = held
        } else {
            alertHoverFloor = 0
        }
        let radius = open ? IslandController.expandedRadius : max(geometry.cornerRadius, activeAlert == nil ? 0 : 14)
        let current = settings.settings
        // This screen's own glow, or the default when it has not been given one.
        let glowSettings = current.glow(on: key)
        // The glow style is the HUD's backdrop in both modes, but the shape it radiates from differs. The
        // notch is a small silhouette, so the field reads as a rim around it. A logo queue wants a curtain
        // exactly as wide as the marks: the shape is a flat lip at the screen's top edge, run wider than the
        // queue so every cell's nearest point is straight above it and the field falls vertically. The glow
        // panel then clips that field back to the queue's own column, cutting off the ends that would dip.
        let backdrop = geometry.mode == .logos && !expanded
        // Only a silhouette is worth rimming. A logo queue has none — its glow is the backdrop behind the
        // marks — so once the panel or an event has grown over the place that field belonged, it stops
        // rather than following the new shape around.
        let drawsGlow = geometry.mode != .logos || backdrop
        let overhang = GlowWindowController.backdropOverhang(glowSettings)
        // The lip is a flat line on the screen's top edge, run wider than the queue: every cell's nearest
        // point is then straight above it, so the field falls vertically instead of curling in at the ends,
        // and the marks sit inside the field rather than below where it starts.
        let glowIsland = backdrop
            ? CGRect(x: geometry.rect.minX - overhang, y: geometry.screenFrame.maxY,
                     width: geometry.rect.width + overhang * 2, height: 2)
            : islandFrame
        let glowRadius = backdrop ? 0 : radius
        let glowGeometry = glowSettings
            .geometry(islandWidth: glowIsland.width, islandHeight: glowIsland.height, islandRadius: glowRadius)
            .fitted(within: geometry.screenFrame.height)
        let appearance = store.glowAppearance(light: systemIsLight, on: key)

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
            island: glowIsland,
            islandRadius: glowRadius,
            glow: glowGeometry,
            outwardOnly: glowSettings.outwardOnly,
            appearance: appearance,
            animated: animated,
            alert: activeAlert,
            quotaVendors: store.rows.filter { $0.level != nil }.map { $0.agent.vendor },
            pattern: glowSettings.pattern(),
            backdrop: backdrop ? geometry.rect : nil,
            drawsGlow: drawsGlow
        )
        // The strip's place on screen is fixed; the window around it is not, so the offset between them is
        // measured rather than assumed to be the window's own top edge — which moves when the panel opens.
        root.logoQueueInset = max(0, windowFrame.maxY - geometry.rect.maxY)
        root.logoQueueHeight = geometry.rect.height
        updateClickThrough(geometry.mode == .logos && !expanded)
        // One source of truth for the pointer while in logo mode: the panel's own tracking disagrees with the
        // coordinator's monitor about the parts of the window the silhouette does not cover, and the two
        // would fight over the state.
        island.onPointerChange = geometry.mode == .logos
            ? nil
            : { [weak self] inside in self?.pointer(inside: inside) }
        root.presentationSize = presentation
        root.onContentHeight = { [weak self] height in self?.updatePanelHeight(height) }
        island.setRootView(root)
    }
}

