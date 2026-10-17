import AgentHUDCore
import Foundation

/// What a mark's sessions want it to show, read from their facts alone: no holds, no history, nothing the director keeps.
extension MarkDirector {
    /// What `mark` wants to show at `now`, its sessions merged in the order the director's rules give
    /// (`MarkDirector`). `owner` is the session whose work the mark shows, which keeps it while it works.
    static func want(of mark: ReportView.QueueMark, owner: String?, at now: Date) -> Want {
        if let request = mark.requests.min(by: comesFirst) {
            return Want(look: look(for: request.kind), owner: request.sessionID)
        }
        if let session = Self.owner(among: mark.sessions.filter { $0.compacting(at: now) }, keeping: owner) {
            return Want(look: .compacting, owner: session.sessionID, subagents: session.subagents)
        }
        let inFlight = mark.sessions.filter(\.isInFlight)
        if var session = Self.owner(among: inFlight, keeping: owner) {
            var shown = look(of: session, at: now)
            // While the session shown thinks, one that uses a tool takes the mark over: the agent shows at work whenever
            // any of its sessions is.
            if shown == .thinking, let tooling = Self.owner(among: inFlight.filter { usesTool($0, at: now) }, keeping: nil) {
                session = tooling
                shown = look(of: tooling, at: now)
            }
            return Want(look: shown, owner: session.sessionID, subagents: session.subagents, holds: holds(session, at: now))
        }
        // A vendor at work with no session to say how shows plain work.
        if mark.isWorking { return Want(look: .plain) }
        if mark.sessions.contains(where: \.isUnverified) { return Want(look: .noNews) }
        if mark.sessions.allSatisfy({ now.timeIntervalSince($0.lastEventAt) >= asleepAfter }) { return Want(look: .asleep) }
        return Want(look: .idle)
    }

    /// Of two requests, the one a mark shows: an approval before a question before a plan, and of two alike the one
    /// that has waited longest, as the island's queue takes them.
    static func comesFirst(_ lhs: AttentionRequest, _ rhs: AttentionRequest) -> Bool {
        lhs.kind.rank != rhs.kind.rank ? lhs.kind.rank > rhs.kind.rank : AttentionRequest.waitedLonger(lhs, rhs)
    }

    static func look(for kind: AttentionRequest.Kind) -> BotLook {
        switch kind {
        case .permission: .permission
        case .question: .question
        case .plan: .plan
        }
    }

    /// The session whose work the mark shows, of those `working`: the one it shows already while it is among them, else
    /// the one whose log wrote last, then the one whose turn began last, then the first by id. A thinking one gives way to
    /// one that uses a tool (`want(of:owner:at:)`).
    static func owner(among working: [SessionLiveFacts], keeping current: String?) -> SessionLiveFacts? {
        if let current, let kept = working.first(where: { $0.sessionID == current }) { return kept }
        return working.min { lhs, rhs in
            if lhs.observedAt != rhs.observedAt { return lhs.observedAt > rhs.observedAt }
            let left = lhs.turnStartedAt ?? .distantPast, right = rhs.turnStartedAt ?? .distantPast
            if left != right { return left > right }
            return lhs.sessionID < rhs.sessionID
        }
    }

    /// What a working session shows itself doing at `now`: its open tool, a tool it used within `thinkingAfter`, or
    /// thinking. A request being retried is thinking, held. A client whose log records no tools shows plain work.
    static func look(of session: SessionLiveFacts, at now: Date) -> BotLook {
        if session.retrying { return .thinking }
        let tool: BotTool
        switch session.look(at: now) {
        case .plain: return .plain
        case .thinking: return .thinking
        case .longCommand: return .tool(.longCommand)
        case .command: tool = .command
        case .edit: tool = .edit
        case .read: tool = .read
        }
        // A tool's look outlasts its call by a moment; a call still out, even one of no kind, keeps it.
        if session.hasOpenCall { return .tool(tool) }
        if let seen = session.toolSeenAt, now.timeIntervalSince(seen) < thinkingAfter { return .tool(tool) }
        return .thinking
    }

    /// Whether a working session shows a tool's look at `now`.
    static func usesTool(_ session: SessionLiveFacts, at now: Date) -> Bool {
        if case .tool = look(of: session, at: now) { return true }
        return false
    }

    /// Whether a session's thinking holds still: it retries a failed request, or its log has been quiet for
    /// `quietAfter` with nothing out, no sub-agent at work and nothing asked, waiting on the model alone.
    static func holds(_ session: SessionLiveFacts, at now: Date) -> Bool {
        if session.retrying { return true }
        return session.awaitsModel && !session.hasOpenCall && !session.subagents && session.requests.isEmpty
            && now.timeIntervalSince(session.observedAt) >= quietAfter
    }

    /// The badge a mark wears at `now`. A failed turn's `×`, until a prompt follows it, its card is looked at after it
    /// (`lookedAt`), or `badgeLife` passes; else a limit's `⧗`, while a turn's limit holds and no prompt has followed it,
    /// or one of the vendor's windows is used up. A window used up never outranks work: it is only ever the badge.
    static func badge(of mark: ReportView.QueueMark, lookedAt: Date?, at now: Date) -> BotBadge? {
        let prompted = mark.sessions.compactMap(\.turnStartedAt).max() ?? .distantPast
        let failed = mark.sessions.contains { session in
            guard session.failed, let end = session.endedAt else { return false }
            return now.timeIntervalSince(end) < badgeLife && end > (lookedAt ?? .distantPast) && prompted <= end
        }
        if failed { return .failed }
        let limited = mark.sessions.contains { session in
            session.limited(at: now) && prompted <= (session.endedAt ?? .distantFuture)
        }
        if limited { return .limited }
        if let until = mark.exhaustedUntil, until > now { return .limited }
        return nil
    }
}
