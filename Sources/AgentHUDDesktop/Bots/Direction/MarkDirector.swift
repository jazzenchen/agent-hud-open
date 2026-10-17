import AgentHUDCore
import Foundation

/// Decides what each mark of a dock shows from what its agents' sessions are doing: one look at a time, the newest event,
/// and a badge. It reads the queue's marks at every report (`ReportView.queueMarks`) and is advanced in between whenever a
/// rule changes something with time alone (`nextChange(after:)`). It never reads a clock of its own, so a test can take it
/// through any stretch of time it likes.
///
/// A mark (one per `VendorMark.key`) shows the first of these that applies (`want(of:owner:at:)`):
/// 1. A request waiting on the user: an approval before a question before a plan, and of two alike the one that has
///    waited longest. It shows at once.
/// 2. Claude compacting a conversation on its own. It shows at once, and the work after it too.
/// 3. Work, as one session does it, the owner: the session shown keeps the mark for as long as it works, and when it
///    stops the session whose log wrote last takes over. While it thinks, a session that uses a tool takes over, so the
///    mark shows the agent at work whenever any of its sessions is. Work shows at once. A new look of the same session shows only
///    once two reads at least `confirmation` apart want it, and not before the one shown has held for `hold`; looks that
///    keep changing hold for `guardedHold` instead.
/// 4. No news: a session in flight by evidence too old to vouch for, and none working.
/// 5. Rest: idle, or asleep once every session has been idle for `asleepAfter`, or when there is none.
///
/// Events are the mark's alone: a start when a prompt begins work after a long rest, a failure or a limit when a turn
/// ends on one.
/// Only what happened after the app started is news (`startedAt`). Badges stay as long as what they say holds.
struct MarkDirector: Sendable {
    /// What one mark shows now.
    struct Direction: Hashable, Sendable {
        var state = BotState()
        /// The newest event, which a mark plays once if it hears of it in time (`BotEvent.freshness`).
        var cue: BotCue?
    }

    /// With no call out to a tool for this long, the model is working something out: the tool's look gives way to
    /// thinking. A call that comes back quickly between two thoughts does not flash past.
    static let thinkingAfter: TimeInterval = 10
    /// A thinking turn this quiet holds its look still, with no clock, until its log moves again.
    static let quietAfter: TimeInterval = 180
    /// A new working look must be wanted at two reads at least this far apart.
    static let confirmation: TimeInterval = 2
    /// A working look, once shown, holds at least this long; what is wanted meanwhile waits, the newest of it winning.
    static let hold: TimeInterval = 6
    /// After `guardChanges` changes within `guardWindow`, a look holds this long, until `guardWindow` passes with none.
    static let guardedHold: TimeInterval = 12
    static let guardChanges = 4
    static let guardWindow: TimeInterval = 30
    /// Work after a rest this long, or after sleep, starts with a jump for joy.
    static let restBeforeStart: TimeInterval = 300
    /// A mark whose sessions have all been idle this long sleeps.
    static let asleepAfter: TimeInterval = 3600
    /// A failure's badge goes after this long, if no prompt or look at the card took it off before.
    static let badgeLife: TimeInterval = 900

    /// Only turns that ended after this, and prompts given after it, are news; what happened before is history. The
    /// app's start, as `IslandEventTracker.startedAt` has it.
    let startedAt: Date
    /// What each mark shows, by mark key, as of the last read or advance.
    private(set) var directions: [String: Direction] = [:]
    /// The marks as last read.
    private var queue: [ReportView.QueueMark] = []
    private var marks: [String: Mark] = [:]
    /// When each mark's card was last looked at, which takes a failure's badge off it.
    private var looked: [String: Date] = [:]

    init(startedAt: Date) { self.startedAt = startedAt }

    /// Takes in the marks of a new report, and decides what each shows at `now`.
    mutating func read(_ queue: [ReportView.QueueMark], at now: Date) {
        self.queue = queue
        advance(to: now)
    }

    /// Decides what each mark shows at `now`, from the marks last read.
    mutating func advance(to now: Date) {
        var next: [String: Direction] = [:], kept: [String: Mark] = [:]
        for mark in queue {
            var state = marks[mark.key] ?? Mark()
            let want = Self.want(of: mark, owner: state.owner, at: now)
            let restingSince = state.restingSince
            let rested = state.follow(want, at: now, restingFrom: mark.sessions.map(\.lastEventAt).max())
            state.hear(mark, after: startedAt, at: now)
            // Only a prompt given during the rest starts work: a turn woken without one, by a task it left running or a
            // sub-agent, goes on from before it.
            if rested, mark.requests.isEmpty,
               let owner = mark.sessions.first(where: { $0.sessionID == state.owner }),
               let prompt = owner.turnStartedAt, prompt > max(startedAt, restingSince ?? .distantPast) {
                state.tell(.start, at: now)
            }
            let look = state.look ?? .idle
            next[mark.key] = Direction(
                state: BotState(look: look, subagents: state.subagents && look.isWorking,
                                badge: Self.badge(of: mark, lookedAt: looked[mark.key], at: now),
                                holds: state.holds && look == .thinking),
                cue: state.cue)
            kept[mark.key] = state
        }
        marks = kept
        directions = next
    }

    /// Someone looked at these marks' cards at `now`, or at every mark's for nil, the whole panel: a failure's badge
    /// has been seen and comes off.
    mutating func looked(at keys: Set<String>?, at now: Date) {
        for key in keys ?? Set(queue.map(\.key)) { looked[key] = now }
        advance(to: now)
    }

    /// The first time after `now` at which a mark may show something else with nothing new read: a wanted look's
    /// confirmation or hold running out, a tool's look giving way to thinking, thinking going still, a long command, a
    /// compaction or a limit running out, a badge lapsing, a mark falling asleep. Nil when nothing will.
    func nextChange(after now: Date) -> Date? {
        var soonest: Date?
        func take(_ time: Date?) { if let time, time > now { soonest = min(soonest ?? time, time) } }
        for mark in queue {
            if let state = marks[mark.key], let wanted = state.wanted {
                take(max(wanted.since + Self.confirmation, state.since + state.holdLength))
            }
            for session in mark.sessions {
                take(session.nextChange(after: now))
                take(session.lastEventAt + Self.asleepAfter)
                if session.isInFlight {
                    if !session.hasOpenCall, let seen = session.toolSeenAt { take(seen + Self.thinkingAfter) }
                    take(session.observedAt + Self.quietAfter)
                }
                if session.failed, let end = session.endedAt { take(end + Self.badgeLife) }
            }
            take(mark.exhaustedUntil)
        }
        return soonest
    }
}

// MARK: - One mark over time

extension MarkDirector {
    /// What a mark's sessions want it to show now, before the rules for switching between looks.
    struct Want: Equatable, Sendable {
        var look: BotLook
        /// The session whose work it is; nil for anything else.
        var owner: String? = nil
        var subagents = false
        /// The owner's thinking holds still (`holds(_:at:)`).
        var holds = false
    }

    /// A working look wanted in place of the one shown, and since when.
    fileprivate struct Wanted: Sendable {
        var look: BotLook
        var since: Date
    }

    fileprivate struct Mark: Sendable {
        /// The look shown; nil before the first read.
        var look: BotLook?
        /// The session the work shown is of. A request showing over it leaves it in place.
        var owner: String?
        var subagents = false, holds = false
        /// When the working look shown began.
        var since = Date.distantPast
        var wanted: Wanted?
        /// When the working looks of the last `guardWindow` began, and whether so many did that looks hold longer.
        var changes: [Date] = []
        var guarded = false
        /// Since when the mark has been at rest: idle, asleep or without news.
        var restingSince: Date?
        /// The newest turn end each session has been heard for, so each fires once.
        var ended: [String: Date] = [:]
        var cue: BotCue?

        var holdLength: TimeInterval { guarded ? MarkDirector.guardedHold : MarkDirector.hold }

        /// Shows what the sessions want as the switching rules allow. Returns whether work began just now after a long
        /// rest or after sleep.
        /// - restingFrom: the newest event of the mark's sessions, how long a mark first read at rest has rested.
        mutating func follow(_ want: Want, at now: Date, restingFrom: Date?) -> Bool {
            guard let shown = look else {
                show(want, at: now)
                if want.look.isCalm { restingSince = restingFrom ?? .distantPast }
                return false
            }
            if guarded, let last = changes.last, now.timeIntervalSince(last) >= MarkDirector.guardWindow {
                guarded = false
                changes = []
            }
            if want.look == shown {
                wanted = nil
                show(want, at: now)
                return false
            }
            guard want.look.isWorking, shown.isWorking, want.look != .compacting, shown != .compacting, want.owner == owner else {
                // A request cuts in at once, and so does a compaction, and the work it ends; rest comes as soon as work
                // ends; work shows as soon as it begins, or as soon as another session takes over from one that stopped.
                let rested = shown == .asleep
                    || shown == .idle && now.timeIntervalSince(restingSince ?? now) >= MarkDirector.restBeforeStart
                show(want, at: now)
                return want.look.isWorking && rested
            }
            // Another look of the same work: the newest wanted, once two reads have wanted it and the shown one has held.
            if wanted?.look != want.look { wanted = Wanted(look: want.look, since: now) }
            let since = wanted?.since ?? now
            if now.timeIntervalSince(since) >= MarkDirector.confirmation, now.timeIntervalSince(self.since) >= holdLength {
                show(want, at: now)
            } else {
                subagents = want.subagents
                holds = want.holds
            }
            return false
        }

        private mutating func show(_ want: Want, at now: Date) {
            let before = look
            if want.look.isWorking {
                owner = want.owner
                if want.look != before {
                    since = now
                    changes = changes.filter { now.timeIntervalSince($0) < MarkDirector.guardWindow } + [now]
                    if changes.count >= MarkDirector.guardChanges { guarded = true }
                }
            } else if want.look.isCalm {
                owner = nil
                if before?.isCalm != true { restingSince = now }
            }
            look = want.look
            wanted = nil
            subagents = want.subagents
            holds = want.holds
        }

        /// Hears the turns that ended since the last read: a failure or a limit is an event, if it came after `startedAt`.
        mutating func hear(_ mark: ReportView.QueueMark, after startedAt: Date, at now: Date) {
            var events: [BotEvent] = []
            for session in mark.sessions {
                guard let end = session.endedAt, let outcome = session.outcome,
                      end > max(startedAt, ended[session.sessionID] ?? .distantPast) else { continue }
                ended[session.sessionID] = end
                switch outcome {
                case .failed: events.append(.failed)
                case .limited: events.append(.limitHit)
                case .completed, .interrupted: break
                }
            }
            let present = Set(mark.sessions.map(\.sessionID))
            ended = ended.filter { present.contains($0.key) }
            for event in events { tell(event, at: now) }
        }

        /// A queue of one: the newest event wins, unless the one before is still fresh and outranks it.
        mutating func tell(_ event: BotEvent, at now: Date) {
            if let cue, now.timeIntervalSince(cue.at) < BotEvent.freshness, cue.event.rank > event.rank { return }
            cue = BotCue(event: event, at: now)
        }
    }
}
