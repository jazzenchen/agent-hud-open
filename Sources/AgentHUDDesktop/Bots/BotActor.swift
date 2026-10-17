import AppKit
import QuartzCore
import SwiftUI

extension EnvironmentValues {
    /// Whether marks show bots while working and occasionally while idle. Off, every mark stays a logo.
    @Entry var botLogos: Bool = true
}

/// One bot on screen: a layer whose picture follows the bot's look and whose transform follows how it sits.
///
/// The two are kept apart on purpose. A breathing body changes its pose every frame and its picture almost
/// never, so the picture is only drawn again when the eyes or the outline have visibly moved.
///
/// Claude's mark plays Clawd's own sprites instead (`ClawdActor`), once they are decoded and while the mark is big
/// enough for them; until then, and in the demo and a recording, its Clawd is drawn like every other bot.
@MainActor
final class BotActor {
    let layer = CALayer()
    let form: BotForm
    private var motion: BotMotion
    /// Clawd's sprites, for Claude's mark; nil for every other.
    private let sprites: ClawdSprites?
    /// Claude's mark played from Clawd's sprites; nil while it is drawn.
    private(set) var clawd: ClawdActor?
    private let staysBot: Bool, arrivesAfter: TimeInterval, seed: UInt64
    /// The bot was out before this actor was built for it: its first look starts where its loop begins (`resume`).
    private let awake: Bool
    /// When the bot was first given a state: a staying Clawd that takes over later arrives when the drawn one did.
    private var firstSet: TimeInterval?
    /// The event told last, and when: Clawd's sprites, taking over after it was told, still play it while it is fresh.
    private var told: (event: BotEvent, at: TimeInterval)?
    /// The actor has shown a frame.
    private var stepped = false
    private var shown: BotRenderer.Look?
    private var placed: CGAffineTransform?
    private var faded: CGFloat = 1
    /// When the bot next needs a frame, on the clock it is stepped with; nil once it rests as its logo.
    private var due: TimeInterval? = 0
    /// Side of the mark in points, the display's pixel scale, and the hairline outline in pixels.
    private var side: CGFloat = 0
    private var scale: CGFloat = 2
    private var outline = 0
    /// The colours of the display the bot is on, once its view is in a window.
    private var space: CGColorSpace?
    /// A queue's bot hides while its mark rests as the logo, because the queue draws that logo itself. A bot
    /// with nothing under it keeps its layer and draws the logo there.
    var restsHidden = true

    /// - arrivesAfter: seconds a staying bot keeps its logo before it turns into the bot.
    /// - seed: the bot's random acts, fixed only to replay them in a test.
    /// - sprites: where Claude's mark finds Clawd's sprites.
    /// - awake: the bot was already out where this one is built, as a dock's are when it is laid out again with bots
    ///   on.
    ///   Its first look is taken up at its loop's start, with no morph from the logo and no intro.
    init(form: BotForm, staysBot: Bool, period: TimeInterval, arrivesAfter: TimeInterval = 0,
         seed: UInt64 = UInt64.random(in: 0...UInt64.max), sprites: ClawdSprites = .shared, awake: Bool = false) {
        self.form = form
        self.staysBot = staysBot
        self.arrivesAfter = arrivesAfter
        self.seed = seed
        self.awake = awake
        self.sprites = form.isClawd ? sprites : nil
        self.sprites?.want()
        motion = BotMotion(seed: seed)
        motion.staysBot = staysBot
        motion.arrivesAfter = arrivesAfter
        motion.period = max(0.5, period)
        motion.fit(form.spec)
        layer.contentsGravity = .resize
        layer.isHidden = true
        // Every change here is already one frame of an animation; an implicit one on top would smear it.
        layer.actions = ["contents": NSNull(), "transform": NSNull(), "hidden": NSNull(), "bounds": NSNull(),
                         "position": NSNull(), "opacity": NSNull()]
    }

    var phase: BotPhase { motion.phase }
    var state: BotState { motion.state }
    /// The event the bot plays now, as every bot is told it; Clawd's sprites play the same.
    var playing: BotEvent? { motion.playing }

    /// How big the layer has to be to hold a mark `side` points wide, with its room for overflow and outline.
    static func extent(side: CGFloat, scale: CGFloat, outline: Int) -> CGFloat {
        ((side * scale * (1 + 2 * BotRenderer.margin)).rounded(.up) + CGFloat(outline) * 2) / max(1, scale)
    }

    func resize(side: CGFloat, scale: CGFloat, outline: Int) {
        guard side != self.side || scale != self.scale || outline != self.outline else { return }
        self.side = side
        self.scale = scale
        self.outline = outline
        let extent = Self.extent(side: side, scale: scale, outline: outline)
        layer.bounds = CGRect(x: 0, y: 0, width: extent, height: extent)
        layer.contentsScale = scale
        shown = nil
        clawd?.redraw()
        due = 0
    }

    /// Frames are drawn in the colours of the display they are shown on, which a layer shows as they are. Until a
    /// display is known they are sRGB, and the layer converts each one.
    func show(in space: CGColorSpace?) {
        if let space, let current = self.space, CFEqual(space, current) { return }
        if space == nil, self.space == nil { return }
        self.space = space
        shown = nil
        clawd?.redraw()
        due = 0
    }

    /// A phase is acted out from the next frame, however long the bot meant to wait.
    func set(_ phase: BotPhase, at now: TimeInterval) { set(phase.state, at: now) }

    /// A state is acted out from the next frame, however long the bot meant to wait.
    func set(_ state: BotState, at now: TimeInterval) {
        let first = firstSet == nil
        if first { firstSet = now }
        motion.set(state, at: now)
        if first, awake { motion.resume(at: now) }
        if first, awake { clawd?.resume(state, at: now) } else { clawd?.set(state, at: now) }
        due = 0
    }

    /// Plays an event over the look once the bot is up (`BotMotion.play`).
    func play(_ event: BotEvent, at now: TimeInterval) {
        told = (event, now)
        motion.play(event, at: now)
        clawd?.play(event, at: now)
        due = 0
    }

    /// Advances to `now` once the bot is due a frame, and redraws if it looks different. Returns the seconds
    /// until it next needs one: a frame's length while it moves, the time to its next visit while an idle mark
    /// waits as its logo with the layer hidden, and nil once it rests as the logo for good.
    ///
    /// - yRunsDown: whether y grows downwards in the space this layer is laid out in.
    /// - aim: where to look, from `BotMotion.aim(from:to:)`. It is asked only while there are eyes to turn.
    func step(to now: TimeInterval, yRunsDown: Bool, aim: () -> CGPoint) -> TimeInterval? {
        takeUpSprites(at: now)
        guard let due else { return nil }
        if now < due - BotPace.slack { return due - now }
        if let clawd {
            let wait = clawd.step(to: now, side: side, scale: scale, outline: outline, space: space, restsHidden: restsHidden,
                                  aim: aim)
            self.due = wait.map { now + $0 }
            stepped = true
            return wait
        }
        motion.step(to: now, aim: motion.morph > 0 || motion.staysBot ? aim() : .zero)
        let wait = motion.wait
        self.due = wait.map { now + $0 }
        // Setting a layer's property, even to what it already is, makes the frame's commit carry the layer.
        if restsHidden, motion.restsAsLogo {
            if !layer.isHidden { layer.isHidden = true }
            return wait
        }
        if layer.isHidden { layer.isHidden = false }
        if motion.opacity != faded {
            faded = motion.opacity
            layer.opacity = Float(faded)
        }
        let look = BotRenderer.look(of: motion)
        if look != shown, side > 0 {
            shown = look
            layer.contents = BotRenderer.image(form, motion: motion, side: side * scale, outline: outline, space: space)
        }
        let transform = Self.transform(motion.pose, side: side, yRunsDown: yRunsDown)
        if transform != placed {
            placed = transform
            layer.setAffineTransform(transform)
        }
        stepped = true
        return wait
    }

    /// Clawd's sprites take over Claude's mark once they are read and while the mark is big enough for them
    /// (`ClawdBaker.drawsSprites`), and give it back to the drawn Clawd otherwise. A bot on screen fades from one into
    /// the other; the motion keeps being told everything, so the drawn Clawd picks up where the sprites leave off.
    private func takeUpSprites(at now: TimeInterval) {
        guard let sprites else { return }
        let sheet = side > 0 && ClawdBaker.drawsSprites(side: side, scale: scale) ? sprites.sheet : nil
        guard sheet !== clawd?.sheet else { return }
        let showing = !layer.isHidden && (layer.contents != nil || clawd != nil)
        // A bot built awake that has shown nothing yet is Clawd from its first frame: nothing to fade from.
        let resumes = awake && !stepped
        if showing, !resumes { IslandAnimation.fadeLook(of: layer, duration: ClawdActor.fade) }
        clawd?.remove()
        clawd = sheet.map { sheet in
            let clawd = ClawdActor(sheet: sheet, form: form, staysBot: staysBot, arrivesAfter: arrivesAfter, seed: seed,
                                   in: layer, showing: showing, resumes: resumes)
            // A staying Clawd arrives when the drawn one did; one that follows its agent takes up its look from now.
            if let firstSet {
                if resumes { clawd.resume(motion.state, at: now) } else { clawd.set(motion.state, at: staysBot ? firstSet : now) }
            }
            if let told, now - told.at <= BotEvent.freshness { clawd.play(told.event, at: told.at) }
            return clawd
        }
        if clawd != nil {
            layer.contents = nil
            layer.setAffineTransform(.identity)
        }
        // Whichever is drawn next starts from what the layer holds now.
        shown = nil
        placed = clawd == nil ? nil : .identity
        faded = CGFloat(layer.opacity)
        due = 0
    }

    /// A pose is written with y running down and turns clockwise; the layer may be laid out either way up.
    nonisolated static func transform(_ pose: BotPose, side: CGFloat, yRunsDown: Bool) -> CGAffineTransform {
        let up: CGFloat = yRunsDown ? 1 : -1
        let foot = CGPoint(x: 0, y: pose.footing * side * up)
        return CGAffineTransform(translationX: pose.x * side + foot.x, y: pose.y * side * up + foot.y)
            .rotated(by: pose.rotation * .pi / 180 * up)
            .scaledBy(x: pose.scaleX, y: pose.scaleY)
            .translatedBy(x: -foot.x, y: -foot.y)
    }
}

/// One timer for every bot on screen. Each follower says how long its bots can go without a frame and the
/// timer fires at the soonest of those: every frame while something moves, a few times a second while bots only
/// breathe, and not until the next visit while every mark rests as its logo. Every wait ends on a frame of the shared
/// clock (`BotPace`), so bots that move at once, on any screen, wake it once.
///
/// Nobody sees a bot whose window is covered or off screen, while the displays sleep, or while another user has the
/// screen. Such followers are left out until they can be seen again; then they step straight to where the clock says
/// they are, without playing what they missed.
@MainActor
final class BotClock {
    static let shared = BotClock(watches: true)

    private struct Follower {
        weak var owner: AnyObject?
        let step: (TimeInterval) -> TimeInterval?
    }

    private var timer: Timer?
    private var followers: [ObjectIdentifier: Follower] = [:]
    /// Followers left out of the last tick because nobody could see them.
    private var unseen: Set<ObjectIdentifier> = []
    /// The displays sleep, or another user has the screen.
    private(set) var screensAsleep = false, sessionAway = false
    /// When the next tick is due, on the clock bots are stepped with; nil while the timer is stopped.
    private(set) var due: TimeInterval?
    private var observers: [NSObjectProtocol] = []

    /// - watches: follows windows, displays and the user session, to leave out what nobody sees. A clock made for a test
    ///   is told of them instead.
    init(watches: Bool = false) {
        guard watches else { return }
        let windows = NotificationCenter.default, workspace = NSWorkspace.shared.notificationCenter
        func on(_ center: NotificationCenter, _ name: Notification.Name, _ act: @escaping @MainActor (BotClock) -> Void) {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { if let self { act(self) } }
            })
        }
        on(windows, NSWindow.didChangeOcclusionStateNotification) { $0.wake() }
        on(workspace, NSWorkspace.screensDidSleepNotification) { $0.screens(asleep: true) }
        on(workspace, NSWorkspace.screensDidWakeNotification) { $0.screens(asleep: false) }
        on(workspace, NSWorkspace.sessionDidResignActiveNotification) { $0.session(away: true) }
        on(workspace, NSWorkspace.sessionDidBecomeActiveNotification) { $0.session(away: false) }
    }

    /// The timer is set: some follower that can be seen has a frame coming.
    var isRunning: Bool { timer != nil }

    /// Calls `step` within a frame and then again after each wait it returns, until it returns nil or `owner` is
    /// dropped. Following again brings the next call forward to now.
    func follow(_ owner: AnyObject, _ step: @escaping (TimeInterval) -> TimeInterval?) {
        let id = ObjectIdentifier(owner)
        followers[id] = Follower(owner: owner, step: step)
        unseen.remove(id)
        fire()
    }

    func drop(_ owner: AnyObject) {
        followers[ObjectIdentifier(owner)] = nil
        if followers.isEmpty { stop() }
    }

    /// Steps every follower that can be seen, and sets the timer for the soonest frame any of them asks for.
    func tick(at now: TimeInterval = CACurrentMediaTime()) {
        var soonest: TimeInterval?
        unseen = []
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for (id, follower) in followers {
            guard let owner = follower.owner else {
                followers[id] = nil
                continue
            }
            guard sees(owner) else {
                unseen.insert(id)
                continue
            }
            if let wait = follower.step(now) { soonest = min(soonest ?? wait, wait) } else { followers[id] = nil }
        }
        CATransaction.commit()
        // Nothing anyone can see is due: the timer stops, and what is left waits to be seen again.
        guard let soonest else { return stop() }
        due = now + soonest
        start()
        // The wait counts from `now`, and stepping took some of it.
        timer?.fireDate = Date(timeIntervalSinceNow: max(0, now + soonest - CACurrentMediaTime()))
    }

    /// Something may be seen again: a window uncovered, the displays awake, the session back. A follower left out of
    /// the last tick that can be seen now steps at once.
    func wake() {
        guard !followers.isEmpty, !screensAsleep, !sessionAway else { return }
        if timer == nil || unseen.contains(where: { followers[$0]?.owner.map(sees) == true }) { fire() }
    }

    func screens(asleep: Bool) {
        screensAsleep = asleep
        if asleep { stop() } else { wake() }
    }

    func session(away: Bool) {
        sessionAway = away
        if away { stop() } else { wake() }
    }

    /// Whether anyone can see what `owner` draws: a view in a window that is on screen and not covered.
    private func sees(_ owner: AnyObject) -> Bool {
        guard !screensAsleep, !sessionAway else { return false }
        guard let window = (owner as? NSView)?.window else { return true }
        return window.isVisible && window.occlusionState.contains(.visible)
    }

    /// Steps every follower within a frame, however long each meant to wait: something they draw from has changed.
    func hurry() {
        if !followers.isEmpty { fire() }
    }

    /// Brings the next tick forward to now.
    private func fire() {
        start()
        let now = Date()
        if let timer, timer.fireDate > now { timer.fireDate = now }
        due = CACurrentMediaTime()
    }

    private func start() {
        guard timer == nil else { return }
        let timer = Timer(timeInterval: BotPace.lively, repeats: true) { [weak self] timer in
            // A clock made for a test may be gone before its timer: the timer then stops itself.
            guard let self else { return timer.invalidate() }
            MainActor.assumeIsolated { self.tick() }
        }
        timer.tolerance = 0.004
        // Common modes, so bots keep moving while a menu is open or a window is being dragged.
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func stop() {
        timer?.invalidate()
        timer = nil
        due = nil
    }
}

/// An agent's mark that turns into its bot. On a reminder it opens as the logo, becomes the bot once the card
/// has opened and stays it for as long as it is shown: asking on a request, smiling on a reply. A mark that
/// follows its agent instead is the bot while the agent works and the logo while it rests, as in a logo queue.
///
/// With bots switched off, or motion reduced, it is the plain logo, wearing a waiting request's or a badge's sign as a
/// still picture over it. The bot is drawn for the ground of the HUD it is on, in light ink on a dark HUD and in dark
/// ink on a light one (`BotForm.ground`).
struct BotMark: View {
    /// How long a reminder's mark stays the logo: the card's own opening, and a moment to read the logo in.
    static let arrival: TimeInterval = IslandAnimation.duration + 0.3

    let vendor: String
    var size: CGFloat = 17
    let state: BotState
    /// The newest event, played once if it is still fresh when the mark is told of it.
    var cue: BotCue? = nil
    var staysBot = true
    @Environment(\.botLogos) private var botLogos
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.hudLook) private var look

    init(vendor: String, size: CGFloat = 17, phase: BotPhase, staysBot: Bool = true) {
        self.init(vendor: vendor, size: size, state: phase.state, staysBot: staysBot)
    }

    init(vendor: String, size: CGFloat = 17, state: BotState, cue: BotCue? = nil, staysBot: Bool = true) {
        self.vendor = vendor
        self.size = size
        self.state = state
        self.cue = cue
        self.staysBot = staysBot
    }

    var body: some View {
        if botLogos, !reduceMotion, BotForm.form(for: vendor, ground: look.appearance) != nil {
            BotStage(vendor: vendor, size: size, state: state, cue: cue, staysBot: staysBot, ground: look.appearance)
                .frame(width: size, height: size)
                .accessibilityHidden(true)
        } else {
            StillMark(vendor: vendor, size: size, sign: state.stillSign, faded: state.look == .noNews)
        }
    }
}

/// An agent's logo, still, wearing a sign over it that never moves: a waiting request's or a badge's, where its bot
/// would carry it, with bots off or motion reduced, or on a card that shows the logo.
struct StillMark: View {
    let vendor: String
    var size: CGFloat = 17
    let sign: BotSign?
    /// The logo shows at half strength, as no news does; its sign does not.
    var faded = false
    @Environment(\.hudLook) private var look
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        AgentLogo(vendor: vendor, size: size)
            .opacity(faded ? 0.5 : 1)
            .overlay(alignment: .topLeading) {
                if let sign,
                   let still = BotSigns.still(sign, form: BotForm.form(for: vendor, ground: look.appearance), side: size,
                                              scale: displayScale, ground: look.appearance) {
                    Image(decorative: still.image, scale: max(1, displayScale))
                        .offset(x: still.frame.minX, y: still.frame.minY)
                        .accessibilityHidden(true)
                }
            }
    }
}

private struct BotStage: NSViewRepresentable {
    let vendor: String
    let size: CGFloat
    let state: BotState
    let cue: BotCue?
    let staysBot: Bool
    let ground: HUDLook.Appearance
    @Environment(\.displayScale) private var displayScale

    func makeNSView(context: Context) -> BotStageView { BotStageView() }

    func updateNSView(_ view: BotStageView, context: Context) {
        view.show(vendor: vendor, size: size, scale: displayScale, state: state, cue: cue, staysBot: staysBot,
                  ground: ground)
    }

    static func dismantleNSView(_ view: BotStageView, coordinator: ()) { BotClock.shared.drop(view) }
}

/// Hosts one bot's layer, centred, with room round it for a hop. A bot for another ground is another bot: a change of
/// the HUD's look builds it again, in the new ground's ink, and fades it in over the old one. A bot is on stage only
/// while motion is on (`BotMark`), so the fade never runs under Reduce Motion.
final class BotStageView: NSView {
    private var actor: BotActor?
    private var vendor = ""
    private var staysBot = true
    private var ground = HUDLook.Appearance.dark
    /// The last event this stage was told of, which a bot built again for another ground does not play again.
    private var played: BotCue?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.masksToBounds = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func show(vendor: String, size: CGFloat, scale: CGFloat, phase: BotPhase, staysBot: Bool = true,
              ground: HUDLook.Appearance) {
        show(vendor: vendor, size: size, scale: scale, state: phase.state, staysBot: staysBot, ground: ground)
    }

    func show(vendor: String, size: CGFloat, scale: CGFloat, state: BotState, cue: BotCue? = nil, staysBot: Bool = true,
              ground: HUDLook.Appearance) {
        if vendor != self.vendor || staysBot != self.staysBot || ground != self.ground || actor == nil {
            // One built again for another ground fades in.
            if actor != nil, vendor == self.vendor, staysBot == self.staysBot, let layer {
                IslandAnimation.fadeLook(of: layer)
            }
            self.vendor = vendor
            self.staysBot = staysBot
            self.ground = ground
            actor?.layer.removeFromSuperlayer()
            actor = BotForm.form(for: vendor, ground: ground).map {
                let actor = BotActor(form: $0, staysBot: staysBot, period: 3, arrivesAfter: staysBot ? BotMark.arrival : 0)
                // Nothing else draws this mark, so the actor draws its logo too.
                actor.restsHidden = false
                return actor
            }
            if let actor { layer?.addSublayer(actor.layer) }
        }
        guard let actor else { return }
        actor.resize(side: size, scale: scale, outline: 0)
        actor.show(in: window?.colorSpace?.cgColorSpace)
        let now = CACurrentMediaTime()
        actor.set(state, at: now)
        if let cue, cue != played {
            played = cue
            // An event the stage hears of too late is let go, as the bot itself would let it go.
            if -cue.at.timeIntervalSinceNow <= BotEvent.freshness { actor.play(cue.event, at: now) }
        }
        needsLayout = true
        follow()
    }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        actor?.layer.position = CGPoint(x: bounds.midX, y: bounds.midY)
        CATransaction.commit()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let window {
            actor?.show(in: window.colorSpace?.cgColorSpace)
            follow()
        } else {
            BotClock.shared.drop(self)
        }
    }

    private func follow() {
        guard window != nil, actor != nil else { return }
        BotClock.shared.follow(self) { [weak self] now in self?.step(to: now) }
    }

    private func step(to now: TimeInterval) -> TimeInterval? {
        guard let actor, let window else { return nil }
        return actor.step(to: now, yRunsDown: layer?.isGeometryFlipped ?? false) {
            let center = window.convertPoint(toScreen: convert(CGPoint(x: bounds.midX, y: bounds.midY), to: nil))
            let pointer = NSEvent.mouseLocation
            // Screen coordinates run up; a bot's gaze is measured with y running down.
            return BotMotion.aim(from: CGPoint(x: center.x, y: -center.y), to: CGPoint(x: pointer.x, y: -pointer.y))
        }
    }
}
