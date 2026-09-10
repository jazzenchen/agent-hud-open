import Foundation
import AgentHUDSupport

/// Combines local usage observations without counting the same event twice.
public enum UsageAggregation {
    /// Combine exact observations without counting copies twice. Preserve repeated equal events within one source.
    public static func eventUnion<T: Hashable>(_ sources: [[T]]) -> [T] {
        var maximum: [T: Int] = [:]
        var result: [T] = []
        for source in sources {
            var count: [T: Int] = [:]
            for event in source {
                count[event, default: 0] += 1
                if count[event]! > maximum[event, default: 0] { result.append(event) }
            }
            for (event, occurrences) in count { maximum[event] = max(maximum[event, default: 0], occurrences) }
        }
        return result
    }

    /// Cache reads enrich an existing usage observation; they do not create another input/output event.
    public static func usageUnion(_ sources: [[TranscriptSession.UsageEvent]]) -> [TranscriptSession.UsageEvent] {
        var priorities: [String: Int] = [:]
        for event in sources.flatMap({ $0 }) {
            if let origin = event.origin { priorities[origin.group] = max(priorities[origin.group] ?? origin.priority, origin.priority) }
        }
        let preferred = sources.map { $0.filter { event in
            event.origin.map { $0.priority == priorities[$0.group] } ?? true
        } }
        // Provider identities survive account-wide imports on multiple devices, including corrected observations.
        var identities = Set<String>()
        let identified = preferred.flatMap { $0 }.filter { event in
            event.eventID.map { identities.insert(RecordCoding.hash([$0, event.attribution?.pool?.id ?? ""])).inserted } ?? false
        }
        let sources = [identified] + preferred.map { $0.filter { $0.eventID == nil } }
        struct Key: Hashable { let timestamp: Date; let agent: String; let input: Int; let output: Int; let attribution: UsageAttribution? }
        var positions: [Key: [Int]] = [:]
        var result: [TranscriptSession.UsageEvent] = []
        for source in sources {
            let groups = Dictionary(grouping: source) { Key(timestamp: $0.timestamp, agent: $0.agentId, input: $0.tokensIn, output: $0.tokensOut, attribution: $0.attribution) }
            for (key, events) in groups {
                for (ordinal, event) in events.sorted(by: { $0.cacheReadTokens < $1.cacheReadTokens }).enumerated() {
                    if ordinal < positions[key, default: []].count {
                        let index = positions[key]![ordinal]
                        if event.cacheReadTokens > result[index].cacheReadTokens {
                            result[index] = .init(timestamp: event.timestamp, agentId: event.agentId, tokensIn: event.tokensIn,
                                tokensOut: event.tokensOut, cacheReadTokens: event.cacheReadTokens, eventID: result[index].eventID ?? event.eventID,
                                origin: result[index].origin ?? event.origin, attribution: result[index].attribution ?? event.attribution)
                        }
                    } else {
                        positions[key, default: []].append(result.count)
                        result.append(event)
                    }
                }
            }
        }
        return result
    }

    public static func costUnion(_ sources: [[APIBilling.CostSample]]) -> [APIBilling.CostSample] {
        var identities = Set<String>()
        return eventUnion(sources).filter { sample in
            guard let id = sample.eventID else { return true }
            return identities.insert(RecordCoding.hash([id, sample.billingPool?.id ?? ""])).inserted
        }
    }

    private struct HistoryKey: Hashable { let agent: String; let hour: Date }

    public static func historyUnion(_ sources: [[HistorySample]]) -> [HistorySample] {
        var seen: Set<HistoryKey> = []
        return sources.flatMap { $0 }.map { HistorySample(agentId: $0.agentId, hourStart: RecordCoding.date(RecordCoding.milliseconds($0.hourStart)),
            remainingStart: $0.remainingStart, remainingEnd: $0.remainingEnd, tokens: $0.tokens)
        }.filter { seen.insert(HistoryKey(agent: $0.agentId, hour: $0.hourStart)).inserted }
            .sorted { $0.hourStart < $1.hourStart }
    }

    public static func sessionsUnion(_ sources: [[LiveSession]]) -> [LiveSession] {
        var seen: Set<String> = []
        return sources.flatMap { $0 }.filter { seen.insert($0.id).inserted }.sorted {
            if $0.isLive != $1.isLive { return $0.isLive }
            return ($0.endedAt ?? $0.startedAt) > ($1.endedAt ?? $1.startedAt)
        }
    }

    public static func consumersUnion(_ sources: [[AgentDescriptor]]) -> [AgentDescriptor] {
        var seen: Set<String> = []
        return sources.flatMap { $0 }.filter { seen.insert($0.id).inserted }
    }

}
