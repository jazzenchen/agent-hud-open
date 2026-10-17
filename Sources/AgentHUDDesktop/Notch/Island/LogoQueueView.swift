import SwiftUI
import AgentHUDCore

/// What an agent's own card does to its mark, on a dock that separates its agents.
enum MarkAttention: Equatable, Sendable {
    /// Nobody is looking at this agent's card: the mark follows its agent.
    case none
    /// Its card is open, a hover's peek or a click's whole panel: the mark is under the pointer until the card closes.
    /// A resting mark comes up and greets; a busy one keeps to its work and turns its eyes.
    case opened
    /// A card from its agent is up beside it, this way: a mark whose agent waits on the user turns to it.
    case facesCard(BotFacing)

    /// The mark's state with this on top of what its agents do.
    func state(of base: BotState) -> BotState {
        var state = base
        switch self {
        case .none: break
        case .opened: state.hovered = true
        case .facesCard(let facing): state.facing = facing
        }
        return state
    }
}

/// One mark in the queue. The queue shows one mark per vendor, not per agent: two Claude windows are
/// one Claude, and the mark's bot shows what the session it follows is doing (`MarkDirector`).
struct LogoQueueItem: Identifiable, Equatable {
    let vendor: String
    /// Any agent of this vendor has a running session.
    let isWorking: Bool
    /// What the mark shows of its agents: the director's state for it, or with no director plain work while any of
    /// them works.
    var state: BotState
    /// The newest event of its agents, for the mark to play once if it hears of it in time.
    var cue: BotCue? = nil
    /// Whether this agent's card is the one being looked at.
    var attention = MarkAttention.none
    var id: String { vendor }

    init(vendor: String, isWorking: Bool, state: BotState? = nil, cue: BotCue? = nil) {
        self.vendor = vendor
        self.isWorking = isWorking
        self.state = state ?? BotState(look: isWorking ? .plain : .idle)
        self.cue = cue
    }

    /// What the mark is told: its agents' state with its card's attention on top.
    var shown: BotState { attention.state(of: state) }

    /// The item with what the director says its mark shows, by mark key; an item the director has nothing for is left
    /// as it is.
    @MainActor
    func directed(by directions: [String: MarkDirector.Direction]) -> LogoQueueItem {
        guard let direction = directions[AgentArtwork.markKey(vendor)] else { return self }
        var item = self
        item.state = direction.state
        item.cue = direction.cue
        return item
    }

    /// Collapses agents onto the mark they are drawn with, keeping the order they were watched in. Two
    /// Claude windows are one Claude; so are Codex and ChatGPT, which share OpenAI's artwork — a second
    /// identical mark would take a place in the row and tell a glance nothing. Either of them running wakes
    /// the bot they share.
    @MainActor
    static func queue(rows: [(vendor: String, isWorking: Bool)]) -> [LogoQueueItem] {
        var order: [String] = []
        var vendors: [String: String] = [:]
        var working: [String: Bool] = [:]
        for row in rows {
            let key = AgentArtwork.markKey(row.vendor)
            if vendors[key] == nil {
                order.append(key)
                vendors[key] = row.vendor
            }
            working[key] = (working[key] ?? false) || row.isWorking
        }
        return order.map { LogoQueueItem(vendor: vendors[$0] ?? $0, isWorking: working[$0] ?? false) }
    }
}

/// A queue resolved for one screen: what to draw, how big, and how fast.
struct LogoQueueConfig: Equatable {
    var items: [LogoQueueItem]
    var edge: HUDEdge
    /// Side of one mark, in points.
    var logo: CGFloat
    var edgeDistance: CGFloat
    var gap: CGFloat
    var workingSeconds: Double
    /// Working agents stay bots; idle agents occasionally become bots for a few seconds.
    var bots: Bool

    init(items: [LogoQueueItem], placement: ScreenPlacement, settings: AgentHUDCore.Settings) {
        self.items = items
        edge = placement.edge
        logo = placement.logoSize
        edgeDistance = placement.logoEdgeDistance
        gap = logo * placement.gapScale
        workingSeconds = settings.breathSeconds
        bots = settings.botLogos
    }

    /// The strip touches the screen edge; centring the marks within it keeps their edge at the requested distance.
    var stripThickness: CGFloat { logo + edgeDistance * 2 }

    /// The marks' run, with their screen-edge distance perpendicular to the run.
    var size: CGSize {
        guard !items.isEmpty else { return .zero }
        let run = logo * CGFloat(items.count) + gap * CGFloat(items.count - 1)
        return edge.isHorizontal ? CGSize(width: run, height: stripThickness) : CGSize(width: stripThickness, height: run)
    }

    /// Each mark's centre along the run, measured from the run's start: left on a horizontal edge, top on a side.
    var markCentres: [(vendor: String, along: CGFloat)] {
        items.enumerated().map { ($1.vendor, CGFloat($0) * (logo + gap) + logo / 2) }
    }
}

/// The collapsed HUD on a screen in logo mode: every watched agent's own mark. Working marks stay bots,
/// idle marks occasionally turn into bots, and their centres stay fixed.
/// Status colour is carried by the glow behind them.
struct LogoQueueView: NSViewRepresentable {
    let config: LogoQueueConfig
    /// The marks are drawn for a light ground, the surface of a light HUD behind them: in dark ink, and their bots too
    /// (`LogoImages.Key.light`). Marks bare on the wallpaper are drawn for it, whatever the look.
    let light: Bool
    /// Wakes every bot whether its agent is working or not, so the settings preview shows their motion.
    var previewsMotion = false

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.displayScale) private var displayScale

    func makeNSView(context: Context) -> LogoQueueLayerView { LogoQueueLayerView() }

    func updateNSView(_ view: LogoQueueLayerView, context: Context) {
        view.apply(config: config, light: light, scale: displayScale, animates: !reduceMotion,
                   previewsMotion: previewsMotion)
    }
}

/// Lays stationary marks out along the edge and follows their bots through the shared clock.
final class LogoQueueLayerView: NSView {
    /// The seed of the bots built from here on, fixed only to replay their acts in a test.
    var seed: UInt64?
    private(set) var marks: [CALayer] = []
    /// One per mark, for the marks that have a bot. A bot's layer sits over its mark and takes its place
    /// while the bot is out; the mark comes back when the bot has settled into the logo again.
    private(set) var actors: [BotActor?] = []
    /// How far the baked outline overflows a mark, in points, so the layout leaves room for exactly that.
    private var bleed: CGFloat = 1
    /// Alpha bounds in each baked bitmap, including its outline, normalized with y running down.
    private var visibleBounds: [CGRect] = []
    /// The still sign over each mark that shows only its logo, with bots off or motion reduced: a waiting request's or
    /// a badge's, a picture that never moves. Nil where there is none.
    private(set) var signs: [CALayer?] = []
    /// What each of them shows, and where it stands from its mark's top left.
    private var signed: [(sign: BotSign, frame: CGRect)?] = []
    /// Each mark's logo box, its top left, as last laid out.
    private var origins: [CGPoint] = []
    /// The newest event each mark was handed, by vendor, so a mark built again never plays one twice.
    private var played: [String: BotCue] = [:]
    /// Where each bot sits on the screen, for its gaze, worked out once for a layout and a window frame.
    private var places: [CGPoint?] = []
    private var placedIn: CGRect?
    private var applied: (config: LogoQueueConfig, light: Bool, scale: CGFloat, animates: Bool, previews: Bool)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.masksToBounds = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        guard let applied else { return }
        position(config: applied.config)
    }

    /// The marks are laid out against the view's own bounds, so a size change has to reach them even when
    /// AppKit does not consider the bounds themselves changed.
    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if let applied { position(config: applied.config) }
    }

    func apply(config: LogoQueueConfig, light: Bool, scale: CGFloat, animates: Bool, previewsMotion: Bool = false) {
        if let applied, applied.config == config, applied.light == light, applied.scale == scale,
           applied.animates == animates, applied.previews == previewsMotion { return }
        let reshaped = applied?.config.items.map(\.id) != config.items.map(\.id)
            || applied?.scale != scale
            || applied?.config.logo != config.logo
            || applied?.config.bots != config.bots || applied?.config.workingSeconds != config.workingSeconds
        let regrounded = applied.map { $0.light != light } ?? false
        // Bots built again while bots were out pick up where they were.
        let botsWereOut = applied.map { $0.config.bots && $0.animates } ?? false
        applied = (config, light, scale, animates, previewsMotion)
        if reshaped || regrounded {
            // Marks drawn again for another ground alone fade into it, as the surface behind them moves to its new
            // colours. A queue that changes otherwise is laid out again at once, as it always has been.
            if regrounded, !reshaped, animates, let layer { IslandAnimation.fadeLook(of: layer) }
            build(config: config, light: light, scale: scale, awake: botsWereOut && config.bots)
        }
        matchDisplay()
        position(config: config)
        animate(config: config, animates: animates, previewsMotion: previewsMotion)
    }

    /// - awake: the marks' bots were out before: each takes up its look at once (`BotActor`).
    private func build(config: LogoQueueConfig, light: Bool, scale: CGFloat, awake: Bool) {
        marks.forEach { $0.removeFromSuperlayer() }
        actors.forEach { $0?.layer.removeFromSuperlayer() }
        signs.forEach { $0?.removeFromSuperlayer() }
        signs = Array(repeating: nil, count: config.items.count)
        signed = Array(repeating: nil, count: config.items.count)
        let side = Int((config.logo * scale).rounded())
        // One device pixel at any size: a hairline reads as the mark's own edge, anything thicker as a
        // border drawn around it.
        let outline = 1
        bleed = CGFloat(outline) / max(1, scale)
        // The queue sits on the wallpaper, and on the HUD's surface once it opens: the marks are drawn for the ground they
        // stand on.
        visibleBounds = []
        marks = config.items.map { item in
            let mark = CALayer()
            mark.contentsScale = scale
            mark.contentsGravity = .resizeAspect
            let key = LogoImages.Key(vendor: item.vendor, side: side, outline: outline, light: light)
            mark.contents = LogoImages.image(key)
            visibleBounds.append(LogoImages.visibleBounds(key))
            layer?.addSublayer(mark)
            return mark
        }
        actors = config.items.map { item in
            guard config.bots, let form = BotForm.form(for: item.vendor, ground: light ? .light : .dark) else { return nil }
            let actor = BotActor(form: form, staysBot: false, period: config.workingSeconds,
                                 seed: seed ?? UInt64.random(in: 0...UInt64.max), awake: awake)
            actor.resize(side: config.logo, scale: scale, outline: outline)
            layer?.addSublayer(actor.layer)
            return actor
        }
    }

    private func position(config: LogoQueueConfig) {
        // The baked outline overflows the mark on every edge, so each layer is grown by exactly that and the
        // extra is centred away; the spacing the user set still applies to the marks themselves.
        let step = config.logo + config.gap
        // The view is given the window's width, which is wider than the run; the queue sits in the middle
        // of it, as the notch does.
        let run = config.edge.isHorizontal ? config.size.width : config.size.height
        let lead = max(0, ((config.edge.isHorizontal ? bounds.width : bounds.height) - run) / 2)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        origins = []
        for (index, mark) in marks.enumerated() {
            let along = lead + CGFloat(index) * step
            var origin = config.edge.isHorizontal
                ? CGPoint(x: along, y: (bounds.height - config.logo) / 2)
                : CGPoint(x: (bounds.width - config.logo) / 2, y: along)
            if index < visibleBounds.count, !visibleBounds[index].isEmpty {
                let alpha = visibleBounds[index], side = config.logo + bleed * 2
                let visible = CGRect(x: origin.x - bleed + alpha.minX * side,
                                     y: origin.y - bleed + alpha.minY * side,
                                     width: alpha.width * side, height: alpha.height * side)
                // Assets carry different transparent padding. Shift only toward/away from the parked edge;
                // the configured logo size and along-edge spacing continue to use the original artwork box.
                switch config.edge {
                case .top: origin.y += config.edgeDistance - visible.minY
                case .bottom: origin.y += bounds.height - config.edgeDistance - visible.maxY
                case .left: origin.x += config.edgeDistance - visible.minX
                case .right: origin.x += bounds.width - config.edgeDistance - visible.maxX
                }
            }
            mark.frame = CGRect(x: origin.x - bleed, y: origin.y - bleed,
                                width: config.logo + bleed * 2, height: config.logo + bleed * 2)
            if index < actors.count {
                actors[index]?.layer.position = CGPoint(x: origin.x + config.logo / 2, y: origin.y + config.logo / 2)
            }
            origins.append(origin)
            if index < signs.count, let sign = signs[index], let placed = signed[index] {
                sign.frame = placed.frame.offsetBy(dx: origin.x, dy: origin.y)
            }
        }
        CATransaction.commit()
        placedIn = nil
    }

    /// The bots draw their frames in the colours of the display this queue is on.
    private func matchDisplay() {
        let space = window?.colorSpace?.cgColorSpace
        for actor in actors { actor?.show(in: space) }
    }

    private func animate(config: LogoQueueConfig, animates: Bool, previewsMotion: Bool) {
        let now = CACurrentMediaTime()
        for index in marks.indices {
            let item = index < config.items.count ? config.items[index] : nil
            let actor = index < actors.count ? actors[index] : nil
            let state = !animates ? BotState() : previewsMotion ? BotState(look: .plain) : item?.shown ?? BotState()
            actor?.set(state, at: now)
            // An event the mark hears of too late is let go, as the bot itself would let it go.
            if let item, let cue = item.cue, played[item.id] != cue {
                played[item.id] = cue
                if animates, -cue.at.timeIntervalSinceNow <= BotEvent.freshness { actor?.play(cue.event, at: now) }
            }
            let stands = animates && actor != nil
            wear(stands ? nil : item?.shown.stillSign, at: index, vendor: item?.vendor, config: config)
            if !stands { marks[index].opacity = stillOpacity(at: index) }
        }
        followBots()
    }

    /// How strongly the mark at `index` shows while it is its still logo, with bots off or motion reduced: half for no
    /// news, which needs no motion to say.
    private func stillOpacity(at index: Int) -> Float {
        guard let items = applied?.config.items, index < items.count else { return 1 }
        return items[index].shown.look == .noNews ? 0.5 : 1
    }

    /// Lays `sign` over the mark at `index` as a still picture of its own, or takes the one there away; a logo that
    /// shows for its bot wears none, since the bot carries it.
    private func wear(_ sign: BotSign?, at index: Int, vendor: String?, config: LogoQueueConfig) {
        guard index < signs.count, signed[index]?.sign != sign, let applied else { return }
        signs[index]?.removeFromSuperlayer()
        signs[index] = nil
        signed[index] = nil
        let ground: HUDLook.Appearance = applied.light ? .light : .dark
        guard let sign, let vendor,
              let still = BotSigns.still(sign, form: BotForm.form(for: vendor, ground: ground), side: config.logo,
                                         scale: applied.scale, ground: ground) else { return }
        let layer = CALayer()
        layer.contents = still.image
        layer.contentsScale = applied.scale
        layer.contentsGravity = .resize
        // A still sign: nothing about it ever moves.
        layer.actions = ["contents": NSNull(), "position": NSNull(), "bounds": NSNull(), "hidden": NSNull(),
                         "opacity": NSNull()]
        let origin = index < origins.count ? origins[index] : .zero
        layer.frame = still.frame.offsetBy(dx: origin.x, dy: origin.y)
        self.layer?.addSublayer(layer)
        signs[index] = layer
        signed[index] = (sign, still.frame)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        placedIn = nil
        if window == nil { BotClock.shared.drop(self) } else { matchDisplay(); followBots() }
    }

    private func followBots() {
        guard window != nil, actors.contains(where: { $0 != nil }) else { return }
        BotClock.shared.follow(self) { [weak self] now in self?.stepBots(to: now) }
    }

    /// One frame for the bots in the queue that are due one. Returns the seconds until the soonest of them needs
    /// the next: idle logos wait for their next bot appearance, and nil means no bot has anything left to show.
    func stepBots(to now: TimeInterval) -> TimeInterval? {
        guard applied?.animates == true else {
            for (index, actor) in actors.enumerated() {
                actor?.layer.isHidden = true
                marks[index].opacity = stillOpacity(at: index)
            }
            return nil
        }
        guard let window else { return nil }
        let runsDown = layer?.isGeometryFlipped ?? false
        if placedIn != window.frame || places.count != actors.count {
            placedIn = window.frame
            places = Array(repeating: nil, count: actors.count)
        }
        var pointer: CGPoint?, soonest: TimeInterval?
        for (index, actor) in actors.enumerated() {
            guard let actor, index < marks.count else { continue }
            let wait = actor.step(to: now, yRunsDown: runsDown) {
                // The pointer is read once a frame, and only when some bot has eyes to turn.
                let target = pointer ?? NSEvent.mouseLocation
                pointer = target
                let place = places[index] ?? window.convertPoint(toScreen: convert(actor.layer.position, to: nil))
                places[index] = place
                // Screen coordinates run up; a bot's gaze is measured with y running down.
                return BotMotion.aim(from: CGPoint(x: place.x, y: -place.y), to: CGPoint(x: target.x, y: -target.y))
            }
            let opacity: Float = actor.layer.isHidden ? 1 : 0
            if marks[index].opacity != opacity { marks[index].opacity = opacity }
            if let wait { soonest = min(soonest ?? wait, wait) }
        }
        return soonest
    }
}
