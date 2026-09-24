import Foundation

/// Usage of readers that parse whole files or account records: one ledger contribution per session.
enum SessionContributions {
    /// Ledger events per session, resolved the way reports always combined these readers: only the highest-priority log
    /// format of each origin group counts, and an event id counts once, in the first session that holds it.
    static func canonical(_ sessions: [(id: String, events: [UsageEvent])]) -> [String: [UsageLedger.Event]] {
        var priorities: [String: Int] = [:]
        for session in sessions {
            for event in session.events {
                if let origin = event.origin { priorities[origin.group] = max(priorities[origin.group] ?? origin.priority, origin.priority) }
            }
        }
        var identities = Set<String>()
        var result: [String: [UsageLedger.Event]] = [:]
        for session in sessions {
            var ordinals: [String: Int] = [:]
            result[session.id, default: []] += session.events.compactMap { event in
                if let origin = event.origin, priorities[origin.group] != origin.priority { return nil }
                let key: String
                if let id = event.eventID {
                    guard identities.insert(id + "\u{1}" + (event.attribution?.pool?.id ?? "")).inserted else { return nil }
                    key = id
                } else {
                    let content = "\(Int64((event.timestamp.timeIntervalSince1970 * 1000).rounded())):\(event.agentId):\(event.tokensIn):\(event.tokensOut)"
                    let ordinal = ordinals[content, default: 0]
                    ordinals[content] = ordinal + 1
                    key = "\(content)#\(ordinal)"
                }
                return UsageLedger.Event(key: key, timestamp: event.timestamp, agentId: event.agentId, tokensIn: event.tokensIn,
                                         tokensOut: event.tokensOut, cacheReadTokens: event.cacheReadTokens,
                                         cacheWriteTokens: event.cacheWriteTokens, reasoningTokens: event.reasoningTokens)
            }
        }
        return result
    }

    /// Readers return a window of history, so each session is replaced from the start of the UTC day holding `since`;
    /// events the reader no longer returns before that stay until the ledger's retention.
    static func windowStart(_ since: Date) -> Date {
        Date(timeIntervalSince1970: (since.timeIntervalSince1970 / 86400).rounded(.down) * 86400)
    }
}
