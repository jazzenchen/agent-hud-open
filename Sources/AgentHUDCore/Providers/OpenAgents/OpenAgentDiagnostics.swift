import Foundation

/// Local-only diagnostic: no account requests, settings writes, transcript text, or credentials in output.
public enum OpenAgentDiagnostics {
    public static func localSummary() async -> String {
        let store = OpenAgentLocalStore(paths: .init(home: FileManager.default.homeDirectoryForCurrentUser, environment: ProcessInfo.processInfo.environment))
        let since = Date().addingTimeInterval(-7 * 86400)
        var result = await store.index(since: since), rounds = 1
        while result.indexing != nil && rounds < 40 {
            result = await store.index(since: since); rounds += 1
        }
        var rows: [String] = []
        for source in [OpenAgentSource.opencode, .kimi, .pi] {
            let sessions = result.sessions.filter { $0.client == source }
            let events = UsageAggregation.usageUnion(sessions.map(\.events)).filter { $0.timestamp >= since }
            rows.append("\(source.name): \(sessions.count) sessions, \(events.count) distinct usage events; \(result.notices[source.name] ?? "OK")")
        }
        if let indexing = result.indexing { rows.append("Indexing: \(indexing.done)/\(indexing.total)") }
        return rows.joined(separator: "\n")
    }
}
