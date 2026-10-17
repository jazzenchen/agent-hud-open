import CoreGraphics
import Foundation

/// The bot's eyes. `look` and `lookY` are shares of a full turn of the head, right and down positive.
struct BotFace: Equatable, Sendable {
    var open: CGFloat = 1
    var look: CGFloat = 0
    var lookY: CGFloat = 0
    var wide: CGFloat = 1
    var happy: CGFloat = 0
    /// Positive grows the right eye and narrows the left, as when peering at something.
    var lean: CGFloat = 0
    /// 1 crosses each eye out, after a failed turn.
    var crossed: CGFloat = 0
}

/// How the whole bot sits: offsets are shares of the mark's side, rotation is in degrees.
struct BotPose: Equatable, Sendable {
    var x: CGFloat = 0
    var y: CGFloat = 0
    var rotation: CGFloat = 0
    var scaleX: CGFloat = 1
    var scaleY: CGFloat = 1
    /// Where it squashes from: 0 is the centre, 0.5 the bottom edge.
    var footing: CGFloat = 0

    func mixed(with other: BotPose, _ share: CGFloat) -> BotPose {
        func mix(_ a: CGFloat, _ b: CGFloat) -> CGFloat { a + (b - a) * share }
        return BotPose(x: mix(x, other.x), y: mix(y, other.y), rotation: mix(rotation, other.rotation),
                       scaleX: mix(scaleX, other.scaleX), scaleY: mix(scaleY, other.scaleY),
                       footing: mix(footing, other.footing))
    }
}

/// Where a sprite is, for a bot drawn as pixel art with limbs (`BotSpec.Limbs`): its arms and legs, where the whole
/// body has stepped or hopped to, and the spinner over its head, all in whole steps of the art. Other bots keep it at
/// rest. The sign over its head is the motion's own (`BotMotion.sign`), as for every bot.
struct BotLimbs: Hashable, Sendable {
    /// How far each arm is raised: 0 at its side, 1 a step up, 2 up and out.
    var left = 0, right = 0
    /// The legs lifted: 0 none, 1 the first and third, 2 the second and fourth, 3 all four, tucked up for a jump.
    var lifted = 0
    /// How far the whole body has stepped aside or hopped, right and down positive.
    var x = 0, y = 0
    /// Over its head while its agent works: Claude Code's spinner, a frame of its star from 1 (a dot) to 6
    /// (the fullest); 0 for none.
    var spinner = 0
}

/// How long a bot can go without a frame. Every frame it is drawn costs a picture or at least a move of its
/// layer, so it asks for no more of them than its motion can show.
///
/// Every frame falls on a whole multiple of its pace on the shared clock, and the paces divide each other, so bots that
/// move at once wake the clock once.
enum BotPace {
    /// A morph, an act, a blink or a hop.
    static let lively: TimeInterval = 1.0 / 24
    /// Eyes following the pointer, and every frame of a sprite.
    static let following: TimeInterval = 1.0 / 12
    /// Nothing but a breath and a slow turn, which move under half a pixel in this long.
    static let calm: TimeInterval = 1.0 / 6
    /// A timer rarely fires on the dot: a step this close to a frame's time is on time.
    static let slack: TimeInterval = 0.004

    /// The first whole multiple of `pace` after `now`, on the shared clock.
    static func next(after now: TimeInterval, every pace: TimeInterval) -> TimeInterval {
        (((now + slack) / pace).rounded(.down) + 1) * pace
    }

    /// The first lively frame at or after `time`, and after `now`.
    static func frame(at time: TimeInterval, after now: TimeInterval) -> TimeInterval {
        max(next(after: now, every: lively), ((time - slack) / lively).rounded(.up) * lively)
    }

    /// `now`, moved onto the frame it is within a slack of, so beats counted from it fall where the clock meant.
    static func onFrame(_ now: TimeInterval) -> TimeInterval {
        let frame = (now / lively).rounded() * lively
        return abs(frame - now) <= slack ? frame : now
    }
}

/// One bot's state over time: how far the logo has turned into the bot, what its eyes are doing and how it
/// sits. It is stepped with a clock and nothing else, so the same seed and the same times give the same bot.
struct BotMotion: Sendable {
    private static let morphSeconds: TimeInterval = 0.5
    /// How long a new look takes to move from the pose the last one left.
    private static let blendSeconds: TimeInterval = 0.3
    /// The most time one step moves the bot by: a calm frame.
    private static let longestStep: TimeInterval = BotPace.calm
    /// How long after a turn ended its mark rests as the logo again.
    private static let doneSeconds: TimeInterval = 2.4
    /// An alert shows its bot for as long as the alert is up; a logo queue lets it settle back into the logo.
    var staysBot = false
    /// Seconds a staying bot keeps its logo after it is first given a phase. A reminder's card is still opening
    /// then, so the mark is read as the agent's logo before it turns into the bot. The phase begins on arrival.
    var arrivesAfter: TimeInterval = 0
    /// Seconds for one breath of the body.
    var period: TimeInterval = 3
    /// Degrees per second the body turns under the face while awake.
    var spin: CGFloat = 0
    /// The body has arms and legs to move, and is moved as pixel art: in whole steps, with no breathing.
    var limbed = false
    /// One step of a limbed body's art, as a share of the mark's side: how far it moves the whole body at a time.
    var stepShare: CGFloat = 0
    /// Its eyes cross out after a failed turn; smaller eyes squint.
    var crossesEyes = false
    /// It keeps a piece of its artwork above the body, which pulses while a request waits.
    var pulses = false
    /// It has hands for fins, which flap (`flap`).
    var finned = false

    private(set) var state = BotState()
    private(set) var since: TimeInterval = -10
    /// 0 is the logo, 1 the bot.
    private(set) var morph: CGFloat = 0
    private(set) var face = BotFace()
    private(set) var pose = BotPose()
    /// Degrees the body has turned.
    private(set) var turn: CGFloat = 0
    /// The piece a mark keeps above its body: how far it has hopped, and how big it is.
    private(set) var kept = (lift: CGFloat(0), scale: CGFloat(1))
    private(set) var limbs = BotLimbs()
    /// How far a fish's hands are raised: on a tool's beats, at a start and over a reply's card, and one waving at a
    /// greeting. Still otherwise, so the clock can rest.
    private(set) var flap = BotFlap()
    /// The sign over its head, if any, and for `…` the dot that is lit.
    private(set) var sign: BotSign?
    private(set) var lit = 0
    /// How opaque the whole mark is: one with no news of its agent rests as a faded logo.
    private(set) var opacity: CGFloat = 1
    /// Seconds from the last step until the bot next needs a frame: up to its next frame on the shared clock while it
    /// moves, the time to its next visit while it waits as its logo, and nothing once it rests for good.
    private(set) var wait: TimeInterval? = BotPace.lively

    private var last: TimeInterval?
    private var leaving: BotPose?
    private var act: Act?
    private var nextAct: TimeInterval = 0
    private var blinks: [TimeInterval] = [-1, -1]
    private var nextBlink: TimeInterval = 2
    /// The eyes are still on their way to where they should look, or a lid is still moving; and whether the look
    /// blinks at all.
    private var following = false, blinking = false, blinksOn = true
    /// The next brief bot visit while idle or asleep, between longer stretches as the original logo.
    private var botInterval: Range<TimeInterval>?
    /// The next act is a wave at whoever just opened the agent's card.
    private var greets = false
    /// When a bot without arms that came up under the pointer hops to greet, and when that hop began; and the side of
    /// the hand a fish waves with, the pointer's.
    private var greetAfter: TimeInterval?, greeting: TimeInterval?, greetSide: CGFloat = 1
    /// The event waiting for the bot to come up, with when it was asked for, and the event playing, with when it began.
    private var pending: (kind: BotEvent, asked: TimeInterval)?
    private var event: (kind: BotEvent, start: TimeInterval)?
    /// The look's first beat, a whole number of beats on the shared clock: a working body's bob and Clawd's typing
    /// count from it, so they land on frames.
    private var beatsFrom: TimeInterval = 0
    /// The side a held thinking look keeps its eyes on.
    private var heldSide: CGFloat = 1
    /// A held look has settled: nothing moves and nothing is due.
    private var settled = false
    private var random: SplitMix

    private struct Act {
        enum Kind: CaseIterable {
            case glance, blinkTwice, squint, widen, smile, inspect, tilt, stretch, wiggle, turn
            /// For a body with limbs: one arm up and waving; a few steps aside and back; a jump.
            case wave, scuttle, hop
        }
        var kind: Kind
        var start: TimeInterval
        var length: TimeInterval
        var side: CGFloat
        var gaze: CGPoint
        var from: CGFloat
    }

    init(seed: UInt64 = UInt64.random(in: 0...UInt64.max)) { random = SplitMix(state: seed) }

    /// Takes on what a body's spec says about how it moves.
    mutating func fit(_ spec: BotSpec) {
        spin = spec.spin
        limbed = spec.limbs != nil
        stepShare = spec.limbs.map { $0.pixel / 2 * spec.radius } ?? 0
        crossesEyes = spec.crossesEyes
        pulses = spec.keeps != nil
        finned = spec.decor.contains { item in
            if case .fins(let fins, _) = item { fins.contains { $0.hand != 0 } } else { false }
        }
    }

    /// The look acted out.
    var look: BotLook { state.look }
    /// The same story as a phase, for callers that still speak in phases.
    var phase: BotPhase { state.phase }
    /// The event playing over the look, if any.
    var playing: BotEvent? { event?.kind }
    /// A small, still copy of the body stands beside it, for sub-agents at work.
    var helper: Bool { state.subagents && look.isWorking }
    /// The mark rests as its plain logo, with nothing on it and nothing faded, for good or until its next visit: a queue
    /// that draws the logo itself can hide the bot.
    var restsAsLogo: Bool {
        guard morph == 0, opacity == 1, sign == nil else { return false }
        return wait == nil || (look.isCalm && !state.hovered && !staysBot)
    }

    /// Nothing left to draw or wait for: the logo is back and at rest, or a held look has settled.
    var isAtRest: Bool {
        guard pending == nil, event == nil, greeting == nil else { return false }
        if settled { return true }
        guard morph == 0, !staysBot, !state.hovered else { return false }
        switch look {
        case .idle, .asleep: return botInterval == nil && pose == BotPose()
        case .noNews: return pose == BotPose()
        case .done: return (last ?? 0) - since > Self.doneSeconds
        default: return false
        }
    }

    mutating func set(_ phase: BotPhase, at now: TimeInterval) { set(phase.state, at: now) }

    /// Acts out `next` from now on. A new look starts afresh; what comes with it — the pointer, a badge, sub-agents —
    /// changes the bot without starting it over.
    mutating func set(_ next: BotState, at now: TimeInterval) {
        let before = state
        state = next
        if next.holds, !before.holds { heldSide = BotMoves.ponderSide(at: now) }
        if next.look != before.look {
            leaving = pose
            since = now + (staysBot && morph == 0 ? max(0, arrivesAfter) : 0)
            beatsFrom = ((since - BotPace.slack) / BotMoves.beat).rounded(.up) * BotMoves.beat
            if next.look.isWorking { nextAct = now + 0.9 + random.unit() * 1.2 }
            botInterval = nil
        }
        if next.hovered != before.hovered { hover(next.hovered, at: now) }
        if visits, botInterval == nil { scheduleVisit(after: now + Double(morph) * Self.morphSeconds) }
    }

    /// Plays `next` over the look once the bot is up, unless it has waited too long by then. The newest event wins,
    /// but never over one waiting that outranks it.
    mutating func play(_ next: BotEvent, at now: TimeInterval) {
        if let waiting = pending, waiting.kind.rank > next.rank { return }
        pending = (next, now)
    }

    /// Takes up the look as a bot that is already up: a mark built again while its bot was out, as a dock's are when
    /// it is laid out again, starts its look where the loop begins, with no morph from the logo, no hop and
    /// no blend from a pose it never had. A look the logo rests in is left as it is.
    mutating func resume(at now: TimeInterval) {
        guard look.isWorking || look.isAttention else { return }
        morph = 1
        leaving = nil
        since = now - (look.isAttention ? BotMoves.firstMove : Self.blendSeconds)
        beatsFrom = ((since - BotPace.slack) / BotMoves.beat).rounded(.up) * BotMoves.beat
    }

    /// Advances to `now`. `aim` is where the bot should look, as returned by `BotMotion.aim(from:to:)`.
    mutating func step(to time: TimeInterval, aim: CGPoint = .zero, hasFace: Bool = true) {
        let now = BotPace.onFrame(time)
        // Time the clock slept through moves nothing: a bot woken for its visit starts from where it stopped.
        let gap = max(0, now - (last ?? now))
        let dt = gap > 2 * Self.longestStep ? 0 : min(Self.longestStep, gap)
        last = now
        let age = now - since
        if visits, let interval = botInterval, now >= interval.upperBound {
            scheduleVisit(after: now + Self.morphSeconds)
        }
        let visiting = visits && botInterval?.contains(now) == true
        // A sleeping mark that shows its bot at all shows it dozing, fading out included.
        let dozing = look == .asleep && !state.hovered
        // Before a staying bot arrives it is the logo at rest: no morph, no pose, no face.
        let arrived = age >= 0
        if let asked = pending, now - asked.asked > BotEvent.freshness { pending = nil }
        if let running = event, now - running.start >= BotMoves.length(of: running.kind, limbed: limbed) { event = nil }
        let awake = arrived && (look.isWorking || look.isAttention || visiting || state.hovered
                                || (look == .done && age < 1.5) || pending != nil || event != nil)

        let wanted: CGFloat = arrived && (awake || staysBot) && hasFace ? 1 : 0
        let stride = CGFloat(dt / Self.morphSeconds)
        morph = abs(wanted - morph) <= stride ? wanted : morph + (wanted > morph ? stride : -stride)
        opacity = look == .noNews ? 0.5 + 0.5 * morph : 1

        // An event plays once the bot is up, over any look but a waiting request's.
        if let next = pending, event == nil, !look.isAttention, morph > 0.9 || !hasFace {
            event = (next.kind, now)
            pending = nil
            act = nil
        }
        // A bot that came up under the pointer hops to say hello.
        if let after = greetAfter, now >= after, morph > 0.9, event == nil, pending == nil {
            greetAfter = nil
            if greetsOnHover {
                greeting = now
                greetSide = aim.x < 0 ? -1 : 1
            }
        }
        // A fish waves on after its hop.
        let greetLength = finned ? max(BotMoves.hopSeconds, BotMoves.finWaveSeconds) : BotMoves.hopSeconds
        if let start = greeting, now - start >= greetLength { greeting = nil }

        // Random life while awake: one small act, a pause, another.
        if let running = act, now - running.start >= running.length {
            if running.kind == .turn { turn = running.from + 60 * running.side }
            act = nil
        }
        let actsInPlace = look.staysInPlace
        if actsInPlace, acts {
            if awake || staysBot, morph > 0.9 || !hasFace, act == nil, now >= nextAct {
                var chosen = pick(at: now)
                // A wave is made with the arm on the pointer's side.
                if chosen.kind == .wave, aim.x != 0 { chosen.side = aim.x > 0 ? 1 : -1 }
                act = chosen
                nextAct = now + chosen.length + 0.5 + random.unit() * 1.9
                if chosen.kind == .blinkTwice { blinks = [now, now + 0.24] }
            }
        } else {
            act = nil
        }
        let progress = act.map { CGFloat((now - $0.start) / $0.length) } ?? 0
        let swell = act == nil ? 0 : Self.easeInOut(min(1, min(progress / 0.22, (1 - progress) / 0.22)))
        if actsInPlace, morph > 0.5, spin != 0 {
            if let act, act.kind == .turn { turn = act.from + 60 * act.side * Self.easeInOut(progress) }
            else { turn += spinRate(at: now, dozing: dozing) * CGFloat(dt) }
        }
        signs(at: now, arrived: arrived, dozing: dozing)

        pose = arrived ? sitting(age: age, progress: progress, swell: swell, dozing: dozing) : BotPose()
        if let leaving, arrived, age < Self.blendSeconds {
            pose = leaving.mixed(with: pose, Self.easeOut(CGFloat(age / Self.blendSeconds)))
        }
        if actsInPlace {
            // Keep the centre fixed, even when returning from an alert's raised pose. The original logo
            // is still while it is shown; breathing and turns arrive with the bot again.
            pose = BotPose().mixed(with: pose, morph)
            pose.x = 0
            pose.y = 0
            pose.footing = 0
            if !limbed, case .tool = look, !state.holds { pose.y = BotMoves.bob(at: now - beatsFrom) * morph }
        } else if morph > 0.5 {
            pose.x += 0.025 * face.look
        }
        if !limbed { played(on: &pose, at: now) }
        limbs = limbed && arrived ? stance(age: age, now: now, dozing: dozing) : BotLimbs()
        flap = finned && arrived ? flapping(age: age, now: now) : BotFlap()
        // A sprite's steps and hops move the whole picture, a step of its art at a time.
        pose.x += CGFloat(limbs.x) * stepShare * morph
        pose.y += CGFloat(limbs.y) * stepShare * morph

        if arrived, !actsInPlace || morph > 0 { look(age: age, now: now, dt: dt, aim: aim, swell: swell, dozing: dozing) }
        else { (following, blinking) = (false, false) }
        if arrived { keep(age: age, now: now, dt: dt) }
        let stills = state.holds && look.isWorking || turnsToCard && age >= Self.hopSeconds
        settled = stills && !state.hovered && morph == wanted && !following && !blinking
            && act == nil && event == nil && pending == nil && greeting == nil
        // Counted from the time the bot was stepped at, so that it lands on the frame meant.
        wait = nextFrame(now: now, age: age, wanted: wanted, dozing: dozing).map { $0 + now - time }
    }

    /// Where a bot centred at `center` should look to face `target`, both in the same y-down coordinates.
    /// Something close by turns the head less than something far away.
    static func aim(from center: CGPoint, to target: CGPoint) -> CGPoint {
        let dx = target.x - center.x, dy = target.y - center.y, distance = hypot(dx, dy)
        guard distance >= 1 else { return .zero }
        let share = distance / (distance + 120)
        return CGPoint(x: dx / distance * share, y: dy / distance * share)
    }

    // MARK: Frames

    /// When the bot next needs a frame: at once while anything moves on its own, at the pace of what moves all along,
    /// or when the next thing it waits for is due, whichever comes first, on the shared clock's frames.
    private func nextFrame(now: TimeInterval, age: TimeInterval, wanted: CGFloat, dozing: Bool) -> TimeInterval? {
        if isAtRest { return nil }
        func at(_ time: TimeInterval) -> TimeInterval { BotPace.frame(at: time, after: now) - now }
        func every(_ pace: TimeInterval) -> TimeInterval { BotPace.next(after: now, every: pace) - now }
        // A logo waiting to arrive shows nothing new until it does.
        if age < 0 { return at(since) }
        if morph == 0, wanted == 0 {
            // The logo is showing: nothing moves before a mark's next visit, or a finished turn's rest.
            if visits, let visit = botInterval { return at(visit.lowerBound) }
            if look == .done { return at(since + Self.doneSeconds + 0.01) }
            return every(BotPace.lively)
        }
        if morph != wanted || act != nil || blinking || age < Self.blendSeconds || event != nil || pending != nil || greeting != nil {
            return every(BotPace.lively)
        }
        var pace: TimeInterval? = following ? BotPace.following
            : breathes(dozing) || spinRate(at: now, dozing: dozing) != 0 ? BotPace.calm : nil
        var next = TimeInterval.infinity
        if blinksOn { next = min(next, nextBlink) }
        if acts { next = min(next, nextAct) }
        if let after = greetAfter { next = min(next, after) }
        if visits, let visit = botInterval { next = min(next, visit.upperBound) }
        switch look {
        case _ where turnsToCard:
            // The hop and the raise, and then nothing until something changes.
            if age < Self.hopSeconds { return every(BotPace.lively) }
        case .permission, .question, .plan:
            // The hop and every move are drawn whole. A kept piece pulses for the first seconds of a wait, and after
            // them the bot is still until its next move.
            if age < BotMoves.firstMove || moving(at: age) { return every(BotPace.lively) }
            if pulses, age < BotMoves.fullWait { pace = BotPace.following }
            next = min(next, since + BotMoves.move(at: age).next)
        case .done:
            if age < 1.5 { return every(BotPace.lively) }
        case .tool where !limbed && !state.holds:
            // The bob comes a beat at a time through a burst, which is often enough for its breath too.
            if BotMoves.inBurst(now - beatsFrom) { pace = following ? BotPace.following : nil }
            next = min(next, beatsFrom + BotMoves.nextBob(after: now - beatsFrom))
        case .thinking where !state.holds && !state.hovered:
            next = min(next, BotMoves.nextPonder(after: now))
            if sign == .ellipsis { next = min(next, ((now * 3 + 1e-6).rounded(.down) + 1) / 3) }
        default:
            break
        }
        if limbed, stanceBeats { next = min(next, BotPace.next(after: now, every: BotMoves.beat)) }
        let byPace = pace.map(every), byTime = next.isFinite ? at(next) : nil
        guard let byPace else { return byTime }
        return min(byPace, byTime ?? byPace)
    }

    // MARK: Body

    /// The look rests as its logo between visits as its bot: an idle mark's every 20–35 s, a sleeping one's every
    /// 3–5 min. A bot that stays, or one under the pointer, is up throughout.
    private var visits: Bool { !staysBot && !state.hovered && (look == .idle || look == .asleep) }

    /// The look leaves room for random acts: work that gives no sign of its own, a visit, a bot under the pointer.
    private var acts: Bool {
        guard event == nil, greeting == nil, !state.holds else { return false }
        if state.hovered { return look.staysInPlace }
        switch look {
        case .plain, .tool, .idle, .noNews: return true
        default: return false
        }
    }

    /// A mark under the pointer greets when it was resting or wears a badge; a busy one only watches.
    private var greetsOnHover: Bool { state.hovered && !look.isAttention && (look.isCalm || state.badge != nil) }

    /// A waiting request whose card stands beside the mark: the bot hops and rises as ever, then keeps still with its
    /// eyes on the card, making none of its moves.
    private var turnsToCard: Bool { state.facing != nil && look.isAttention }

    /// Seconds a waiting request's hop lasts.
    private static let hopSeconds: TimeInterval = 0.45

    /// The body breathes: a bot that is not pixel art, in a look that sits in place, unless it dozes or holds still.
    private func breathes(_ dozing: Bool) -> Bool { !limbed && look.staysInPlace && !state.holds && !dozing }

    /// Degrees per second the body turns: a spinning body's own speed, faster through a burst of work and still between
    /// bursts, and not at all while it dozes or holds.
    private func spinRate(at now: TimeInterval, dozing: Bool) -> CGFloat {
        guard spin != 0, look.staysInPlace, morph > 0.5, !state.holds, !dozing else { return 0 }
        if case .tool = look { return BotMoves.inBurst(now - beatsFrom) ? BotMoves.busySpin : 0 }
        return spin
    }

    private func moving(at age: TimeInterval) -> Bool {
        guard !turnsToCard, let start = BotMoves.move(at: age).start else { return false }
        return age - start < BotMoves.moveLength(look)
    }

    private mutating func hover(_ on: Bool, at now: TimeInterval) {
        greetAfter = nil
        botInterval = nil
        guard on else {
            greets = false
            return
        }
        if limbed {
            // A body with arms greets whoever opened its card: a wave as soon as it has turned into its bot.
            greets = true
            act = nil
            nextAct = now
        } else {
            // Any other bot hops once the pointer has stayed a moment, then watches it.
            greetAfter = now + BotMoves.hopSeconds
            nextAct = now + 0.9 + random.unit() * 1.2
        }
    }

    private mutating func scheduleVisit(after now: TimeInterval) {
        let start = now + (look == .asleep ? 180 + random.unit() * 120 : 20 + random.unit() * 15)
        // Fade-in is part of this interval; fade-out follows it. Together the visit lasts 3–5 seconds.
        botInterval = start..<(start + 3 + random.unit() * 2 - Self.morphSeconds)
        nextAct = start + Self.morphSeconds
    }

    private func sitting(age: TimeInterval, progress: CGFloat, swell: CGFloat, dozing: Bool) -> BotPose {
        var pose = BotPose()
        let t = CGFloat(age)
        switch look {
        case .permission, .question, .plan:
            if t < Self.hopSeconds {
                let x = t / Self.hopSeconds, arc = sin(.pi * x)
                pose.y = -0.26 * 4 * x * (1 - x)
                pose.scaleY = 1 + 0.08 * arc
                pose.scaleX = 1 - 0.05 * arc
            }
            let raised = 1 + 0.12 * Self.easeOutBack(min(1, t / 0.35))
            pose.scaleX *= raised
            pose.scaleY *= raised
            // A move every few seconds, until it is answered: a knock, a tilt to the other side, two nods. A body with
            // limbs jumps for its knock instead (`stance`). A bot turned to its card makes none.
            guard !turnsToCard else { break }
            let start = BotMoves.move(at: age).start
            switch look {
            case .permission where !limbed: pose.rotation = start.map { BotMoves.knock(age - $0) } ?? 0
            case .question where !limbed: pose.rotation = BotMoves.questionTilt(at: age)
            case .plan where !limbed: pose.y += start.map { BotMoves.nod(age - $0) } ?? 0
            default: break
            }
        case .done:
            if t < 0.6 {
                let pop = sin(.pi * t / 0.6)
                pose.scaleX = 1 + 0.2 * pop
                pose.scaleY = 1 + 0.2 * pop
                pose.y = -0.1 * pop
            }
        default:
            // Breathing changes the body's shape around its fixed centre. Pixel art keeps its shape.
            if breathes(dozing) {
                let breath = 2 * CGFloat.pi * CGFloat(age / period)
                pose.scaleX = 1 + 0.03 * sin(2 * breath)
                pose.scaleY = 1 - 0.03 * sin(2 * breath)
            }
            // A thinking head leans toward where it looks.
            if look == .thinking, !limbed {
                pose.rotation = BotMoves.ponderTilt * max(-1, min(1, face.look / BotMoves.ponderGaze.x))
            }
        }
        // A sprite's acts move it in steps (`stance`), never by turning or stretching the picture.
        guard let act, !limbed else { return pose }
        switch act.kind {
        case .tilt: pose.rotation += act.side * 9 * swell
        case .stretch:
            let arc = sin(.pi * progress)
            pose.scaleY *= 1 + 0.06 * arc
            pose.scaleX *= 1 - 0.04 * arc
        case .wiggle: pose.rotation += 8 * sin(2 * .pi * 3 * progress) * (1 - progress)
        default: break
        }
        return pose
    }

    /// What a greeting and the event playing add to how a bot without limbs sits, on top of its look.
    private func played(on pose: inout BotPose, at now: TimeInterval) {
        if let start = greeting { pose.y += BotMoves.hop(now - start) * morph }
        guard let event else { return }
        let time = now - event.start
        switch event.kind {
        case .start:
            pose.y += BotMoves.hop(time) * morph
        case .failed:
            let sag = BotMoves.sag(time)
            pose.x += BotMoves.shake(time) * morph
            pose.rotation += 6 * sag * morph
            pose.y += 0.02 * sag * morph
        case .limitHit:
            let sigh = BotMoves.sigh(time)
            pose.scaleX *= 1 + (sigh.scaleX - 1) * morph
            pose.scaleY *= 1 + (sigh.scaleY - 1) * morph
            pose.footing = 0.5
        }
    }

    /// How far a fish's hands are raised now: both on every beat of a tool's bob through a burst, both in a flap at a
    /// start and in three over a reply's card, and the one on the pointer's side waving at a greeting, the left one while
    /// the copy for sub-agents stands on the right.
    private func flapping(age: TimeInterval, now: TimeInterval) -> BotFlap {
        if let event {
            return event.kind == .start ? BotFlap(both: BotMoves.finFlap(now - event.start) * morph) : BotFlap()
        }
        if let start = greeting {
            // The copy for sub-agents stands in front of the hand on the right (`BotRenderer.helper`).
            let wave = BotMoves.finWave(now - start) * morph
            return greetSide < 0 || helper ? BotFlap(left: wave) : BotFlap(right: wave)
        }
        switch look {
        case .tool where !state.holds:
            return BotFlap(both: BotMoves.bob(at: now - beatsFrom) > 0 ? BotMoves.paddle * morph : 0)
        case .done:
            return BotFlap(both: BotMoves.cheer(age) * morph)
        default:
            return BotFlap()
        }
    }

    // MARK: Face

    private mutating func look(age: TimeInterval, now: TimeInterval, dt: TimeInterval, aim: CGPoint, swell: CGFloat,
                               dozing: Bool) {
        var target = BotFace()
        var every: ClosedRange<TimeInterval>? = 3.2...6.5
        switch look {
        case .permission, .question, .plan:
            // Straight at the user, wide open, and no blinking until the stare has landed; under the pointer, at the
            // pointer. A long wait is still between its moves, eyes too.
            target.wide = 1.22
            if let facing = state.facing {
                target.look = facing.aim.x
                target.lookY = facing.aim.y
            } else if state.hovered {
                target.look = aim.x
                target.lookY = aim.y
            }
            // Down at the plan as it nods.
            if look == .plan, moving(at: age) { target.lookY = 0.5 }
            every = age < 1.5 || age >= BotMoves.fullWait || turnsToCard ? nil : 4...7
        case .done:
            if age < 1.25 {
                target.happy = 1
                every = nil
            } else {
                target.look = aim.x
                target.lookY = aim.y
            }
        case .thinking where !state.hovered:
            // Up and to one side, then the other, as when working something out.
            let side = state.holds ? heldSide : BotMoves.ponderSide(at: now)
            target.look = BotMoves.ponderGaze.x * side
            target.lookY = BotMoves.ponderGaze.y
            every = 2.2...4.5
        case _ where dozing:
            target.open = 0
            every = nil
        case .compacting where limbed:
            // Eyes shut, as at meditation.
            target.open = 0
            every = nil
        default:
            target.look = aim.x
            target.lookY = aim.y
            every = 2.2...4.5
            if let act {
                switch act.kind {
                case .glance:
                    // A glance looks away and comes back.
                    target.look += (act.gaze.x * 0.8 - target.look) * swell
                    target.lookY += (act.gaze.y - target.lookY) * swell
                case .squint: target.open = 1 - 0.55 * swell
                case .widen: target.wide = 1 + 0.2 * swell
                case .smile: target.happy = swell > 0.4 ? 1 : 0
                case .inspect:
                    target.lean = act.side * swell
                    target.look += (act.side * 0.6 - target.look) * swell
                    target.lookY -= target.lookY * swell
                case .stretch where limbed: target.open = 1 - 0.9 * swell
                case .hop: target.wide = 1 + 0.2 * swell
                default: break
                }
            }
        }
        if state.holds { every = nil }
        // A start opens the eyes of a bot that comes up with them shut.
        if pending?.kind == .start { target.open = 0 }
        if let event {
            let time = now - event.start
            switch event.kind {
            case .start: target.open = CGFloat(min(1, time / BotMoves.hopSeconds))
            case .failed:
                if crossesEyes { target.crossed = 1 } else { target.open = 0.45 }
                target.lookY = 0.35 * BotMoves.sag(time)
            case .limitHit:
                target.open = limbed ? (time < 4 * BotMoves.beat ? 0 : 0.5) : BotMoves.sigh(time).open
                target.lookY = 0.3
            }
            every = nil
        }
        blinksOn = every != nil

        let eyes = CGFloat(1 - exp(-dt * 18)), head = CGFloat(1 - exp(-dt * 9))
        face.look += (target.look - face.look) * head
        face.lookY += (target.lookY - face.lookY) * head
        face.wide += (target.wide - face.wide) * eyes
        face.happy += (target.happy - face.happy) * eyes
        face.lean += (target.lean - face.lean) * eyes
        face.crossed += (target.crossed - face.crossed) * eyes

        var lid = target.open
        if let every {
            if now >= nextBlink {
                blinks = [now, -1]
                nextBlink = now + every.lowerBound + random.unit() * (every.upperBound - every.lowerBound)
            }
        } else if nextBlink < now {
            nextBlink = now + 1.2
        }
        for start in blinks {
            let blink = (now - start) / 0.17
            if blink >= 0, blink < 1 { lid *= 1 - CGFloat(sin(.pi * blink)) }
        }
        face.open += (lid - face.open) * CGFloat(1 - exp(-dt * 60))
        // Finer than the steps a picture is drawn in, so the last frame of a move is the settled one.
        blinking = abs(lid - face.open) > 0.01 || blinks.contains { now >= $0 && now - $0 < 0.17 }
        following = abs(target.look - face.look) > 0.004 || abs(target.lookY - face.lookY) > 0.004
            || abs(target.wide - face.wide) > 0.004 || abs(target.happy - face.happy) > 0.01
            || abs(target.lean - face.lean) > 0.01 || abs(target.crossed - face.crossed) > 0.01
    }

    private mutating func keep(age: TimeInterval, now: TimeInterval, dt: TimeInterval) {
        guard !look.staysInPlace else {
            kept = (lift: 0, scale: 1)
            return
        }
        var lift: CGFloat = 0, scale: CGFloat = 1
        if look.isAttention {
            // Up and swollen while a request waits; pulsing for its first seconds, then only with each move.
            lift = 0.03
            scale = 1.22
            if !turnsToCard, age < BotMoves.fullWait || moving(at: age) { scale += 0.08 * CGFloat(sin(2 * .pi * now / 0.8)) }
        } else if look == .done, age < 0.6 {
            lift = 0.22 * CGFloat(sin(.pi * age / 0.6))
        }
        let ease = CGFloat(1 - exp(-dt * 22))
        kept.lift += (lift - kept.lift) * ease
        kept.scale += (scale - kept.scale) * ease
    }

    /// The one sign the bot wears: a waiting request's, a failure's while it plays and after as a badge, a limit's
    /// badge, a thinking bot's dots and a dozing bot's z, the first of them that applies.
    private mutating func signs(at now: TimeInterval, arrived: Bool, dozing: Bool) {
        var shown: BotSign?
        if arrived {
            switch look {
            case .permission: shown = .exclamation
            case .question: shown = .question
            case .plan: shown = .lines
            default: break
            }
        }
        if event?.kind == .failed { shown = BotSign.shown(shown, .cross) }
        if let badge = state.badge { shown = BotSign.shown(shown, BotSign(badge)) }
        // Clawd says it thinks with its spinner, not with dots.
        if morph > 0, look == .thinking, !limbed { shown = BotSign.shown(shown, .ellipsis) }
        if morph > 0, dozing { shown = BotSign.shown(shown, .z) }
        sign = shown
        lit = shown == .ellipsis && !state.holds ? Int((now * 3 + 1e-6).rounded(.down)) % 3 : 0
    }

    // MARK: Limbs

    /// Where the sprite is now, in whole steps of its art. While its agent works, Claude Code's spinner turns over its
    /// head and the sprite types, a hand at a time, in bursts of three seconds with a pause between; thinking, it only
    /// watches the spinner, and compacting it sits with its eyes shut. While a request waits the sprite waves both arms
    /// in turn through the hop and jumps through each move, and keeps a hand up between them. A reply gets a dance:
    /// arms up in turn and a jump on every other beat. Dozing, it sits. An event takes the whole body; otherwise
    /// whatever the act in hand moves.
    private func stance(age: TimeInterval, now: TimeInterval, dozing: Bool) -> BotLimbs {
        if let event { return Self.stance(for: event.kind, at: now - event.start) }
        var limbs = BotLimbs()
        func beat(_ seconds: TimeInterval, _ every: TimeInterval) -> Int { Int(max(0, seconds) / every + 1e-6) }
        let clock = now - beatsFrom
        switch look {
        case .thinking, .tool, .plain:
            // Its six shapes out and back, unless a badge stands where it turns. A held look keeps it at a dot.
            if sign == nil {
                let frame = state.holds ? 0 : beat(now, BotMoves.beat) % 10
                limbs.spinner = frame < 6 ? frame + 1 : 11 - frame
            }
            if look != .thinking, !state.holds, BotMoves.inBurst(clock) {
                let tap = beat(clock, BotMoves.beat)
                limbs.left = tap % 2
                limbs.right = 1 - limbs.left
                // Leaning into the keys every other pair of taps.
                limbs.y = tap / 2 % 2
            }
        case .compacting:
            (limbs.left, limbs.right, limbs.lifted) = (1, 1, 3)
        case _ where turnsToCard:
            // A hand up on the card's side, held. A card above or below gets none, which would point past it.
            if state.facing == .left { limbs.left = 2 } else if state.facing == .right { limbs.right = 2 }
        case .permission, .question, .plan:
            let move = BotMoves.move(at: age), into = move.start.map { age - $0 }
            if age < BotMoves.firstMove || moving(at: age) {
                limbs.left = 1 + beat(clock, BotMoves.beat) % 2
                limbs.right = 3 - limbs.left
                if let into, beat(into, BotMoves.beat) % 2 == 0 {
                    limbs.y = -2
                    limbs.lifted = 3
                }
            } else {
                limbs.right = 2
            }
        case .done:
            if age < 1.5 {
                let step = beat(clock, 2 * BotMoves.beat)
                limbs.left = step % 2 == 0 ? 2 : 0
                limbs.right = 2 - limbs.left
                if step % 2 == 1 {
                    limbs.y = -2
                    limbs.lifted = 3
                }
            }
        case .asleep where dozing:
            (limbs.lifted, limbs.y) = (3, 1)
        default:
            break
        }
        guard let act, look.staysInPlace else { return limbs }
        let since = now - act.start, ending = since > act.length - 0.15
        func frame(_ count: Int) -> Int { min(count - 1, beat(since, act.length / Double(count))) }
        // An act of the body's takes its arms and legs from whatever they were doing; the face's acts leave them.
        if [.wave, .scuttle, .hop, .stretch].contains(act.kind) { (limbs.left, limbs.right, limbs.y) = (0, 0, 0) }
        switch act.kind {
        case .wave:
            // A small jump to say hello, then the arm on the pointer's side up and waving.
            if since < 0.24 { limbs.y = -1 }
            let arm = since < 0.12 || ending ? 1 : 2 - beat(since - 0.12, 0.16) % 2
            if act.side < 0 { limbs.left = arm } else { limbs.right = arm }
        case .scuttle:
            // Sideways, as crabs go: two steps out, back past where it stood, and home, a pair of legs up on each step.
            let path = [0, 1, 2, 1, 0, -1, -2, -1, 0], step = frame(path.count)
            limbs.x = path[step] * (act.side < 0 ? -1 : 1)
            limbs.lifted = step == 0 || step == path.count - 1 ? 0 : 1 + step % 2
        case .hop:
            let at = Self.hopPath[frame(Self.hopPath.count)]
            (limbs.y, limbs.lifted, limbs.left, limbs.right) = (at.y, at.lifted, at.arms, at.arms)
        case .stretch:
            // Arms up and out and a step taller, eyes shut, as at a yawn.
            if since > 0.15, !ending { (limbs.left, limbs.right, limbs.y) = (2, 2, -1) }
        default:
            break
        }
        return limbs
    }

    /// A jump: down onto bent legs, up three steps with the arms flung up, and down onto bent legs again.
    private static let hopPath: [(y: Int, lifted: Int, arms: Int)] = [(1, 3, 0), (-2, 0, 1), (-3, 0, 2), (-3, 0, 2),
                                                                     (-2, 0, 1), (0, 0, 0), (1, 3, 0), (0, 0, 0)]

    /// The sprite through an event, `time` seconds in: a start is a jump for joy, a beat a frame; a failure shakes it
    /// a step each way six times a second and then slumps it; a limit stretches it up with its arms out and then sinks it.
    private static func stance(for event: BotEvent, at time: TimeInterval) -> BotLimbs {
        var limbs = BotLimbs()
        func beat(_ every: TimeInterval) -> Int { Int(max(0, time) / every + 1e-6) }
        switch event {
        case .start:
            let at = hopPath[min(hopPath.count - 1, beat(BotMoves.beat))]
            (limbs.y, limbs.lifted, limbs.left, limbs.right) = (at.y, at.lifted, at.arms, at.arms)
        case .failed:
            if time < 0.5 { limbs.x = beat(1.0 / 12) % 2 == 0 ? 1 : -1 } else { (limbs.y, limbs.lifted) = (1, 3) }
        case .limitHit:
            if time < 4 * BotMoves.beat { (limbs.left, limbs.right, limbs.y) = (2, 2, -1) } else { (limbs.y, limbs.lifted) = (1, 3) }
        }
        return limbs
    }

    /// The sprite's limbs change on the beat grid: its spinner turns, or it types.
    private var stanceBeats: Bool {
        guard !state.holds else { return false }
        switch look {
        case .tool, .plain: return true
        case .thinking: return sign == nil
        default: return false
        }
    }

    // MARK: Acts

    private mutating func pick(at now: TimeInterval) -> Act {
        var pool: [(Act.Kind, Double)] = [(.glance, 5), (.blinkTwice, 2), (.squint, 2), (.widen, 2), (.smile, 2),
                                          (.inspect, 2), (.tilt, 3), (.stretch, 2), (.wiggle, 2)]
        if spin != 0 { pool.append((.turn, 3)) }
        if limbed {
            // A sprite does not tilt or wobble; its stretch reaches up. Now and then it steps aside, jumps or waves
            // between bursts of typing; a wave and a jump for whoever looks; and a short visit is always one of its
            // own moves.
            pool.removeAll { $0.0 == .tilt || $0.0 == .wiggle }
            if state.hovered { pool += [(.wave, 4), (.hop, 2), (.scuttle, 2)] }
            else if look.isWorking { pool += [(.scuttle, 4), (.hop, 3), (.wave, 1)] }
            else if look.isCalm { pool = [(.scuttle, 3), (.hop, 3), (.stretch, 3), (.wave, 2)] }
        }
        var roll = random.unit() * pool.reduce(0) { $0 + $1.1 }
        var kind = pool[0].0
        for (candidate, weight) in pool {
            roll -= weight
            if roll < 0 { kind = candidate; break }
        }
        if greets {
            greets = false
            if greetsOnHover { kind = .wave }
        }
        let length: TimeInterval
        switch kind {
        case .glance: length = 1.1 + random.unit() * 0.7
        case .blinkTwice: length = 0.5
        case .squint: length = 1.2
        case .widen: length = 0.9
        case .smile: length = 1.1
        case .inspect: length = 1.5
        case .tilt: length = 1.1
        case .stretch: length = 0.55
        case .wiggle: length = 0.6
        case .turn: length = 0.7
        case .wave: length = 1.4
        case .scuttle: length = 1.1
        case .hop: length = 0.8
        }
        let angle = random.unit() * 2 * .pi
        return Act(kind: kind, start: now, length: length, side: random.unit() < 0.5 ? -1 : 1,
                   gaze: CGPoint(x: cos(angle), y: sin(angle) * 0.7), from: turn)
    }

    // MARK: Curves

    static func easeInOut(_ x: CGFloat) -> CGFloat { x < 0.5 ? 2 * x * x : 1 - pow(-2 * x + 2, 2) / 2 }
    static func easeOut(_ x: CGFloat) -> CGFloat { 1 - pow(1 - x, 3) }
    static func easeOutBack(_ x: CGFloat) -> CGFloat {
        let c: CGFloat = 1.70158
        return 1 + (c + 1) * pow(x - 1, 3) + c * pow(x - 1, 2)
    }
}

/// A small seeded generator, so a bot's random acts can be replayed in a test.
struct SplitMix: Sendable {
    var state: UInt64

    mutating func next() -> UInt64 {
        state &+= 0x9e37_79b9_7f4a_7c15
        var z = state
        z = (z ^ (z >> 30)) &* 0xbf58_476d_1ce4_e5b9
        z = (z ^ (z >> 27)) &* 0x94d0_49bb_1331_11eb
        return z ^ (z >> 31)
    }

    /// 0 up to, not including, 1.
    mutating func unit() -> Double { Double(next() >> 11) / Double(1 << 53) }
}
