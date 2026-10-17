import Foundation

/// What one screen's dock, a Dynamic dock or a Simple panel, remembers of the events its marks have been handed. A dock
/// never hides its marks, but a mark can first be shown an event that fired before it was there to play it: the app had
/// just started, the screen was in Notch mode, or the agent had no mark yet.
struct DockMemory: Sendable {
    /// The newest event handed to each mark, by mark key, and the copy of it dated afresh that replays it.
    private var handed: [String: BotCue] = [:]
    private var replays: [String: BotCue] = [:]

    /// The event to hand the mark `key`, from the director's newest for it. One that is still fresh plays as it is. One
    /// that fired before the mark could play it is over, unless it was a failure or a limit whose badge the mark still
    /// wears: that plays once now, dated afresh. Asked again, the answer stays the same.
    mutating func cue(for key: String, _ cue: BotCue?, badge: BotBadge?, at now: Date) -> BotCue? {
        guard let cue else { return nil }
        if handed[key] == cue { return replays[key] ?? cue }
        handed[key] = cue
        replays[key] = nil
        guard now.timeIntervalSince(cue.at) > BotEvent.freshness else { return cue }
        let stands = switch cue.event {
        case .failed: badge == .failed
        case .limitHit: badge == .limited
        case .start: false
        }
        guard stands else { return cue }
        let replay = BotCue(event: cue.event, at: now)
        replays[key] = replay
        return replay
    }
}
