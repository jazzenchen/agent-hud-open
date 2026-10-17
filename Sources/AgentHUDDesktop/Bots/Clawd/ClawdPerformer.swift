import CoreGraphics
import Foundation

/// A Claude mark's state, as Clawd's own clips act it out. It is told what every bot is told, the mark's state and its
/// events, and asks its `ClawdPlayer` for the clip of each: a look's loop, an event over it, a card's clip. While the mark
/// rests as its logo it brings Clawd in for a visit now and then, and under the pointer it greets and follows the pointer
/// with its eyes.
///
/// It is the part `BotMotion` plays for every other bot, and like it is stepped with a clock and nothing else: the same
/// seed and the same times give the same pictures, and `due` says exactly when the next one changes.
struct ClawdPerformer: Sendable {
    /// How long the pointer rests on a mark before its Clawd greets it.
    static let greetAfter: TimeInterval = 0.3

    private(set) var player: ClawdPlayer
    private(set) var state = BotState()
    /// A card keeps its Clawd, standing once its clip is over. A mark that follows its agent rests as its logo.
    let staysBot: Bool
    /// Seconds a staying Clawd keeps its logo after it is first told a state: its card is still opening.
    let arrivesAfter: TimeInterval
    /// When the picture next changes, as of the last step; nil while it holds until something is asked for.
    private(set) var due: TimeInterval?

    /// It has been told a state.
    private var told = false
    /// When a staying Clawd comes on, from its first state; nil until it is told one.
    private var arrival: TimeInterval?
    /// The player has been told the state: a mark that follows its agent from the first, a staying one once it has arrived.
    private var onStage: Bool
    /// The next visit while the mark rests as its logo.
    private var nextVisit: TimeInterval?
    /// A visit has begun and not yet ended.
    private var visiting = false
    /// When the pointer has rested long enough on the mark for a greeting.
    private var greetAt: TimeInterval?
    /// Clawd has greeted the pointer and follows it with its eyes.
    private var watching = false
    /// The event asked for last, and when: one that has not begun within `BotEvent.freshness` is let go.
    private var asked: (event: BotEvent, at: TimeInterval)?
    private var random: SplitMix

    init(sheet: ClawdSheet?, clips: ClawdClips = .standard, staysBot: Bool, arrivesAfter: TimeInterval = 0,
         seed: UInt64 = UInt64.random(in: 0...UInt64.max)) {
        player = ClawdPlayer(clips: clips, sheet: sheet, rest: staysBot ? .standing : .logo)
        self.staysBot = staysBot
        self.arrivesAfter = staysBot ? max(0, arrivesAfter) : 0
        onStage = !staysBot
        random = SplitMix(state: seed)
    }

    /// What to draw: a frame of Clawd, or nil for the logo.
    var picture: ClawdPicture? { onStage ? player.picture : nil }

    /// The sign the logo wears while it shows: a badge's. A request's comes with the bot.
    var logoSign: BotSign? { state.badge.map(BotSign.init) }

    // MARK: Requests

    /// Acts out `next` from now on. A new look is shown as its clip allows; what comes with it, the pointer, a badge,
    /// sub-agents, changes what plays without starting it over.
    mutating func set(_ next: BotState, at now: TimeInterval) {
        let before = told ? state : nil
        state = next
        told = true
        if staysBot, arrival == nil { arrival = Self.onGrid(now + arrivesAfter) }
        guard onStage else { return }
        apply(from: before, at: now)
    }

    /// Takes up `next` as a Clawd already showing it would be: a mark built again while Clawd was out, as a dock's are
    /// when it is laid out again, starts its look's loop with no intro. Anything else is told as `set` tells it.
    mutating func resume(_ next: BotState, at now: TimeInterval) {
        set(next, at: now)
        guard onStage, next.facing == nil || !next.look.isAttention, let look = Self.look(for: next) else { return }
        player.resume(look, subagents: next.subagents, at: now)
    }

    /// Plays `event` once the look allows, unless it has waited too long by then. The newest event wins, but never over
    /// one waiting that outranks it.
    mutating func play(_ event: BotEvent, at now: TimeInterval) {
        letGoOfStaleEvent(at: now)
        if let waiting = asked, player.pendingEvent != nil || !onStage, waiting.event.rank > event.rank { return }
        asked = (event, now)
        if onStage { player.play(ClawdEvent(event), at: now) }
    }

    /// Advances to `now`, and works out when the picture next changes. `aim` is where the pointer is, as
    /// `BotMotion.aim(from:to:)` gives it; it is asked only while Clawd's eyes follow it.
    mutating func step(to now: TimeInterval, aim: () -> CGPoint) {
        if !onStage, let arrival, now >= arrival - BotPace.slack {
            onStage = true
            // An event asked for while the card opened plays first, if it is still fresh.
            if let asked, arrival - asked.at <= BotEvent.freshness { player.play(ClawdEvent(asked.event), at: arrival) }
            apply(from: nil, at: arrival)
        }
        letGoOfStaleEvent(at: now)
        if let at = greetAt, now >= at - BotPace.slack {
            greetAt = nil
            // Only a resting Clawd greets: a busy one keeps to its clip.
            if state.look.isCalm {
                player.greet(at: now)
                watching = true
            }
        }
        if watching { player.look(toward: aim(), at: now) }
        if visits, let at = nextVisit, now >= at - BotPace.slack {
            let idle = ClawdVisit.idle
            player.visit(state.look == .asleep ? .dozing : idle[Int(random.unit() * Double(idle.count))], at: now)
            visiting = player.picture != nil
            nextVisit = nil
            // A visit the player turned away waits for the next one.
            if !visiting { scheduleVisit(after: now) }
        }
        player.advance(to: now)
        if visiting, player.picture == nil {
            visiting = false
            scheduleVisit(after: now)
        }
        var soonest = onStage ? player.due : arrival
        func take(_ time: TimeInterval?) { if let time { soonest = min(soonest ?? time, time) } }
        take(greetAt)
        if visits { take(nextVisit) }
        // The eyes turn to whichever eighth of the circle the pointer moves into, looked at every sprite frame.
        if watching { take(BotPace.next(after: now, every: BotPace.following)) }
        due = soonest
    }

    // MARK: Acting it out

    /// Tells the player the state, or what changed in it since `before`: nil when it is told for the first time.
    private mutating func apply(from before: BotState?, at now: TimeInterval) {
        letGoOfStaleEvent(at: now)
        if state.look == .done {
            if before?.look != .done || before?.card != state.card { player.card(ClawdCard(state.card), at: now) }
        } else if state.look.isAttention, let facing = state.facing, let look = Self.look(for: state) {
            // Its card says what it waits for: Clawd turns to it, held still, and the next look asked for ends it. It
            // points at a card beside it; pointing would aim past one above or below, so there it stands with its sign.
            if before?.facing != facing || before?.look != state.look {
                switch facing {
                case .left, .right: player.pointAtCard(mirrored: facing == .left, at: now)
                case .up, .down: player.standAtCard(for: look, at: now)
                }
            }
        } else {
            player.show(Self.look(for: state), subagents: state.subagents, at: now)
        }
        player.set(badge: state.badge.map(ClawdSign.init), at: now)
        if state.hovered != (before?.hovered ?? false) {
            if state.hovered {
                greetAt = Self.onGrid(now + Self.greetAfter)
            } else {
                greetAt = nil
                watching = false
                player.endHover(at: now)
            }
        }
        // Eyes on the pointer are for a Clawd at rest; a look that comes in ends them.
        if !state.look.isCalm { watching = false }
        if state.look != before?.look || state.hovered != before?.hovered {
            visiting = false
            nextVisit = nil
            if visits { scheduleVisit(after: now) }
        }
    }

    /// The look rests as its logo between visits as Clawd: an idle mark's every 20–35 s, a sleeping one's every 3–5 min.
    /// A Clawd that stays, or one under the pointer, has no visits.
    private var visits: Bool { !staysBot && !state.hovered && (state.look == .idle || state.look == .asleep) }

    private mutating func scheduleVisit(after now: TimeInterval) {
        let wait = state.look == .asleep ? 180 + random.unit() * 120 : 20 + random.unit() * 15
        nextVisit = Self.onGrid(now + wait)
    }

    /// An event the player still keeps waiting, because the look had no point to play it at, is let go once it is stale.
    private mutating func letGoOfStaleEvent(at now: TimeInterval) {
        guard let asked, now - asked.at > BotEvent.freshness else { return }
        if onStage, player.pendingEvent != nil { player.cancelEvent() }
        self.asked = nil
    }

    /// The first frame of Clawd's 1/12 s grid at or after `time`.
    private static func onGrid(_ time: TimeInterval) -> TimeInterval {
        let perSecond = Double(ClawdTicks.perSecond)
        return ((time - BotPace.slack) * perSecond).rounded(.up) / perSecond
    }

    /// The clip a state's look is played with; nil for the logo.
    static func look(for state: BotState) -> ClawdLook? {
        switch state.look {
        case .permission: .permission
        case .question: .question
        case .plan: .plan
        case .compacting: .compacting
        // Thinking held still, after a long quiet or while a request is retried, is its loop's first frame.
        case .thinking: state.holds ? .retrying : .thinking
        case .tool(.edit): .edit
        case .tool(.command): .command
        case .tool(.read): .read
        case .tool(.longCommand): .longCommand
        // A client whose logs name no tools works at the laptop all the same.
        case .plain: .command
        case .noNews, .idle, .asleep, .done: nil
        }
    }
}

extension ClawdEvent {
    init(_ event: BotEvent) {
        switch event {
        case .start: self = .start
        case .failed: self = .failed
        case .limitHit: self = .limitHit
        }
    }
}

extension ClawdCard {
    init(_ card: BotCard) {
        switch card {
        case .done: self = .done
        case .longDone: self = .longDone
        case .reset: self = .reset
        }
    }
}

extension ClawdSign {
    init(_ badge: BotBadge) {
        switch badge {
        case .failed: self = .failed
        case .limited: self = .limited
        }
    }
}
