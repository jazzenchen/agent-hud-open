import AppKit
import AgentHUDCore
import SwiftUI

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

    /// The display this HUD lives on, looked up again each time: `NSScreen` instances are replaced when
    /// displays change, while the key outlives them.
    let key: String
    private let store: UsageStore
    private let settings: SettingsStore
    private let openSession: @MainActor (SessionNavigationTarget) async -> Bool
    private var navigationTask: Task<Bool, Never>?
    private var failedNavigationAlertID: String?
    private var failedListedSessionID: String?
    /// One expansion per screen, shared by its live panel and the fresh view used for measuring it.
    private var answeringSessionID: String?
    let mouseLocation: @MainActor () -> CGPoint
    /// Whether the Mac asks for reduced motion, the one place this HUD reads it: whether it animates, whether a separated
    /// dock's card moves and its shadow waits for it (`CardShadowHold`), and what its materials are made for. The
    /// system's setting, or a test's, which runs both ways on any Mac.
    private let reduceMotion: @MainActor () -> Bool
    private let additionalHUDControls: @MainActor (@escaping @MainActor () -> Void) -> AnyView
    private let renderMaterials: @Sendable (GlowWindowController.PreparationInputs) async -> GlowWindowController.PreparedImages
    private(set) var geometry: NotchGeometry
    /// Set by the coordinator, which watches the system appearance once for every screen.
    var systemIsLight = SystemAppearance.isLight
    /// Set by the coordinator while the HUD is recorded: glass is then drawn as its stand-in (`IslandRootView`).
    var drawsGlassStandIn = false
    /// What the director says each mark shows, shared by every screen; nil marks show plain work while their agents work.
    private let directions: MarkDirections?
    /// What this screen's dock remembers of the events its marks were handed.
    private var dock = DockMemory()
    let glow: GlowWindowController
    let island: IslandWindowController
    private(set) var machine = HoverMachine() {
        didSet { if !machine.isOpen { pinned = false } }
    }
    /// On a dock that separates its agents, hovering only peeks; a click asks for the agent's whole panel,
    /// which lasts until the HUD closes.
    var pinned = false
    /// What this screen's panel follows of an agent's hold on it (`ScreenHUD+Hold.swift`).
    var panelHold = PanelHoldState()
    /// The holds agents' edits put on the screens' panels, which this screen's panel follows.
    var panelHolds: PanelHolds { settings.panelLayout.holds }
    /// Set once the display is gone and the HUD's windows are off screen; a hold opens nothing then.
    private(set) var isClosed = false
    /// A detached card moves inside a window that waits for it: it takes in the old place and the new one at once, and
    /// shrinks to the new one when this fires.
    private var cardSettle: Task<Void, Never>?
    /// The last pass showed a detached card, whose way back into its dock moves as its way out did.
    private var cardWasDetached = false
    /// One owner for the next surface's measurements and bitmaps, including opening and collapse.
    private var materialPreparation: MaterialPreparation? {
        didSet { oldValue?.task?.cancel() }
    }
    private var timer: Timer?
    private var targetWindowFrame: CGRect?
    /// The card whose geometry is still interpolating; canvas-only changes share its completion.
    private var geometryTransitionFrame: CGRect?
    /// A detached card's shadow, held back until the card has arrived.
    private let cardShadow = CardShadowHold()
    /// The shells the glow was last given shadows for.
    private(set) var glowShells: HUDShells?
    /// The island's own shape while an event is showing: wider than the silhouette by the two wings it grew.
    private var alertFrame: CGRect?
    private let alerts = IslandAlertQueue()
    var activeAlert: IslandAlert? {
        guard let alert = alerts.current?.alert else { return nil }
        // A queued card keeps its reply, but the source's latest reading owns where its session now lives.
        if case .completion(var event) = alert,
           let current = store.report?.completions.first(where: { $0.id == event.id }) {
            event.navigationTarget = current.navigationTarget
            return .completion(event)
        }
        return alert
    }
    private var sessionNavigationFailed: Bool {
        failedNavigationAlertID != nil && failedNavigationAlertID == activeAlert?.id
    }
    private var showsAlertDetails: Bool { machine.isOpen && alerts.current?.inUsagePanel == false }
    private(set) var pointerInside = false
    /// The hover region the pointer was last found inside; nil once it is found outside. See `hovers`.
    private var heldHoverRegion: CGRect?
    /// Where the pointer was when the HUD last looked at it (`samplePointer()`), which tells the pointer moving onto the
    /// HUD from the HUD opening or growing under a pointer at rest; nil before the first look.
    private var sampledPointer: CGPoint?
    /// Whether the hover currently counts as one that opens the panel; see `reevaluateHover`.
    private var hoverOpens = false
    /// The user is typing into the island. It stays open under their hands, wherever the pointer goes, until they stop.
    private var typing = false
    private var modifierWatch: Timer?
    private let dragSurface = HUDDragWindowController()
    /// A drag is previewed without writing preferences on every mouse movement; mouse-up commits once.
    private(set) var previewPlacement: ScreenPlacement?
    /// The grip along the strip, kept when the queue turns onto another edge.
    private var dragGrabFraction: CGFloat = 0.5
    /// The mark the pointer was last over, on a dock that separates its agents.
    var hoveredMark: String?

    /// One agent's mark on the dock: the vendor it is drawn for, and its centre along the edge on screen.
    struct AgentMark: Equatable {
        let vendor: String
        let anchor: CGFloat
        var key: String { AgentArtwork.markKey(vendor) }
    }

    /// Every mark on a dock, which an event's card grows out of whether or not the dock separates its agents. None on
    /// the notch, which draws no marks.
    var dockMarks: [AgentMark] {
        guard geometry.mode.isDocked else { return [] }
        let centres = simpleMeter?.markCentres ?? logoQueue?.markCentres ?? []
        return Self.agentMarks(centres: centres, strip: geometry.rect, edge: geometry.edge)
    }

    /// The marks hovering opens an agent's own card from; none unless this dock separates its agents.
    var agentMarks: [AgentMark] { placement.separatesAgents ? dockMarks : [] }

    /// Puts each mark's place along its strip on screen. A side strip is read top to bottom, while the
    /// screen's own coordinates run upward.
    static func agentMarks(centres: [(vendor: String, along: CGFloat)], strip: CGRect, edge: HUDEdge) -> [AgentMark] {
        centres.map { AgentMark(vendor: $0.vendor, anchor: edge.isHorizontal ? strip.minX + $0.along : strip.maxY - $0.along) }
    }

    /// The mark nearest a pointer that is on the strip; nil while it is anywhere else.
    static func mark(at point: CGPoint, among marks: [AgentMark], strip: CGRect, edge: HUDEdge) -> AgentMark? {
        guard containsPointer(point, in: strip) else { return nil }
        let along = edge.isHorizontal ? point.x : point.y
        return marks.min { abs($0.anchor - along) < abs($1.anchor - along) }
    }

    /// The agent the card is for: the sender of the event on show, otherwise the hovered mark. Nil shows the whole HUD.
    ///
    /// A dock never gives its marks up to an event, as the notch's island gives itself up: the event's card grows out
    /// of its sender's mark, and every mark stays. Only an event the open usage panel holds as one of its rows has no
    /// card of its own.
    private var focus: AgentMark? {
        guard previewPlacement == nil else { return nil }
        if let alert = activeAlert {
            guard geometry.mode.isDocked, !(machine.isOpen && alerts.current?.inUsagePanel == true) else { return nil }
            return eventMark(for: alert)
        }
        return agentMarks.first { $0.key == hoveredMark }
    }

    /// Where `alert`'s card grows from on a dock: its sender's mark, or the middle of the dock for an agent with no
    /// mark here.
    private func eventMark(for alert: IslandAlert) -> AgentMark {
        let key = AgentArtwork.markKey(alert.vendor)
        if let mark = dockMarks.first(where: { $0.key == key }) { return mark }
        let strip = geometry.rect
        return AgentMark(vendor: alert.vendor, anchor: geometry.edge.isHorizontal ? strip.midX : strip.midY)
    }

    /// A click on a mark, or on its peek, opens that agent's whole panel. It is the person's choice, so it ends a hold.
    func pinAgent() {
        guard !agentMarks.isEmpty, activeAlert == nil, previewPlacement == nil else { return }
        personTookOver()
        if let mark = Self.mark(at: mouseLocation(), among: agentMarks, strip: geometry.rect, edge: geometry.edge) {
            hoveredMark = mark.key
        }
        guard focus != nil else { return }
        materialPreparation = nil
        if machine.isOpen {
            guard !pinned else { return }
            pinned = true
            apply(animated: false)
        } else {
            forceOpen()
            pinned = true
            apply(animated: false)
        }
        samplePointer()
    }

    /// What hovering a mark shows: a card beside it on a Dynamic dock, one line out of the Simple panel.
    private func peek(for mark: AgentMark) -> AgentPeek {
        var peek = AgentPeek.make(vendor: mark.vendor, store: store)
        peek.edge = geometry.edge
        peek.scale = stripScale
        if geometry.mode == .simple {
            peek.style = .line
            // On a side edge the line starts behind the panel, so it reads as the panel growing.
            peek.tuck = geometry.edge.isHorizontal ? 0 : (geometry.rect.width / 2).rounded()
        }
        return peek
    }

    /// What the dock grows out of itself — the Simple panel's line, the Dynamic dock's bubble, a compact reminder —
    /// is written to this scale: as tall as one of the dock's marks, and never smaller than at the default. Cards
    /// keep their own sizes.
    private var stripScale: CGFloat { IslandRootView.dockScale(mode: geometry.mode, placement: placement) }

    /// The halves of the reminder on show, as its view lays them out on this dock.
    private var compactWings: CompactWings? {
        activeAlert.map { CompactWings.dock(for: $0, waiting: max(1, PermissionRequests.shared.pending.count), scale: stripScale) }
    }

    /// A detached card's place: beside the strip on its inner side, centred on the mark and kept on screen.
    /// A negative gap starts the card behind the strip.
    static func detachedFrame(size: CGSize, anchor: CGFloat, strip: CGRect, edge: HUDEdge, gap: CGFloat,
                              screen: CGRect) -> CGRect {
        // Whole points throughout: a window cannot sit on half a point, and one asked to would be nudged back and
        // forth by every later pass.
        let width = min(size.width.rounded(.up), screen.width), height = min(size.height.rounded(.up), screen.height)
        let x = min(screen.maxX - width, max(screen.minX, (anchor - width / 2).rounded()))
        let y = min(screen.maxY - height, max(screen.minY, (anchor - height / 2).rounded()))
        let gap = gap.rounded()
        switch edge {
        case .top: return CGRect(x: x, y: strip.minY - gap - height, width: width, height: height)
        case .bottom: return CGRect(x: x, y: strip.maxY + gap, width: width, height: height)
        case .left: return CGRect(x: strip.maxX + gap, y: y, width: width, height: height)
        case .right: return CGRect(x: strip.minX - gap - width, y: y, width: width, height: height)
        }
    }

    /// Sliding along the strip moves the open panel from one agent to the next. Opening another agent's card is the
    /// person's choice, so it ends a hold.
    private func followHoveredMark(_ point: CGPoint) {
        guard let mark = Self.mark(at: point, among: agentMarks, strip: geometry.rect, edge: geometry.edge),
              mark.key != hoveredMark else { return }
        if isHeld { endHold() }
        hoveredMark = mark.key
        guard activeAlert == nil else { return }
        if machine.isOpen {
            materialPreparation = nil
            apply(animated: true)
        } else {
            prepareOpening()
        }
    }

    /// Inputs that can change the natural layout while a hover is waiting, including token data that
    /// need not change the coordinator's quota rows or glow appearance.
    private struct PanelInputs: Equatable {
        let report: UsageReport?
        let settings: AgentHUDCore.Settings
        let agents: [AgentDescriptor]
        /// The layout this screen's panel draws, and whether it shows an unsaved draft, which adds the bar that says so.
        /// The layout also says which agents the resting HUD shows, which the appearance below follows.
        let layout: PanelLayout
        let draft: Bool
        let now: Date
        let dataDate: Date
        let error: String?
        let loading: Bool
        let range: StatsRange
        let bucketSize: TokenBucketSize
        let dimensions: TokenDimensions
        let requests: [PermissionRequest]
        let geometry: NotchGeometry
        let light: Bool
        /// The look the island draws with: a change of it makes the panel and its images again.
        let look: HUDLook
        let appearance: GlowAppearance
        let alertID: String?
        let focus: AgentMark?
        let pinned: Bool
        let alertDetails: Bool
        let pendingIDs: [String]
        let sessionNavigationFailed: Bool
        let failedListedSessionID: String?
        let answeringSessionID: String?
        let reduceMotion: Bool
        let colorSpace: CGColorSpace?
    }

    private final class MaterialPreparation {
        let inputs: PanelInputs
        let open: Bool
        let surface: Surface
        let height: CGFloat
        let task: Task<GlowWindowController.PreparedImages?, Never>?
        var images: GlowWindowController.PreparedImages?

        init(inputs: PanelInputs, open: Bool, surface: Surface, height: CGFloat,
             task: Task<GlowWindowController.PreparedImages?, Never>?) {
            self.inputs = inputs
            self.open = open
            self.surface = surface
            self.height = height
            self.task = task
        }
    }

    private var panelInputs: PanelInputs {
        PanelInputs(report: store.report, settings: settings.effectiveSettings, agents: settings.agents,
                      layout: settings.panelLayout.effective(for: key), draft: settings.panelLayout.hasDraft(on: key),
                      now: store.now, dataDate: store.dataDate,
                      error: store.lastError, loading: store.isLoading, range: store.statsRange,
                      bucketSize: store.tokenBucketSize, dimensions: store.tokenDimensions,
                      requests: PermissionRequests.shared.pending, geometry: geometry, light: systemIsLight, look: look,
                      appearance: agents.glowAppearance(light: systemIsLight, on: key),
                      alertID: activeAlert?.id, focus: focus, pinned: pinned, alertDetails: alerts.current?.inUsagePanel == false,
                      pendingIDs: alerts.pendingIDs,
                      sessionNavigationFailed: sessionNavigationFailed, failedListedSessionID: failedListedSessionID,
                      answeringSessionID: answeringSessionID,
                      reduceMotion: reduceMotion(),
                      colorSpace: glow.panel.screen?.colorSpace?.cgColorSpace)
    }

    var onOpenStats: (() -> Void)?
    var onOpenSettings: (() -> Void)?
    /// Asks for a request another screen holds: the waiting list names every request, wherever it arrived.
    var onClaimRequest: ((String) -> Void)?

    init(key: String, screen: NSScreen?, store: UsageStore, settings: SettingsStore,
         mouseLocation: @escaping @MainActor () -> CGPoint = { NSEvent.mouseLocation },
         reduceMotion: @escaping @MainActor () -> Bool = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion },
         openSession: @escaping @MainActor (SessionNavigationTarget) async -> Bool = SessionNavigator.open,
         additionalHUDControls: @escaping @MainActor (@escaping @MainActor () -> Void) -> AnyView = { _ in AnyView(EmptyView()) },
         renderMaterials: @escaping @Sendable (GlowWindowController.PreparationInputs) async -> GlowWindowController.PreparedImages
            = { GlowWindowController.render($0) },
         directions: MarkDirections? = nil) {
        self.key = key
        self.store = store
        self.directions = directions
        self.settings = settings
        self.openSession = openSession
        self.mouseLocation = mouseLocation
        self.reduceMotion = reduceMotion
        self.additionalHUDControls = additionalHUDControls
        self.renderMaterials = renderMaterials
        // The stored placement decides notch or queue before the first frame, so the HUD never flashes
        // the wrong shape on launch. A style draft an agent proposed for this screen is drawn over it.
        let placement = screen.map { ScreenIdentity.placement(for: $0, in: settings) }
            ?? settings.effectivePlacement(on: key, hasNotch: false)
        let geometry = NotchGeometry.detect(screen: screen, placement: placement)
        self.geometry = geometry
        glow = GlowWindowController(geometry: geometry)
        island = IslandWindowController(frame: geometry.islandFrame, rootView: IslandRootView.placeholder)
        island.onPointerChange = { [weak self] _ in self?.samplePointer() }
        alerts.onExpire = { [weak self] in self?.dismissAlert() }
        cardShadow.onReveal = { [weak self] shells, look in self?.revealCardShadow(shells, look: look) }
        dragSurface.onPress = { [weak self] in self?.grabHUD() }
        dragSurface.onRelease = { [weak self] in self?.releaseHUD() }
        dragSurface.onBegin = { [weak self] point in self?.beginMoving(at: point) }
        dragSurface.onDrag = { [weak self] point in self?.move(to: point) }
        dragSurface.onEnd = { [weak self] in self?.finishMoving() }
        dragSurface.onClick = { [weak self] in self?.pinAgent() }

        apply(animated: false)
        island.show()
        followHolds()
    }

    // MARK: Hover

    /// - came: whether the pointer itself moved onto the HUD, from a place the HUD does not cover now, which a hold on
    ///   the panel follows (`followHoldPointer(_:came:)`).
    func pointer(inside: Bool, came: Bool = false) {
        guard previewPlacement == nil else { return }
        if pointerInside != inside {
            pointerInside = inside
            alerts.hold(inside || navigationTask != nil)
            // Whether Option is down can change without the pointer moving, so while it is over the HUD the
            // modifier is watched. A global keyboard monitor would ask for accessibility; this does not.
            modifierWatch?.invalidate()
            modifierWatch = nil
            if inside, settings.effectiveSettings.requiresOptionToOpen {
                modifierWatch = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
                    Task { @MainActor in self?.reevaluateHover() }
                }
            }
        }
        followHoldPointer(inside, came: came)
        // Moving within the HUD can reach the top while an ordinary hover is still waiting to open.
        reevaluateHover()
    }

    /// The bare island canvas passes clicks through. A visible strip has its own small grab surface.
    /// Hover is followed by the coordinator, while an open island takes events for its controls.
    private func updateClickThrough(_ passes: Bool) {
        guard island.panel.ignoresMouseEvents != passes else { return }
        island.panel.ignoresMouseEvents = passes
    }

    /// The region that counts as hovering: the marks while collapsed, the wings an event grew, the panel once it is
    /// open. The target frame rather than the window's, which is briefly grown into a canvas for the opening
    /// animation. An event is the only thing on screen at that moment, so everything it draws is part of it — a
    /// reminder you cannot point at is a reminder you cannot answer.
    func samplePointer() {
        let point = mouseLocation()
        if previewPlacement == nil, !dragSurface.isPressed { followHoveredMark(point) }
        let region = hoverRegion, usagePanelOpen = machine.isOpen && !showsAlertDetails
        let inside = Self.hovers(point, region: region, held: heldHoverRegion, usagePanelOpen: usagePanelOpen)
        // The pointer came onto the HUD where it was last found off what the HUD covers now: a panel that opened or grew
        // under it while it rested, or that it then moves within, it did not come onto.
        let came = inside && sampledPointer.map {
            !Self.hovers($0, region: region, held: heldHoverRegion, usagePanelOpen: usagePanelOpen)
        } == true
        sampledPointer = point
        if Self.containsPointer(point, in: region) { heldHoverRegion = region } else if !inside { heldHoverRegion = nil }
        pointer(inside: inside, came: came)
    }

    /// Whether a pointer counts as on the HUD: inside its hover region, or, while the usage panel is open, still inside
    /// the region it was last found in.
    ///
    /// The panel can turn shorter under the pointer that is using it: discarding a layout takes away what the draft
    /// added, and a row can leave with a refresh. The pointer is then below the panel it never left, which reads as
    /// having left the HUD, and the panel would close under the hand that pressed the button. The place the panel
    /// gave up counts as hovering until the pointer moves out of it. An event's card holds its window instead
    /// (`heldWindowHeight`).
    static func hovers(_ point: CGPoint, region: CGRect, held: CGRect?, usagePanelOpen: Bool) -> Bool {
        if containsPointer(point, in: region) { return true }
        guard usagePanelOpen, let held else { return false }
        return containsPointer(point, in: held)
    }

    /// CGRect.contains excludes its top and right edges. A pointer pinned to a display edge is still
    /// over the HUD, including at the physical notch, so hover includes the target's boundary.
    static func containsPointer(_ point: CGPoint, in region: CGRect) -> Bool {
        !region.isEmpty && point.x >= region.minX && point.x <= region.maxX
            && point.y >= region.minY && point.y <= region.maxY
    }

    /// AppKit's pointer may stop one point short of maxY at a physical display boundary.
    /// Only the HUD's own top edge qualifies; side and bottom queues keep their ordinary hover delay.
    static func pointerTouchesTop(_ point: CGPoint, geometry: NotchGeometry) -> Bool {
        geometry.edge == .top && point.y >= geometry.screenFrame.maxY - 1
            && point.y <= geometry.screenFrame.maxY && containsPointer(point, in: geometry.rect)
    }

    var hoverRegion: CGRect {
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
        guard !dragSurface.isPressed, previewPlacement == nil else { return }
        let opens = ScreenHUD.opensOnHover(counted: hoverOpens, open: machine.isOpen, pointerInside: pointerInside,
                                           typing: typing, requiresOption: settings.effectiveSettings.requiresOptionToOpen,
                                           optionDown: NSEvent.modifierFlags.contains(.option))
        let canFinishOpening = switch machine.state {
        case .opening: true
        // A handoff to Settings or Stats deliberately stays collapsed until the pointer leaves.
        case .collapsed: !hoverOpens
        default: false
        }
        if opens, canFinishOpening, settings.effectiveSettings.openImmediatelyAtTop,
           Self.pointerTouchesTop(mouseLocation(), geometry: geometry) {
            hoverOpens = true
            transition(machine.reduce(.forceOpen, config: config))
            return
        }
        guard opens != hoverOpens else { return }
        hoverOpens = opens
        // A panel a hold keeps open stays open when the hover ends: the hold ending closes it (`letGo()`).
        if !opens, isHeldOpen { return }
        let now = Date()
        transition(machine.reduce(opens ? .pointerEntered(at: now) : .pointerExited(at: now), config: config))
    }

    /// Whether the hover counts as one that opens the panel; `counted` is whether it already does. Option is asked for
    /// only to start it: a tap is enough, and from then on the hover counts until the pointer leaves. An open panel
    /// needs no Option either — it is being read and pointed at, including by a pointer that slipped off the edge and
    /// came back before it closed.
    static func opensOnHover(counted: Bool, open: Bool, pointerInside: Bool, typing: Bool,
                             requiresOption: Bool, optionDown: Bool) -> Bool {
        typing || pointerInside && (counted || open || !requiresOption || optionDown)
    }

    func forceOpen() {
        transition(machine.reduce(.forceOpen, config: config))
    }

    /// Closes the panel at once: a person's click that leads elsewhere, or the host's. It ends a hold first.
    func forceCollapse() {
        endHold()
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
        guard !silenced else { return }
        // A separated dock has no usage panel for a card to become a row of: an event is always its own card.
        guard alerts.show(alert, inUsagePanel: inUsagePanel ?? (machine.isOpen && agentMarks.isEmpty)) else {
            if alerts.contains(id: alert.id) {
                materialPreparation = nil
                apply(animated: true)
            }
            return
        }
        // An event owns the brief expansion; a pending hover must not open the full panel underneath it.
        timer?.invalidate()
        timer = nil
        materialPreparation = nil
        if !machine.isOpen { machine = HoverMachine() }
        if hoverOpens {
            transition(machine.reduce(.pointerEntered(at: Date()), config: config))
        }
        // A request cannot be answered from a peek, so on a separated dock it opens as its whole card.
        if case .permission = alert, let focus, agentMarks.contains(focus), !machine.isOpen { forceOpen() }
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

    /// Brings a stacked request to the front, so the buttons act on the card the user is looking at. One that arrived
    /// on another screen moves to this one first. Opening another card is the person's choice, so it ends a hold.
    func selectRequest(_ id: String) {
        personTookOver()
        if !alerts.contains(id: id) { onClaimRequest?(id) }
        guard alerts.promote(id: id) else { return }
        stopTyping()
        materialPreparation = nil
        apply(animated: true)
    }

    /// The requests this screen holds, which outlive it when its display goes away.
    var questions: [IslandAlert] { alerts.questions }

    func holds(_ id: String) -> Bool { alerts.contains(id: id) }

    /// Hands a request over to another screen: it leaves this one as a withdrawn one does, still unanswered.
    func take(requestID: String) -> IslandAlert? {
        guard let alert = alerts.questions.first(where: { $0.id == requestID }) else { return nil }
        withdraw(requestID: requestID)
        return alert
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
        updateDragSurface()
        reevaluateHover()
    }

    private func answerQuestions(in sessionID: String?) {
        guard answeringSessionID != sessionID else { return }
        stopTyping()
        answeringSessionID = sessionID
        apply(animated: true)
    }

    /// The card being typed into is gone: the keyboard goes back to the app it came from.
    private func stopTyping() {
        island.panel.releaseKeyboard()
        setTyping(false)
    }

    /// Closing a reply removes that reminder; an unanswered request must stay with its client.
    func dismissCompletionAlert() {
        guard case .completion? = activeAlert else { return }
        cancelSessionNavigation()
        dismissAlert()
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
        // A panel a hold keeps open stays so, and one a hold waited to open opens.
        if ScreenHUD.closesAfterLastAlert(wasInUsagePanel: wasInUsagePanel, pointerInside: pointerInside), !panelHold.open {
            machine = HoverMachine()
            materialPreparation = nil
            timer?.invalidate()
            timer = nil
        }
        apply(animated: true)
        followHoldAfterAlert()
    }

    /// Whether the island collapses once the last card is answered.
    static func closesAfterLastAlert(wasInUsagePanel: Bool, pointerInside: Bool) -> Bool {
        !wasInUsagePanel || !pointerInside
    }

    /// A compact reply opens its choices; quota events still open their usage page. Either is the person's choice, so
    /// it ends a hold.
    func openAlert() {
        guard let alert = activeAlert else { return }
        personTookOver()
        if case .completion = alert {
            forceOpen()
        } else {
            openAlertUsage()
        }
    }

    /// The session action never changes destination to usage. A failed attempt leaves its choices available.
    func openAlertSession() async {
        guard case .completion(let event)? = activeAlert, let target = event.navigationTarget else { return }
        cancelSessionNavigation()
        alerts.hold(true)
        forceCollapse()
        let task = Task { await openSession(target) }
        navigationTask = task
        let opened = await task.value
        guard !task.isCancelled else { return }
        navigationTask = nil
        alerts.hold(pointerInside)
        guard activeAlert?.id == event.id else { return }
        if opened {
            dismissAlert()
        } else {
            failedNavigationAlertID = event.id
            if machine.isOpen { apply(animated: true) } else { forceOpen() }
        }
    }

    /// A listed session follows its source's current destination, independently of its token usage action.
    func openListedSession(_ id: String) async {
        guard let target = store.sessions.first(where: { $0.id == id })?.navigationTarget else { return }
        cancelSessionNavigation()
        alerts.hold(true)
        forceCollapse()
        let task = Task { await openSession(target) }
        navigationTask = task
        let opened = await task.value
        guard !task.isCancelled else { return }
        navigationTask = nil
        alerts.hold(pointerInside)
        if opened {
            forceCollapse()
        } else {
            failedListedSessionID = id
            if machine.isOpen { apply(animated: true) } else { forceOpen() }
        }
    }

    private func cancelSessionNavigation() {
        navigationTask?.cancel()
        navigationTask = nil
        failedNavigationAlertID = nil
        failedListedSessionID = nil
    }

    /// Token usage is a distinct user choice, even when the agent's session cannot be located.
    func openAlertUsage() {
        guard let alert = activeAlert else { return }
        cancelSessionNavigation()
        dismissAlert()
        alerts.hold(pointerInside)
        forceCollapse()
        showAlertUsage(alert)
    }

    private func showAlertUsage(_ alert: IslandAlert) {
        store.focusedSessionID = nil
        switch alert {
        case .quota(let event) where store.rows.contains(where: { $0.id == event.agent.id }):
            store.selectedQuotaId = event.agent.id
        case .completion(let event) where store.sessions.contains(where: { $0.id == event.sessionID }):
            store.focusedSessionID = event.sessionID
        default: store.statsTab = .tokens
        }
        onOpenStats?()
    }

    /// A click that opens another window takes the HUD down first: the panel floats above every window and stays open
    /// while the pointer rests on it, so the window it opened would appear underneath it. The pointer has to leave and
    /// come back to open it again.
    private func handOff(_ open: () -> Void) {
        cancelSessionNavigation()
        alerts.hold(pointerInside)
        forceCollapse()
        open()
    }

    /// The hover's timing, as the app draws its settings, an agent's unsaved draft of them included.
    var config: HoverMachine.Config {
        let settings = settings.effectiveSettings
        return HoverMachine.Config(hoverDelay: settings.hoverDelay, collapseDelay: settings.collapseDelay)
    }

    /// - reads: whether an opening reads the accounts again, which a hover's does and a hold's does not.
    func transition(_ transition: HoverMachine.Transition, reads: Bool = true) {
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
        if case .opening = machine.state {
            prepareOpening()
        } else if !machine.isOpen {
            materialPreparation = nil
            answeringSessionID = nil
        }
        if wasOpen != machine.isOpen {
            // The panel opening is someone looking at the numbers, which is reason enough to read the accounts again. A
            // hold opens it for an agent's edit, which is no such reason.
            if machine.isOpen, reads { Task { await store.refreshAccounts() } }
            apply(animated: true)
        }
    }

    private func timerFired() {
        if case .opening(let deadline) = machine.state {
            // Validate even when a report changed without a coordinator apply. The worker, rather than a
            // synchronous raster fallback, finishes an opening whose delay expired before its images were ready.
            prepareOpening()
            if Date() >= deadline, materialPreparation?.images == nil {
                timer?.invalidate()
                timer = nil
                return
            }
        }
        transition(machine.reduce(.timerFired(at: Date()), config: config))
    }

    // MARK: Layout

    private var maximumContentHeight: CGFloat {
        geometry.screenFrame.height - 80
    }

    /// How this display draws the whole panel, for a picture or a measurement made off screen: the insets its notch or
    /// its dock gives the content, as `IslandRootView` sets them, the tallest the panel opens here, and the HUD's look.
    /// It is worked out from the placement in effect now, a style draft just proposed included, before the HUD has
    /// re-applied it.
    var panelDisplay: PanelPictureRenderer.Display {
        let geometry = resolveGeometry(), look = look
        var insets = Self.panelInsets(on: geometry)
        if !geometry.mode.isDocked {
            insets.top += IslandRootView.inkClearance(inksNotch: geometry.hasNotch, look: look)
        }
        return PanelPictureRenderer.Display(insets: insets, maximumHeight: maximumContentHeight, look: look)
    }

    /// The room the open panel's content keeps from the card's edges on `geometry`, as `IslandRootView` sets it: the
    /// notch's, or a dock's, which keeps its strip clear.
    private static func panelInsets(on geometry: NotchGeometry) -> EdgeInsets {
        geometry.mode.isDocked
            ? IslandRootView.dockInsets(edge: geometry.edge, collapsedSize: geometry.rect.size) : HoverPanelView.notchInsets
    }

    /// The whole panel's width here: its layout's, widened on a side dock to keep the content at its narrowest
    /// (`HoverPanelView.width(_:insets:)`).
    private var panelWidth: CGFloat {
        HoverPanelView.width(CGFloat(panelStyle.panelWidth), insets: Self.panelInsets(on: geometry))
    }

    /// Resizes the open panel to its content (rows come and go as windows are discovered).
    private func updatePanelHeight(_ height: CGFloat) {
        let clamped = max(80, min(height.rounded(), maximumContentHeight))
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

    /// The placement this HUD draws: a drag's preview, else the screen's settings with its style draft drawn over them.
    private var placement: ScreenPlacement {
        previewPlacement ?? screen.map { ScreenIdentity.placement(for: $0, in: settings) }
            ?? settings.effectivePlacement(on: key, hasNotch: false)
    }

    /// What this HUD looks like, which its island, its panel, the cards on it and the pictures made of them draw with:
    /// the look its screen's settings give it, a style draft included, as Settings previews it
    /// (`SettingsStore.effectiveLook(on:)`). It is read afresh on every apply, which `IslandController` asks for with the
    /// HUD's motion when the settings, a style draft, macOS's light or dark setting or Reduce Transparency change it.
    var look: HUDLook { settings.effectiveLook(on: key) }

    /// The island in Notch mode at a hardware notch, whose row keeps the camera housing's dark.
    private var atHardwareNotch: Bool { geometry.mode == .notch && geometry.hasNotch }

    /// The look the island draws with: at a hardware notch the notch row's while it rests there or shows a reminder's
    /// wings, dark in the look's material (`HUDLook.inNotchRow`), and the screen's look once it opens into the panel or
    /// a card; everywhere else the screen's look. Opening and closing cross-fade between the two on the island's curve.
    /// The glow keeps the screen's look, which decides by whether the desktop shows through it, glass or a solid with a
    /// transparency (`HUDLook.seeThrough`), what the glow cuts out and how the shells cast their shadows (`GlowCutout`,
    /// `castsShellShadows`). The row's dark glass lets the desktop through as the screen's glass does; its opaque black
    /// under a see-through Solid hides the glow beneath it whether that is cut out or not.
    private func islandLook(open: Bool) -> HUDLook {
        atHardwareNotch && !open ? look.inNotchRow : look
    }

    /// The panel's style in this screen's layout, whose width the open panel takes: the whole panel's, or on a dock
    /// that separates its agents one agent's own, 100 points narrower.
    private var panelStyle: PanelStyle { settings.panelLayout.effective(for: key).style }

    // MARK: Repositioning

    private func updateDragSurface() {
        let visibleHUD = geometry.mode == .notch || logoQueue != nil || simpleMeter != nil
        guard !typing, visibleHUD || previewPlacement != nil || dragSurface.isPressed else {
            dragSurface.hide()
            return
        }
        let outline = simpleMeter.map { Self.dragOutline(for: geometry, panelRadius: $0.cornerRadius) }
        dragSurface.show(frame: Self.dragSurfaceFrame(for: geometry, outline: outline?.rect),
                         outlined: previewPlacement != nil || dragSurface.isArmed, outline: outline)
    }

    /// Clear space between the Simple panel and the dashes drawn around it.
    private static let outlineGap: CGFloat = 2

    /// The Simple panel's own shape, a little larger: its strip without the edge margin, with the panel's corner.
    static func dragOutline(for geometry: NotchGeometry, panelRadius: CGFloat) -> (rect: CGRect, radius: CGFloat) {
        let panel = lifted(geometry.rect, on: geometry.edge)
        return (panel.insetBy(dx: -outlineGap, dy: -outlineGap), panelRadius + outlineGap)
    }

    /// The dashed bounds leave room at both ends of the queue without moving its marks or backdrop.
    /// A panel with an outline of its own is grabbed by that outline, with room for its stroke.
    static func dragSurfaceFrame(for geometry: NotchGeometry, outline: CGRect? = nil) -> CGRect {
        if let outline { return outline.insetBy(dx: -2, dy: -2).intersection(geometry.screenFrame) }
        var frame = geometry.rect.insetBy(dx: geometry.edge.isHorizontal ? -8 : 0,
                                         dy: geometry.edge.isHorizontal ? 0 : -8)
        // The physical notch cannot be clicked. A little space beneath it makes its move surface reachable.
        if geometry.mode == .notch { frame.origin.y -= 14; frame.size.height += 14 }
        return frame.intersection(geometry.screenFrame)
    }

    private func grabHUD() {
        materialPreparation = nil
        timer?.invalidate()
        timer = nil
        if !machine.isOpen { machine = HoverMachine(); hoverOpens = false }
        updateDragSurface()
    }

    private func releaseHUD() {
        updateDragSurface()
        if previewPlacement == nil { samplePointer() }
    }

    func beginMoving(at point: CGPoint? = nil) {
        // A person moving the HUD wins over an agent's style draft and its hold: both are dropped first, and the move
        // starts from the placement the screen has saved.
        endHold()
        settings.panelLayout.discardStyle(key)
        materialPreparation = nil
        if let point {
            let run = geometry.edge.isHorizontal ? geometry.rect.width : geometry.rect.height
            let grip = geometry.edge.isHorizontal ? point.x - geometry.rect.minX : geometry.rect.maxY - point.y
            dragGrabFraction = min(1, max(0, grip / run))
        } else {
            dragGrabFraction = 0.5
        }
        previewPlacement = placement
        if previewPlacement?.mode == .notch { previewPlacement?.mode = .logos }
        timer?.invalidate()
        timer = nil
        targetWindowFrame = nil
        geometryTransitionFrame = nil
        machine = HoverMachine()
        hoverOpens = false
        alerts.hold(true)
        apply(animated: false)
    }

    func move(to point: CGPoint) {
        guard let current = previewPlacement else { return }
        previewPlacement = NotchGeometry.dragPlacement(at: point, screenFrame: geometry.screenFrame,
                                                       queue: dockSize(for: current), placement: current,
                                                       grabFraction: dragGrabFraction)
        apply(animated: false)
    }

    func finishMoving() {
        guard let next = previewPlacement else { return }
        settings.updateScreen(key) { $0.screens[key] = next }
        previewPlacement = nil
        // Dropping the HUD is an explicit repositioning, not a request to open its panel under the pointer.
        hoverOpens = true
        apply(animated: false)
        pointerInside = Self.containsPointer(mouseLocation(), in: hoverRegion)
        alerts.hold(pointerInside)
    }

    private func queueSize(_ config: LogoQueueConfig) -> CGSize {
        guard config.items.isEmpty else { return config.size }
        let run = max(32, config.logo)
        return config.edge.isHorizontal ? CGSize(width: run, height: config.stripThickness) : CGSize(width: config.stripThickness, height: run)
    }

    /// What this screen's resting HUD shows of the agents, under the agents the layout it draws lists.
    var agents: ScreenAgents {
        ScreenAgents(store: store, listed: settings.panelLayout.effective(for: key).hud.agents, mode: placement.mode,
                     directions: directions?.current ?? [:])
    }

    /// The marks this screen shows: the watched agents and anything else run in the last day, as its layout lists them.
    private var queueItems: [LogoQueueItem] { agents.queueItems }

    /// A docked mode sizes its strip from what it has to hold.
    private func resolveGeometry() -> NotchGeometry {
        let screen = screen
        let placement = placement
        guard placement.mode.isDocked else { return NotchGeometry.detect(screen: screen, placement: placement) }
        return NotchGeometry.detect(screen: screen, placement: placement, queue: dockSize(for: placement))
    }

    /// What a docked HUD takes along its edge: the logo queue, or the Simple panel with its margin.
    private func dockSize(for placement: ScreenPlacement) -> CGSize {
        if placement.mode == .simple { return SimpleMeterConfig(items: meterItems, placement: placement).stripSize }
        return queueSize(LogoQueueConfig(items: queueItems, placement: placement, settings: settings.effectiveSettings))
    }

    /// The agents with a quota row, each with its tightest window, then those with only a balance, as its layout lists
    /// them.
    private var meterItems: [SimpleMeterItem] { agents.meterItems }

    private var simpleMeter: SimpleMeterConfig? {
        guard geometry.mode == .simple else { return nil }
        return SimpleMeterConfig(items: meterItems, placement: placement, bots: settings.effectiveSettings.botLogos)
    }

    private var logoQueue: LogoQueueConfig? {
        // The geometry still measures the queue when the marks are hidden, so the backdrop keeps the place
        // and the width it had; only the drawing stops.
        guard geometry.mode == .logos, placement.showsLogos else { return nil }
        let config = LogoQueueConfig(items: queueItems, placement: placement, settings: settings.effectiveSettings)
        return config.items.isEmpty ? nil : config
    }

    /// Takes this HUD's windows off screen; the display it belonged to is gone.
    func close() {
        isClosed = true
        cancelSessionNavigation()
        materialPreparation = nil
        modifierWatch?.invalidate()
        timer?.invalidate()
        cardSettle?.cancel()
        cardSettle = nil
        cardShadow.cancel()
        targetWindowFrame = nil
        geometryTransitionFrame = nil
        island.panel.orderOut(nil)
        glow.close()
        dragSurface.hide()
    }

    private func makeRoot(open: Bool, animated: Bool) -> IslandRootView {
        var root = IslandRootView(
            store: store,
            isOpen: open,
            collapsedSize: geometry.mode.isDocked ? geometry.rect.size : geometry.islandFrame.size,
            collapsedTopRadius: geometry.mode.isDocked ? NotchGeometry.expandedTopRadius : NotchGeometry.collapsedTopRadius,
            collapsedBottomRadius: geometry.cornerRadius,
            look: islandLook(open: open),
            lightBorder: systemIsLight,
            drawsGlassStandIn: drawsGlassStandIn,
            inksNotch: atHardwareNotch,
            onOpenStats: { [weak self] in self?.handOff { self?.onOpenStats?() } },
            onOpenSettings: { [weak self] in self?.handOff { self?.onOpenSettings?() } },
            additionalHUDControls: { [weak self] in
                guard let self else { return AnyView(EmptyView()) }
                return self.additionalHUDControls { [weak self] in self?.forceCollapse() }
            },
            alert: previewPlacement == nil ? activeAlert : nil,
            onOpenAlert: { [weak self] in self?.openAlert() },
            onDismissAlert: { [weak self] in self?.dismissCompletionAlert() },
            onOpenAlertSession: { [weak self] in Task { await self?.openAlertSession() } },
            onOpenAlertUsage: { [weak self] in self?.openAlertUsage() },
            sessionNavigationFailed: sessionNavigationFailed,
            onDecideAlert: { [weak self] decision in self?.decideAlert(decision) },
            waitingRequests: PermissionRequests.shared.pending,
            answeringSessionID: answeringSessionID,
            onAnswerSession: { [weak self] id in self?.answerQuestions(in: id) },
            onSelectRequest: { [weak self] id in self?.selectRequest(id) },
            onTyping: { [weak self] typing in self?.setTyping(typing) },
            showsAlertDetails: open && alerts.current?.inUsagePanel == false,
            animatesGeometry: animated
        )
        root.onOpenListedSession = { [weak self] id in Task { await self?.openListedSession(id) } }
        root.failedListedSessionID = failedListedSessionID
        // The panel draws this screen's layout, at its width, and so does the copy the island is measured from.
        root.panelScreen = key
        root.panelWidth = panelWidth
        root.agentPanelWidth = CGFloat(panelStyle.agentPanelWidth)
        root.sessionEvents = alerts.sessionEvents
        root.logoQueue = logoQueue
        root.simpleMeter = simpleMeter
        root.separatesAgents = placement.separatesAgents && geometry.mode.isDocked
        root.stripScale = stripScale
        let focus = focus
        // The agent whose own card is up has its mark woken, from the hover's peek on and through the click that opens
        // the whole panel. An event's card leaves its sender's mark to its agent.
        if open, activeAlert == nil, previewPlacement == nil, let focus {
            if let index = root.logoQueue?.items.firstIndex(where: { AgentArtwork.markKey($0.vendor) == focus.key }) {
                root.logoQueue?.items[index].attention = .opened
            }
            if let index = root.simpleMeter?.items.firstIndex(where: { AgentArtwork.markKey($0.vendor) == focus.key }) {
                root.simpleMeter?.items[index].attention = .opened
            }
        }
        // An event's card up beside its sender's mark: a mark whose agent waits on the user turns to it.
        if previewPlacement == nil, activeAlert != nil, let focus {
            let facing = MarkAttention.facesCard(BotFacing(cardFrom: geometry.edge))
            if let index = root.logoQueue?.items.firstIndex(where: { AgentArtwork.markKey($0.vendor) == focus.key }) {
                root.logoQueue?.items[index].attention = facing
            }
            if let index = root.simpleMeter?.items.firstIndex(where: { AgentArtwork.markKey($0.vendor) == focus.key }) {
                root.simpleMeter?.items[index].attention = facing
            }
        }
        handOutCues(to: &root)
        root.focusVendor = focus?.vendor
        root.peek = focus.map(peek)
        root.pinned = pinned
        root.onPin = { [weak self] in self?.pinAgent() }
        root.botLogos = settings.effectiveSettings.botLogos
        root.edge = geometry.edge
        // The mode decides the silhouette, not whether there are marks to draw.
        root.hidesSilhouette = geometry.mode.isDocked
        return root
    }

    /// Hands the dock's marks their events, through what the dock remembers (`DockMemory`).
    private func handOutCues(to root: inout IslandRootView) {
        let now = store.clock()
        if let items = root.logoQueue?.items {
            root.logoQueue?.items = items.map { item in
                var item = item
                item.cue = dock.cue(for: AgentArtwork.markKey(item.vendor), item.cue, badge: item.state.badge, at: now)
                return item
            }
        }
        if let items = root.simpleMeter?.items {
            root.simpleMeter?.items = items.map { item in
                var item = item
                item.cue = dock.cue(for: AgentArtwork.markKey(item.vendor), item.cue, badge: item.state.badge, at: now)
                return item
            }
        }
    }

    /// Run during the existing hover delay without touching the visible windows or their current glow.
    private func prepareOpening() {
        guard case .opening = machine.state else { return }
        geometry = resolveGeometry()
        let inputs = panelInputs
        guard materialPreparation?.inputs != inputs || materialPreparation?.open != true else { return }
        let root = makeRoot(open: true, animated: false)
        let height = max(80, min(island.contentHeight(for: root).rounded(), maximumContentHeight))
        let surface = surface(open: true, height: height)
        beginMaterialPreparation(inputs: inputs, open: true, height: height, surface: surface)
    }

    /// - shells: the shells the glow gives shadows now, where they are not all of the surface's.
    @discardableResult
    private func captureMaterials(inputs: PanelInputs, surface: Surface,
                                  shells: HUDShells? = nil) -> GlowWindowController.PreparationInputs {
        glow.capturePreparation(geometry: geometry, island: surface.glowIsland,
            islandRadius: surface.glowRadius, islandFlare: surface.glowFlare, glow: surface.glowGeometry,
            outwardOnly: surface.glowSettings.outwardOnly, appearance: inputs.appearance, look: inputs.look,
            shells: shells ?? surface.shells, pattern: surface.glowSettings.pattern(), drawsGlow: surface.drawsGlow)
    }

    @discardableResult
    private func beginMaterialPreparation(inputs: PanelInputs, open: Bool, height: CGFloat, surface: Surface,
                                         captured: GlowWindowController.PreparationInputs? = nil) -> MaterialPreparation {
        let captured = captured ?? captureMaterials(inputs: inputs, surface: surface)
        let render = renderMaterials
        let installed = glow.installedPreparation(matching: captured)
        let task: Task<GlowWindowController.PreparedImages?, Never>? = installed == nil
            ? Task.detached(priority: .userInitiated) {
                guard !Task.isCancelled else { return nil }
                let images = await render(captured)
                return Task.isCancelled ? nil : images
            } : nil
        let preparation = MaterialPreparation(inputs: inputs, open: open, surface: surface, height: height, task: task)
        preparation.images = installed
        materialPreparation = preparation
        guard let task else { return preparation }
        Task { [weak self, weak preparation] in
            guard let images = await task.value, let self, let preparation,
                  self.materialPreparation === preparation else { return }
            guard preparation.inputs == self.panelInputs else {
                self.apply(animated: false)
                return
            }
            preparation.images = images
            if preparation.open, case .opening(let deadline) = self.machine.state, Date() >= deadline {
                self.timerFired()
            } else if preparation.open == self.machine.isOpen {
                self.apply(animated: self.previewPlacement == nil)
            }
        }
        return preparation
    }

    func apply(animated: Bool) {
        // An open card, or the whole panel, is a look at the agents on it: a failure's badge has been seen.
        if machine.isOpen, activeAlert == nil, previewPlacement == nil { directions?.looked(at: focus.map { [$0.key] }) }
        if let id = answeringSessionID,
           !PermissionRequests.shared.pending.contains(where: { $0.sessionID == id && $0.answersQuestion }) {
            answeringSessionID = nil
            stopTyping()
        }
        let open = machine.isOpen
        let previousGeometry = geometry
        geometry = resolveGeometry()
        // A changed edge replaces the coordinate system; it is a repositioning, not an expansion. A detached card moves
        // on its own, inside a window that holds both ends of the move: always on a separated dock, and on any dock
        // while an event's card comes out of its mark, stands beside it or goes back in.
        let reducesMotion = reduceMotion()
        let detached = focus != nil
        let cardsMove = !agentMarks.isEmpty || detached || cardWasDetached || cardSettle != nil
        cardWasDetached = detached
        let cardMoves = cardsMove && previewPlacement == nil && geometry.edge == previousGeometry.edge
            && geometry.mode == previousGeometry.mode && geometry.screenFrame == previousGeometry.screenFrame
            && !reducesMotion
        let animated = animated && geometry.edge == previousGeometry.edge && !cardsMove
            && geometry.mode == previousGeometry.mode && geometry.screenFrame == previousGeometry.screenFrame
            && !reducesMotion
        var root = makeRoot(open: open, animated: animated)
        // A report can change the token legend without changing the coordinator's observed quota rows.
        // Validate at consumption as well as when the coordinator applies an update during the delay.
        let inputs = panelInputs
        var prepared = materialPreparation?.inputs == inputs && materialPreparation?.open == open ? materialPreparation : nil
        if open {
            let height = prepared?.height
                ?? max(80, min(island.contentHeight(for: root).rounded(), maximumContentHeight))
            if showsAlertDetails { alertDetailHeight = height }
            else { panelHeight = height }
        }
        let surface = surface(open: open, height: showsAlertDetails ? alertDetailHeight : panelHeight)
        if prepared?.surface != surface { prepared = nil }
        let (shells, cardLeaves, cardSlides) = cardShadow.shells(surface.shells,
                                                                 card: focus != nil && surface.expanded ? surface.cardFrame : nil,
                                                                 moves: cardMoves, look: inputs.look)
        let captured = captureMaterials(inputs: inputs, surface: surface, shells: shells)
        let installed = glow.installedPreparation(matching: captured)
        let preparingFutureOpening: Bool = if case .opening = machine.state { true } else { false }
        if prepared == nil, installed == nil, !preparingFutureOpening {
            prepared = beginMaterialPreparation(inputs: inputs, open: open,
                height: showsAlertDetails ? alertDetailHeight : panelHeight, surface: surface, captured: captured)
        }
        var windowFrame = surface.windowFrame
        let expanded = surface.expanded
        alertFrame = (activeAlert != nil && !open && expanded) ? windowFrame : nil
        // What the black shape fills, before the window is stretched to keep a surface under the pointer.
        let presentation = surface.cardFrame.size
        if showsAlertDetails {
            alertHoverFloor = pointerInside ? max(alertHoverFloor, windowFrame.height) : 0
            let held = ScreenHUD.heldWindowHeight(card: windowFrame.height, floor: alertHoverFloor,
                                                  pointerInside: pointerInside)
            // On a dock the canvas already spans the card and the marks; only the surface under the pointer grows.
            windowFrame.origin.y -= held - windowFrame.height
            windowFrame.size.height = held
            if focus != nil { windowFrame = windowFrame.intersection(geometry.screenFrame) }
        } else {
            alertHoverFloor = 0
        }
        let agents = agents
        let appearance = agents.glowAppearance(light: systemIsLight, on: key)
        let previousCanvas = island.panel.frame
        let previousCard = island.rootView.presentationFrame.map {
            CGRect(x: previousCanvas.minX + $0.minX, y: previousCanvas.maxY - $0.maxY,
                   width: $0.width, height: $0.height)
        }
        let cardFrame = surface.cardFrame
        if animated && previousCard != cardFrame {
            geometryTransitionFrame = cardFrame
        } else if !animated {
            geometryTransitionFrame = nil
        }

        if cardMoves {
            if targetWindowFrame != windowFrame || cardSettle == nil {
                let moved = targetWindowFrame != windowFrame
                targetWindowFrame = windowFrame
                // Moving the window re-bases what is drawn in it and stops a card in mid-move, so a window that
                // already holds the new place is left alone until the card has arrived.
                if !island.panel.frame.contains(windowFrame) { island.setFrame(island.panel.frame.union(windowFrame)) }
                island.setVisibleSize(windowFrame.size)
                if moved || island.panel.frame != windowFrame {
                    cardSettle?.cancel()
                    cardSettle = Task { [weak self] in
                        try? await Task.sleep(for: .seconds(IslandAnimation.duration + 0.05))
                        guard !Task.isCancelled, let self else { return }
                        self.cardSettle = nil
                        if self.targetWindowFrame == windowFrame { self.island.setFrame(windowFrame) }
                    }
                }
            }
        } else if !animated || targetWindowFrame != windowFrame {
            cardSettle?.cancel()
            cardSettle = nil
            targetWindowFrame = windowFrame
            if animated {
                // Every edge uses the same transition canvas. Card geometry supplies the parked anchor;
                // the union also covers a clamped card and any longer, stationary logo queue.
                island.setFrame(island.panel.frame.union(windowFrame))
                island.setVisibleSize(windowFrame.size)
            } else {
                island.setFrame(windowFrame)
                island.setVisibleSize(windowFrame.size)
            }
        }
        glow.update(
            geometry: geometry,
            island: surface.glowIsland,
            islandRadius: surface.glowRadius,
            islandFlare: surface.glowFlare,
            glow: surface.glowGeometry,
            outwardOnly: surface.glowSettings.outwardOnly,
            appearance: appearance,
            look: inputs.look,
            shells: shells,
            animated: animated,
            animatesShells: animated || cardMoves,
            cardLeaves: cardLeaves,
            fadesDock: cardSlides,
            alert: activeAlert,
            quotaVendors: agents.pulseVendors,
            pattern: surface.glowSettings.pattern(),
            backdrop: surface.backdrop ? geometry.rect : nil,
            drawsGlow: surface.drawsGlow,
            prepared: prepared?.images ?? installed,
            materialsPending: prepared?.images == nil && installed == nil
        )
        glowShells = shells
        if !preparingFutureOpening, prepared?.images != nil || installed != nil { materialPreparation = nil }
        // The strip's place on screen is fixed; the window around it is not, so the offset between them is
        // measured rather than assumed to be the window's own top edge — which moves when the panel opens.
        let canvas = island.panel.frame
        root.logoQueueInset = max(0, canvas.maxY - geometry.rect.maxY)
        root.logoQueueHeight = geometry.rect.height
        root.logoQueueFrame = CGRect(x: geometry.rect.minX - canvas.minX,
                                     y: canvas.maxY - geometry.rect.maxY,
                                     width: geometry.rect.width, height: geometry.rect.height)
        updateClickThrough(geometry.mode.isDocked && !expanded)
        // A tracking area can report an exit when the window or its hosting view resizes, even though the
        // pointer has not left the visible HUD. Both native events and the coordinator use screen geometry.
        // Collapsed logo queues pass clicks through, so their coordinator alone supplies the events.
        island.onPointerChange = geometry.mode.isDocked
            ? nil
            : { [weak self] _ in self?.samplePointer() }
        root.presentationSize = presentation
        root.cardScreenFrame = surface.cardFrame
        root.presentationFrame = CGRect(x: surface.cardFrame.minX - canvas.minX,
                                        y: canvas.maxY - surface.cardFrame.maxY,
                                        width: surface.cardFrame.width, height: surface.cardFrame.height)
        root.onContentHeight = { [weak self] height in self?.updatePanelHeight(height) }
        root.onGeometryCompletion = { [weak self] in
            // Reversing or repositioning can finish an older animation together with the new one.
            // A changing logo queue can also change the canvas without changing the card's target.
            guard let self, self.geometryTransitionFrame == cardFrame,
                  let target = self.targetWindowFrame else { return }
            self.geometryTransitionFrame = nil
            self.island.setFrame(target)
        }
        island.setRootView(root)
        if geometryTransitionFrame == nil, cardSettle == nil { island.setFrame(windowFrame) }
        updateDragSurface()
        prepareOpening()
    }

    /// A detached card has arrived (`CardShadowHold`): its shadow fades in under it, and the dock's, cut by the card from
    /// now on, fades to its new shape with it. Under Reduce Motion both change at once.
    private func revealCardShadow(_ shells: HUDShells, look: HUDLook) {
        glowShells = shells
        glow.updateShells(shells, look: look, geometry: geometry, animates: !reduceMotion())
    }

    /// One agent's card: a peek, its whole panel, or an event it sent. It floats beside the agent's mark, and
    /// the canvas holds the card and the strip, so every other mark stays where it is.
    private func detachedSurface(open: Bool, height: CGFloat, focus: AgentMark) -> Surface {
        let peek = peek(for: focus)
        let showsPeek = open && !pinned && activeAlert == nil
        let size: CGSize = if !open {
            IslandRootView.dockCompactSize(edge: geometry.edge, collapsedSize: geometry.rect.size, scale: stripScale, wings: compactWings)
        } else if alerts.current?.inUsagePanel == false {
            CGSize(width: activeAlert?.detailWidth ?? IslandController.alertDetailWidth, height: height)
        } else if showsPeek {
            peek.size
        } else {
            CGSize(width: CGFloat(panelStyle.agentPanelWidth), height: height)
        }
        let cardFrame = Self.detachedFrame(size: size, anchor: focus.anchor, strip: geometry.rect, edge: geometry.edge,
                                           gap: showsPeek && peek.style == .line ? -peek.tuck : 6,
                                           screen: geometry.screenFrame)
        let windowFrame = cardFrame.union(geometry.islandFrame).intersection(geometry.screenFrame).integral
        // The marks have not moved, so whatever glows behind them goes on doing so, and the Simple panel keeps its
        // shadow beside the card's: its own on a separated dock, the island's at rest on any other.
        let marks = surface(open: false, height: height, parked: true)
        let panel = marks.shells.dock ?? (geometry.mode == .simple ? marks.shells.surface : nil)
        let full = open && (pinned || alerts.current?.inUsagePanel == false)
        let card = HUDShell(frame: cardFrame, radius: Self.detachedRadius(height: cardFrame.height, full: full,
                                                                          bubble: peek.style == .bubble && activeAlert == nil),
                            flare: 0, floats: true, bare: false)
        return Surface(windowFrame: windowFrame, cardFrame: cardFrame, expanded: true, glowIsland: marks.glowIsland,
                       glowRadius: marks.glowRadius, glowFlare: marks.glowFlare, glowGeometry: marks.glowGeometry,
                       glowSettings: marks.glowSettings, backdrop: marks.backdrop, drawsGlow: marks.drawsGlow,
                       shells: HUDShells(surface: card, dock: panel))
    }

    /// How round a detached card is, as `IslandRootView` draws it: a full card keeps a card's corner, a peek bubble a
    /// smaller one, and a line or a compact reminder is as round as its height allows.
    static func detachedRadius(height: CGFloat, full: Bool, bubble: Bool) -> CGFloat {
        min(full ? 22 : bubble ? 14 : height, height / 2)
    }

    /// Both preparation and presentation use the same parked edge, clamping and glow dimensions.
    private struct Surface: Equatable {
        let windowFrame: CGRect
        let cardFrame: CGRect
        let expanded: Bool
        let glowIsland: CGRect
        let glowRadius: CGFloat
        /// The flares of the island the glow surrounds, which a glass island has cut out of it with the island.
        let glowFlare: CGFloat
        let glowGeometry: GlowGeometry
        let glowSettings: GlowSettings
        let backdrop: Bool
        let drawsGlow: Bool
        /// What a light or glass look gives shadows.
        let shells: HUDShells
    }

    /// What a light or glass look gives shadows on this screen, in the shapes `IslandRootView` fills: the island, or the
    /// panel or reminder it has grown into, kept bare on a collapsed logo queue and at a hardware notch so the panel grows
    /// out of it with its shadow; and on a dock that separates its agents the Simple panel, which stays put while a card comes and goes.
    private func shells(expanded: Bool, open: Bool, islandFrame: CGRect, radius: CGFloat, flare: CGFloat) -> HUDShells {
        let separates = placement.separatesAgents && geometry.mode.isDocked
        let panel = simpleMeter.map { meter in
            HUDShell(frame: Self.lifted(geometry.rect, on: geometry.edge), radius: meter.cornerRadius, flare: 0,
                     floats: true, bare: false)
        }
        switch geometry.mode {
        case .notch:
            // At a hardware notch the collapsed island is the camera housing, black in any look (`NotchInk`), and casts
            // no shadow; it is kept bare so that the panel grows out of it with its shadow.
            let bare = geometry.hasNotch && !expanded
            return HUDShells(surface: HUDShell(frame: islandFrame, radius: radius, flare: flare, floats: false, bare: bare),
                             dock: nil)
        case .logos:
            let silhouette = HUDShell(frame: islandFrame, radius: radius, flare: flare, floats: false, bare: !expanded)
            return HUDShells(surface: separates && !expanded ? nil : silhouette, dock: nil)
        case .simple:
            // The panel at rest is the island; it grows into the panel or a reminder, as round as the island makes them.
            let grown = HUDShell(frame: Self.lifted(islandFrame, on: geometry.edge),
                                 radius: open ? IslandController.expandedRadius : 16, flare: 0, floats: true, bare: false)
            return HUDShells(surface: expanded ? grown : separates ? nil : panel, dock: separates ? panel : nil)
        }
    }

    /// `rect` less the margin that lifts the Simple panel off its edge, as `IslandShape` lifts it: the panel's own
    /// shape, at rest in its strip or grown open.
    private static func lifted(_ rect: CGRect, on edge: HUDEdge) -> CGRect {
        var panel = rect
        let margin = SimpleMeterConfig.edgeMargin
        switch edge {
        case .top: panel.size.height -= margin
        case .bottom: panel.origin.y += margin; panel.size.height -= margin
        case .left: panel.origin.x += margin; panel.size.width -= margin
        case .right: panel.size.width -= margin
        }
        return panel
    }

    /// `parked` asks for the collapsed surface whatever is showing: the marks' own, which a detached card keeps.
    private func surface(open: Bool, height: CGFloat, parked: Bool = false) -> Surface {
        let expanded = (open || activeAlert != nil) && previewPlacement == nil && !parked
        // On a dock an event's card always stands beside a mark (`focus`); only the notch grows wings for one.
        if let focus, expanded { return detachedSurface(open: open, height: height, focus: focus) }
        let compactSize = CGSize(width: geometry.rect.width + 2 * (IslandController.alertWingWidth + IslandController.alertSidePadding),
                                 height: max(38, geometry.rect.height))
        let size = open
            ? (alerts.current?.inUsagePanel == false
                ? CGSize(width: activeAlert?.detailWidth ?? IslandController.alertDetailWidth,
                         height: height)
                : CGSize(width: panelWidth, height: height))
            : compactSize
        // The shoulder curves widen the contact with the parked edge. Clamp their whole canvas, then
        // derive the card's core from it, so even a dock near a corner retains both curved shoulders.
        let flare = open || geometry.mode.isDocked ? NotchGeometry.expandedTopRadius : NotchGeometry.collapsedTopRadius
        let horizontal = geometry.edge.isHorizontal
        let dockWindowSize = CGSize(width: size.width + (horizontal ? flare * 2 : 0),
                                    height: size.height + (horizontal ? 0 : flare * 2))
        let cardFrame = expanded
            ? (geometry.mode.isDocked
                ? geometry.expandedFrame(size: dockWindowSize)
                : geometry.expandedFrame(size: size).insetBy(dx: -flare, dy: 0))
            : geometry.islandFrame
        // A short card must not clip a longer queue. The extra canvas stays transparent and the card
        // retains its own screen position, including while a reminder's hover floor holds a taller window.
        let windowFrame = open && geometry.mode.isDocked && activeAlert == nil
            ? cardFrame.union(geometry.islandFrame).intersection(geometry.screenFrame)
            : cardFrame
        let islandFrame = expanded
            ? (geometry.mode.isDocked
                ? cardFrame.insetBy(dx: horizontal ? flare : 0, dy: horizontal ? 0 : flare)
                : geometry.expandedFrame(size: size))
            : geometry.rect
        let radius = open ? IslandController.expandedRadius : max(geometry.cornerRadius, activeAlert == nil ? 0 : 14)
        // This screen's own glow, or the default when it has not been given one, with its style draft drawn over it.
        let glowSettings = settings.effectiveGlow(on: key)
        // The glow style is the HUD's backdrop in both modes, but the shape it radiates from differs. The
        // notch is a small silhouette, so the field reads as a rim around it. A logo queue wants a curtain
        // exactly as wide as the marks: the shape is a flat lip at the screen's top edge, run wider than the
        // queue so every cell's nearest point is straight above it and the field falls vertically. The glow
        // panel then clips that field back to the queue's own column, cutting off the ends that would dip.
        let parked = geometry.mode.isDocked && !expanded
        let backdrop = parked && geometry.mode == .logos
        // Only a silhouette is worth rimming. A logo queue has none — its glow is the backdrop behind the
        // marks — so once the panel or an event has grown over the place that field belonged, it stops
        // rather than following the new shape around.
        let drawsGlow = !geometry.mode.isDocked || backdrop
        let overhang = GlowWindowController.backdropOverhang(glowSettings)
        // The lip is a flat line on the screen's top edge, run wider than the queue: every cell's nearest
        // point is then straight above it, so the field falls vertically instead of curling in at the ends,
        // and the marks sit inside the field rather than below where it starts.
        let glowIsland: CGRect = {
            guard parked else { return islandFrame }
            switch geometry.edge {
            case .top:
                return CGRect(x: geometry.rect.minX - overhang, y: geometry.screenFrame.maxY,
                              width: geometry.rect.width + overhang * 2, height: 2)
            case .bottom:
                return CGRect(x: geometry.rect.minX - overhang, y: geometry.screenFrame.minY - 2,
                              width: geometry.rect.width + overhang * 2, height: 2)
            case .left:
                return CGRect(x: geometry.screenFrame.minX - 2, y: geometry.rect.minY - overhang,
                              width: 2, height: geometry.rect.height + overhang * 2)
            case .right:
                return CGRect(x: geometry.screenFrame.maxX, y: geometry.rect.minY - overhang,
                              width: 2, height: geometry.rect.height + overhang * 2)
            }
        }()
        let glowRadius = parked ? 0 : radius
        let glowGeometry = glowSettings
            .geometry(islandWidth: geometry.edge.isHorizontal ? glowIsland.width : glowIsland.height,
                      islandHeight: geometry.edge.isHorizontal ? glowIsland.height : glowIsland.width,
                      islandRadius: glowRadius)
            .fitted(within: geometry.edge.isHorizontal ? geometry.screenFrame.height : geometry.screenFrame.width)
        return Surface(windowFrame: windowFrame, cardFrame: cardFrame, expanded: expanded, glowIsland: glowIsland,
                       glowRadius: glowRadius, glowFlare: parked ? 0 : flare, glowGeometry: glowGeometry,
                       glowSettings: glowSettings, backdrop: backdrop, drawsGlow: drawsGlow,
                       shells: shells(expanded: expanded, open: open, islandFrame: islandFrame, radius: radius, flare: flare))
    }
}
