import AgentHUDCore
import XCTest
@testable import AgentHUDDesktop

/// The director on a clock of its own: every time below is seconds after the app started.
final class MarkDirectorTests: XCTestCase {
    private static let start = Date(timeIntervalSinceReferenceDate: 800_000_000)
    private func at(_ seconds: TimeInterval) -> Date { Self.start.addingTimeInterval(seconds) }

    /// A session's activity: a turn begun, with `calls` opened (and closed when their second time is given).
    private func activity(_ calls: [(id: String, work: SessionActivity.Work?, opened: TimeInterval, closed: TimeInterval?)] = [],
                          line: SessionActivity.Line? = nil, retrying: Bool = false, compacting: TimeInterval? = nil,
                          subagents: TimeInterval? = nil) -> SessionActivity {
        var activity = SessionActivity()
        activity.startTurn()
        for call in calls {
            activity.open(call.id, work: call.work, at: at(call.opened))
            if let closed = call.closed { activity.close(call.id, at: at(closed)) }
        }
        activity.settle()
        activity.line = line
        activity.retrying = retrying
        activity.compactingSince = compacting.map(at)
        activity.subagentsAt = subagents.map(at)
        return activity
    }

    private func session(_ id: String, _ state: SessionPhase.State = .running, activity: SessionActivity? = SessionActivity(),
                         turn: TimeInterval? = 0, outcome: TurnOutcome? = nil, ended: TimeInterval? = nil,
                         limitedUntil: TimeInterval? = nil, observed: TimeInterval = 0, lastEvent: TimeInterval? = nil,
                         requests: [AttentionRequest] = []) -> SessionLiveFacts {
        SessionLiveFacts(sessionID: id, vendor: "Claude", phase: SessionPhase(state: state, since: at(turn ?? 0), validUntil: nil),
                         activity: activity, turnID: turn.map { "turn-\($0)" }, turnStartedAt: turn.map(at),
                         outcome: outcome, endedAt: ended.map(at), limitedUntil: limitedUntil.map(at),
                         observedAt: at(observed), lastEventAt: at(lastEvent ?? max(observed, ended ?? observed)),
                         requests: requests)
    }

    private func mark(_ sessions: [SessionLiveFacts], key: String = "Claude", exhaustedUntil: TimeInterval? = nil)
        -> ReportView.QueueMark {
        ReportView.QueueMark(key: key, vendor: key, isWorking: sessions.contains(where: \.isInFlight), sessions: sessions,
                             exhaustedUntil: exhaustedUntil.map(at))
    }

    private func request(_ kind: AttentionRequest.Kind, _ id: String, session: String, at seconds: TimeInterval) -> AttentionRequest {
        AttentionRequest(id: id, sessionID: session, kind: kind, at: at(seconds))
    }

    /// A shell command open since `opened`, in a running session.
    private func commanding(_ id: String = "a", opened: TimeInterval = 0, observed: TimeInterval = 0,
                            turn: TimeInterval = 0) -> SessionLiveFacts {
        session(id, activity: activity([("call", .shell, opened, nil)], line: .call), turn: turn, observed: observed)
    }

    /// An edit open since `opened`, in a running session.
    private func editing(_ id: String = "a", opened: TimeInterval, observed: TimeInterval? = nil) -> SessionLiveFacts {
        session(id, activity: activity([("edit-\(opened)", .edit, opened, nil)], line: .call), observed: observed ?? opened)
    }

    private func reading(_ id: String = "a", opened: TimeInterval) -> SessionLiveFacts {
        session(id, activity: activity([("read-\(opened)", .read, opened, nil)], line: .call), observed: opened)
    }

    private func director(reading queue: [ReportView.QueueMark], at seconds: TimeInterval) -> MarkDirector {
        var director = MarkDirector(startedAt: Self.start)
        director.read(queue, at: at(seconds))
        return director
    }

    private func read(_ director: inout MarkDirector, _ queue: [ReportView.QueueMark], at seconds: TimeInterval) {
        director.read(queue, at: at(seconds))
    }

    private func state(_ director: MarkDirector, _ key: String = "Claude") -> BotState? { director.directions[key]?.state }
    private func look(_ director: MarkDirector, _ key: String = "Claude") -> BotLook? { state(director, key)?.look }

    // MARK: Merging the sessions of one mark

    func testARequestComesFirstAndAnApprovalBeforeAQuestionBeforeAPlan() {
        let working = commanding("a")
        let plan = session("b", .waitingForApproval, requests: [request(.plan, "p", session: "b", at: 1)])
        let question = session("c", .waitingForApproval, requests: [request(.question, "q", session: "c", at: 2)])
        let approval = session("d", .waitingForApproval, requests: [request(.permission, "x", session: "d", at: 3)])
        XCTAssertEqual(look(director(reading: [mark([working, plan])], at: 5)), .plan)
        XCTAssertEqual(look(director(reading: [mark([working, plan, question])], at: 5)), .question)
        XCTAssertEqual(look(director(reading: [mark([working, plan, question, approval])], at: 5)), .permission,
                       "The kind decides before the wait does")
    }

    func testOfTwoRequestsAlikeTheOneThatWaitedLongestShows() {
        let older = request(.question, "z", session: "b", at: 1), newer = request(.question, "a", session: "c", at: 2)
        XCTAssertEqual(MarkDirector.want(of: mark([session("c", .waitingForApproval, requests: [newer]),
                                                   session("b", .waitingForApproval, requests: [older])]),
                                         owner: nil, at: at(5)).owner, "b")
        // Asked at the same moment, the first by id.
        let tie = [request(.permission, "y", session: "c", at: 1), request(.permission, "x", session: "b", at: 1)]
        XCTAssertEqual(tie.min(by: MarkDirector.comesFirst)?.id, "x")
    }

    func testCompactingShowsOverWorkAndARequestOverCompacting() {
        let compacting = session("a", activity: activity(line: .result, compacting: 1), observed: 0)
        XCTAssertEqual(look(director(reading: [mark([commanding("b", observed: 2), compacting])], at: 3)), .compacting)
        let asked = session("c", .waitingForApproval, requests: [request(.permission, "x", session: "c", at: 2)])
        XCTAssertEqual(look(director(reading: [mark([compacting, asked])], at: 3)), .permission)
        // Compaction runs out after five minutes with no newer line.
        XCTAssertEqual(look(director(reading: [mark([compacting])], at: 1 + CompactionHooks.limit)), .thinking)
        // It comes and goes at once, with no confirmation and no hold, however recently the work's look changed.
        var director = director(reading: [mark([editing(opened: 0)])], at: 0)
        let editingAndCompacting = session("a", activity: activity([("edit-0", .edit, 0, nil)], line: .call, compacting: 1), observed: 0)
        read(&director, [mark([editingAndCompacting])], at: 1)
        XCTAssertEqual(look(director), .compacting)
        read(&director, [mark([editing(opened: 0, observed: 2)])], at: 2)
        XCTAssertEqual(look(director), .tool(.edit))
    }

    func testTheSessionShownKeepsTheMarkWhileItWorks() {
        var director = director(reading: [mark([commanding("a", observed: 5)])], at: 5)
        XCTAssertEqual(look(director), .tool(.command))
        // Another session writes later and edits: the mark stays with the first, and never shows the other's look.
        read(&director, [mark([commanding("a", observed: 5), editing("b", opened: 8)])], at: 10)
        director.advance(to: at(30))
        XCTAssertEqual(look(director), .tool(.command))
        // The first stops: the other takes over at once, with its own look.
        read(&director, [mark([session("a", .idle, outcome: .completed, ended: 31), editing("b", opened: 8)])], at: 32)
        XCTAssertEqual(look(director), .tool(.edit))
    }

    func testWhileTheSessionShownThinksOneThatUsesAToolTakesOver() {
        // "a" read a file long ago and thinks, with a sub-agent at work; "c" thinks too, which takes nothing over.
        let thinking = session("a", activity: activity([("read", .read, 0, 1)], line: .thinking, subagents: 19), observed: 20)
        let alsoThinking = session("c", activity: activity(line: .thinking), observed: 15)
        var director = director(reading: [mark([thinking, alsoThinking])], at: 20)
        XCTAssertEqual(look(director), .thinking)
        XCTAssertEqual(state(director)?.subagents, true)
        // "b" runs a command: it takes the mark at once, with its own look and without "a"'s sub-agents.
        read(&director, [mark([thinking, alsoThinking, commanding("b", opened: 21, observed: 21)])], at: 22)
        XCTAssertEqual(look(director), .tool(.command), "The agent shows at work while any of its sessions is")
        XCTAssertEqual(state(director)?.subagents, false)
        // Once "b" thinks too it keeps the mark, until "a" uses a tool again.
        let doneCommanding = session("b", activity: activity([("call", .shell, 21, 23)], line: .thinking), observed: 23)
        XCTAssertEqual(MarkDirector.want(of: mark([thinking, doneCommanding]), owner: "b", at: at(40)).owner, "b")
        let editing = editing("a", opened: 41)
        let want = MarkDirector.want(of: mark([editing, doneCommanding]), owner: "b", at: at(42))
        XCTAssertEqual(want.owner, "a")
        XCTAssertEqual(want.look, .tool(.edit))
    }

    func testANewOwnerIsTheSessionWhoseLogWroteLastThenWhoseTurnBeganLastThenByID() {
        let older = commanding("a", observed: 4), newer = editing("b", opened: 1, observed: 6)
        XCTAssertEqual(MarkDirector.owner(among: [older, newer], keeping: nil)?.sessionID, "b")
        XCTAssertEqual(MarkDirector.owner(among: [older, newer], keeping: "a")?.sessionID, "a", "The one shown is kept")
        let early = commanding("a", observed: 6, turn: 1), late = commanding("b", observed: 6, turn: 2)
        XCTAssertEqual(MarkDirector.owner(among: [early, late], keeping: nil)?.sessionID, "b")
        XCTAssertEqual(MarkDirector.owner(among: [commanding("b", observed: 6), commanding("a", observed: 6)], keeping: nil)?
            .sessionID, "a")
    }

    func testNoNewsWhenASessionIsUnverifiedAndNoneWorks() {
        let unverified = session("a", .unverified, observed: 0)
        XCTAssertEqual(look(director(reading: [mark([unverified, session("b", .idle, observed: 10)])], at: 20)), .noNews)
        XCTAssertEqual(look(director(reading: [mark([unverified, commanding("b", opened: 10, observed: 10)])], at: 20)),
                       .tool(.command))
    }

    func testAMarkSleepsWithNoSessionOrWhenEverySessionHasBeenIdleAnHour() {
        XCTAssertEqual(look(director(reading: [mark([])], at: 0)), .asleep)
        let idle = session("a", .idle, outcome: .completed, ended: 0)
        XCTAssertEqual(look(director(reading: [mark([idle])], at: 3599)), .idle)
        XCTAssertEqual(look(director(reading: [mark([idle])], at: 3600)), .asleep)
        var director = director(reading: [mark([idle])], at: 10)
        XCTAssertEqual(director.nextChange(after: at(10)), at(3600), "It wakes itself to fall asleep")
        director.advance(to: at(3600))
        XCTAssertEqual(look(director), .asleep)
    }

    func testAVendorAtWorkWithNoSessionToSayHowShowsPlainWork() {
        let queue = ReportView.QueueMark(key: "Cursor", vendor: "Cursor", isWorking: true, sessions: [], exhaustedUntil: nil)
        XCTAssertEqual(look(director(reading: [queue], at: 0), "Cursor"), .plain)
        XCTAssertEqual(look(director(reading: [mark([session("a", activity: nil)], key: "Cursor")], at: 0), "Cursor"), .plain)
    }

    // MARK: A session's working look

    func testAToolsLookGivesWayToThinkingTenSecondsAfterItsCallCameBack() {
        let done = session("a", activity: activity([("call", .shell, 0, 2)], line: .result), observed: 2)
        XCTAssertEqual(MarkDirector.look(of: done, at: at(11.9)), .tool(.command))
        XCTAssertEqual(MarkDirector.look(of: done, at: at(12)), .thinking)
        // A call of no kind still out keeps the kind.
        let listing = session("a", activity: activity([("call", .shell, 0, 2), ("todo", nil, 3, nil)], line: .call),
                              observed: 3)
        XCTAssertEqual(MarkDirector.look(of: listing, at: at(60)), .tool(.command))
        // Before the turn's first call it thinks; a shell command open twenty seconds is a long one.
        XCTAssertEqual(MarkDirector.look(of: session("a", activity: activity(line: .prompt)), at: at(1)), .thinking)
        XCTAssertEqual(MarkDirector.look(of: commanding(opened: 0), at: at(20)), .tool(.longCommand))
        XCTAssertEqual(MarkDirector.look(of: reading(opened: 0), at: at(1)), .tool(.read))
    }

    func testThinkingHoldsStillAfterThreeQuietMinutesOrWhileARequestIsRetried() {
        let quiet = session("a", activity: activity(line: .thinking), observed: 0)
        var director = director(reading: [mark([quiet])], at: 1)
        XCTAssertEqual(state(director), BotState(look: .thinking))
        XCTAssertEqual(director.nextChange(after: at(1)), at(MarkDirector.quietAfter))
        director.advance(to: at(MarkDirector.quietAfter))
        XCTAssertEqual(state(director), BotState(look: .thinking, holds: true))
        // A newer line moves it again.
        read(&director, [mark([session("a", activity: activity(line: .text), observed: 200)])], at: 200)
        XCTAssertEqual(state(director), BotState(look: .thinking))

        let retrying = session("a", activity: activity([("call", .shell, 0, 1)], line: .text, retrying: true), observed: 2)
        XCTAssertEqual(state(self.director(reading: [mark([retrying])], at: 3)), BotState(look: .thinking, holds: true))
        // Waiting on a sub-agent is not quiet.
        let delegating = session("a", activity: activity(line: .result, subagents: 0), observed: 0)
        XCTAssertEqual(state(self.director(reading: [mark([delegating])], at: 400)), BotState(look: .thinking, subagents: true))
    }

    // MARK: Switching between working looks

    func testWorkShowsAtOnceAndRestAsSoonAsTheTurnEnds() {
        var director = director(reading: [mark([session("a", .idle, outcome: .completed, ended: 0)])], at: 1)
        XCTAssertEqual(look(director), .idle)
        read(&director, [mark([commanding("a", opened: 2, observed: 2, turn: 2)])], at: 2)
        XCTAssertEqual(look(director), .tool(.command), "Work needs no confirmation")
        read(&director, [mark([session("a", .idle, outcome: .completed, ended: 3)])], at: 3)
        XCTAssertEqual(look(director), .idle, "Leaving work skips the hold")
    }

    func testANewLookOfTheSameWorkWaitsForTwoReadsAndTheHold() {
        var director = director(reading: [mark([commanding(observed: 0)])], at: 0)
        // An edit is wanted a second in; the command shown holds six seconds from its start.
        read(&director, [mark([editing(opened: 1)])], at: 1)
        XCTAssertEqual(look(director), .tool(.command))
        XCTAssertEqual(director.nextChange(after: at(1)), at(MarkDirector.hold))
        director.advance(to: at(5.9))
        XCTAssertEqual(look(director), .tool(.command))
        director.advance(to: at(6))
        XCTAssertEqual(look(director), .tool(.edit))

        // Past the hold, a new look still needs a second read two seconds after the first.
        read(&director, [mark([reading(opened: 20)])], at: 20)
        XCTAssertEqual(look(director), .tool(.edit))
        XCTAssertEqual(director.nextChange(after: at(20)), at(22))
        director.advance(to: at(22))
        XCTAssertEqual(look(director), .tool(.read))

        // A look wanted for one read only never shows.
        read(&director, [mark([editing(opened: 40)])], at: 40)
        read(&director, [mark([reading(opened: 20)])], at: 41)
        director.advance(to: at(45))
        XCTAssertEqual(look(director), .tool(.read))
    }

    func testLooksWantedDuringTheHoldCollapseToTheNewest() {
        var director = director(reading: [mark([commanding(observed: 0)])], at: 0)
        read(&director, [mark([editing(opened: 1)])], at: 1)
        read(&director, [mark([reading(opened: 3)])], at: 3)
        director.advance(to: at(MarkDirector.hold))
        XCTAssertEqual(look(director), .tool(.read), "The edit wanted first never shows")
    }

    func testLooksThatKeepChangingHoldLongerUntilThingsCalmDown() {
        let looks: [(TimeInterval) -> SessionLiveFacts] = [
            { self.editing(opened: $0) }, { self.reading(opened: $0) }, { self.commanding(opened: $0, observed: $0) },
        ]
        var director = director(reading: [mark([commanding(observed: 0)])], at: 0)
        // Three more changes, each as soon as the six-second hold allows: four within thirty seconds.
        var now: TimeInterval = 0
        for index in 0..<3 {
            read(&director, [mark([looks[index](now + 4)])], at: now + 4)
            director.advance(to: at(now + 6))
            now += 6
            XCTAssertEqual(director.directions["Claude"]?.state.look, [BotLook.tool(.edit), .tool(.read), .tool(.command)][index])
        }
        // The fifth change waits twelve seconds.
        read(&director, [mark([editing(opened: now + 1)])], at: now + 1)
        director.advance(to: at(now + 6))
        XCTAssertEqual(look(director), .tool(.command))
        XCTAssertEqual(director.nextChange(after: at(now + 6)), at(now + MarkDirector.guardedHold))
        director.advance(to: at(now + MarkDirector.guardedHold))
        XCTAssertEqual(look(director), .tool(.edit))
        // Thirty seconds with no change: back to six.
        now += MarkDirector.guardedHold + MarkDirector.guardWindow
        read(&director, [mark([reading(opened: now)])], at: now)
        director.advance(to: at(now + 2))
        XCTAssertEqual(look(director), .tool(.read))
    }

    func testARequestCutsInAtOnceAndWorkComesBackAfterIt() {
        var director = director(reading: [mark([commanding(observed: 0)])], at: 0)
        let asked = session("a", .waitingForApproval, activity: activity([("call", .shell, 0, nil)], line: .call),
                            requests: [request(.permission, "x", session: "a", at: 1)])
        read(&director, [mark([asked])], at: 1)
        XCTAssertEqual(look(director), .permission)
        read(&director, [mark([editing(opened: 2)])], at: 2)
        XCTAssertEqual(look(director), .tool(.edit), "Back from a request, work shows at once")
    }

    func testSubAgentsGoWithTheWorkShown() {
        let delegating = session("a", activity: activity([("task", .agents, 0, nil), ("call", .shell, 1, nil)], line: .call),
                                 observed: 1)
        XCTAssertEqual(state(director(reading: [mark([delegating])], at: 2)),
                       BotState(look: .tool(.command), subagents: true))
    }

    // MARK: Events

    func testWorkAfterSleepOrALongRestStartsWithAJump() {
        // Asleep: no session at all.
        var director = director(reading: [mark([])], at: 10)
        read(&director, [mark([commanding(opened: 20, observed: 20, turn: 20)])], at: 20)
        XCTAssertEqual(director.directions["Claude"]?.cue, BotCue(event: .start, at: at(20)))

        // Idle for five minutes.
        let rested = session("a", .idle, outcome: .completed, ended: 10)
        director = self.director(reading: [mark([rested])], at: 11)
        read(&director, [mark([commanding(opened: 310, observed: 310, turn: 310)])], at: 310)
        XCTAssertEqual(director.directions["Claude"]?.cue?.event, .start)

        // Idle for less: no jump.
        director = self.director(reading: [mark([rested])], at: 11)
        read(&director, [mark([commanding(opened: 300, observed: 300, turn: 300)])], at: 300)
        XCTAssertNil(director.directions["Claude"]?.cue)
    }

    func testAStartNeedsAPromptGivenSinceTheAppStartedAndNothingWaiting() {
        var director = director(reading: [mark([])], at: 10)
        read(&director, [mark([commanding(opened: 20, observed: 20, turn: -5)])], at: 20)
        XCTAssertNil(director.directions["Claude"]?.cue, "A turn begun before the app started is history")
        director = self.director(reading: [mark([])], at: 10)
        let other = session("b", .waitingForApproval, requests: [request(.question, "q", session: "b", at: 15)])
        read(&director, [mark([commanding(opened: 20, observed: 20, turn: 20), other])], at: 20)
        XCTAssertNil(director.directions["Claude"]?.cue)
        // Work found at the first read has no rest to come from.
        director = self.director(reading: [mark([commanding(opened: 20, observed: 20, turn: 20)])], at: 20)
        XCTAssertNil(director.directions["Claude"]?.cue)
        // A turn woken with no prompt, by a background task or a sub-agent, starts nothing: its prompt came before the rest.
        director = self.director(reading: [mark([commanding(opened: 10, observed: 10, turn: 10)])], at: 20)
        read(&director, [mark([session("a", .idle, turn: 10, outcome: .completed, ended: 60)])], at: 61)
        read(&director, [mark([session("a", activity: activity(subagents: 700), turn: 10, observed: 700)])], at: 700)
        XCTAssertTrue(look(director)?.isWorking == true)
        XCTAssertNil(director.directions["Claude"]?.cue)
        director = self.director(reading: [mark([session("a", .idle, turn: 10, outcome: .completed, ended: 20)])], at: 30)
        read(&director, [mark([commanding(opened: 700, observed: 700, turn: 10)])], at: 700)
        XCTAssertNil(director.directions["Claude"]?.cue)
    }

    func testATurnEndingOnAnErrorOrALimitIsAnEventOnce() {
        var director = director(reading: [mark([commanding(observed: 0)])], at: 0)
        read(&director, [mark([session("a", .idle, outcome: .failed, ended: 5)])], at: 5)
        XCTAssertEqual(director.directions["Claude"]?.cue, BotCue(event: .failed, at: at(5)))
        read(&director, [mark([session("a", .idle, outcome: .failed, ended: 5)])], at: 9)
        XCTAssertEqual(director.directions["Claude"]?.cue, BotCue(event: .failed, at: at(5)), "Heard once")
        read(&director, [mark([session("a", .idle, outcome: .limited(resetsAt: nil), ended: 20)])], at: 20)
        XCTAssertEqual(director.directions["Claude"]?.cue, BotCue(event: .limitHit, at: at(20)))
        // A completion or a stop is no event.
        read(&director, [mark([session("a", .idle, outcome: .interrupted, ended: 30)])], at: 30)
        read(&director, [mark([session("a", .idle, outcome: .completed, ended: 40)])], at: 40)
        XCTAssertEqual(director.directions["Claude"]?.cue?.event, .limitHit)
        // A failure from before the app started is history.
        director = self.director(reading: [mark([session("a", .idle, outcome: .failed, ended: -1)])], at: 1)
        XCTAssertNil(director.directions["Claude"]?.cue)
    }

    func testTheNewestEventWinsUnlessAFreshOneOutranksIt() {
        var director = director(reading: [mark([])], at: 0)
        read(&director, [mark([session("b", .idle, outcome: .failed, ended: 9),
                             commanding("a", opened: 10, observed: 10, turn: 10)])], at: 10)
        XCTAssertEqual(director.directions["Claude"]?.cue?.event, .failed, "A failure outranks a start told with it")
        read(&director, [mark([session("b", .idle, outcome: .limited(resetsAt: nil), ended: 13),
                             commanding("a", opened: 10, observed: 10, turn: 10)])], at: 14)
        XCTAssertEqual(director.directions["Claude"]?.cue, BotCue(event: .limitHit, at: at(14)),
                       "Once the failure is stale, the newest wins")
    }

    // MARK: Badges

    func testAFailuresBadgeLastsUntilAPromptALookAtTheCardOrFifteenMinutes() {
        let failed = session("a", .idle, outcome: .failed, ended: 0)
        var director = director(reading: [mark([failed])], at: 1)
        XCTAssertEqual(state(director)?.badge, .failed)
        XCTAssertEqual(director.nextChange(after: at(1)), at(MarkDirector.badgeLife))
        director.advance(to: at(MarkDirector.badgeLife))
        XCTAssertNil(state(director)?.badge)

        director = self.director(reading: [mark([failed])], at: 1)
        director.looked(at: ["Claude"], at: at(2))
        XCTAssertNil(state(director)?.badge, "Its card was looked at")
        director = self.director(reading: [mark([failed])], at: 1)
        director.looked(at: nil, at: at(2))
        XCTAssertNil(state(director)?.badge, "The whole panel was looked at")
        director = self.director(reading: [mark([failed])], at: 1)
        director.looked(at: ["Codex"], at: at(2))
        XCTAssertEqual(state(director)?.badge, .failed, "Another agent's card was")

        // A prompt in any of the agent's sessions takes it off.
        director = self.director(reading: [mark([failed, commanding("b", opened: 5, observed: 5, turn: 5)])], at: 5)
        XCTAssertEqual(state(director), BotState(look: .tool(.command)))
    }

    func testALimitsBadgeLastsWhileItHoldsAndNeverOutranksWork() {
        let limited = session("a", .idle, outcome: .limited(resetsAt: nil), ended: 0, limitedUntil: 100)
        var director = director(reading: [mark([limited])], at: 1)
        XCTAssertEqual(state(director), BotState(look: .idle, badge: .limited))
        director.advance(to: at(100))
        XCTAssertNil(state(director)?.badge)
        // An account window used up: a badge on the work, never in its place.
        director = self.director(reading: [mark([commanding(observed: 0)], exhaustedUntil: 50)], at: 1)
        XCTAssertEqual(state(director), BotState(look: .tool(.command), badge: .limited))
        XCTAssertEqual(director.nextChange(after: at(1)), at(20), "The command turns long first")
        director.advance(to: at(50))
        XCTAssertEqual(state(director)?.badge, nil)
        // A failure's badge comes before a limit's.
        let both = [session("a", .idle, outcome: .failed, ended: 0), limited]
        XCTAssertEqual(state(self.director(reading: [mark(both)], at: 1))?.badge, .failed)
        // A limit with no reset known lasts until a prompt, in any of the agent's sessions.
        let unknown = session("a", .idle, outcome: .limited(resetsAt: nil), ended: 0)
        XCTAssertEqual(state(self.director(reading: [mark([unknown])], at: 3600))?.badge, .limited)
        director = self.director(reading: [mark([unknown, commanding("b", opened: 5, observed: 5, turn: 5)])], at: 5)
        XCTAssertEqual(state(director), BotState(look: .tool(.command)))
    }

    func testTheDockNeverShowsDoneAndMarksGoneFromTheQueueAreForgotten() {
        var director = director(reading: [mark([session("a", .idle, outcome: .completed, ended: 0)]),
                                           mark([], key: "Codex")], at: 1)
        XCTAssertEqual(Set(director.directions.keys), ["Claude", "Codex"])
        XCTAssertFalse(director.directions.values.contains { $0.state.look == .done })
        read(&director, [mark([], key: "Codex")], at: 2)
        XCTAssertEqual(Set(director.directions.keys), ["Codex"])
    }
}

/// Which events a dock hands its marks, among them ones that fired before a mark could play them.
final class DockMemoryTests: XCTestCase {
    private let now = Date(timeIntervalSinceReferenceDate: 800_000_000)

    func testAFreshEventIsHandedAsItIsAndAgainTheSame() {
        var memory = DockMemory()
        let cue = BotCue(event: .start, at: now)
        XCTAssertEqual(memory.cue(for: "Claude", cue, badge: nil, at: now.addingTimeInterval(1)), cue)
        XCTAssertEqual(memory.cue(for: "Claude", cue, badge: nil, at: now.addingTimeInterval(9)), cue)
        XCTAssertNil(memory.cue(for: "Claude", nil, badge: nil, at: now))
    }

    func testAMissedFailurePlaysOnceWhileItsBadgeStands() {
        var memory = DockMemory()
        let failed = BotCue(event: .failed, at: now)
        let later = now.addingTimeInterval(40)
        let replay = memory.cue(for: "Claude", failed, badge: .failed, at: later)
        XCTAssertEqual(replay, BotCue(event: .failed, at: later), "Dated afresh, so the mark plays it")
        XCTAssertEqual(memory.cue(for: "Claude", failed, badge: .failed, at: later.addingTimeInterval(5)), replay,
                       "Asked again, the same replay, which the mark has played already")
        let limit = BotCue(event: .limitHit, at: now)
        XCTAssertEqual(memory.cue(for: "Codex", limit, badge: .limited, at: later), BotCue(event: .limitHit, at: later))
    }

    func testAMissedStartIsOverAndSoIsAFailureWhoseBadgeWentOff() {
        var memory = DockMemory()
        let start = BotCue(event: .start, at: now), failed = BotCue(event: .failed, at: now)
        let later = now.addingTimeInterval(40)
        XCTAssertEqual(memory.cue(for: "Claude", start, badge: nil, at: later), start, "Stale, so the mark lets it go")
        XCTAssertEqual(memory.cue(for: "Codex", failed, badge: nil, at: later), failed)
        XCTAssertEqual(memory.cue(for: "Grok", failed, badge: .limited, at: later), failed)
    }
}
