import AgentHUDSupport
import Foundation
import SQLite3

enum AntigravitySessions {
    static func read(_ url: URL) throws -> ProviderSessions {
        let database = try ReadOnlySQLite(url)
        try database.requireTable("gen_metadata")
        var rows: [(index: Int64, turn: AntigravityProtoReader.ParsedTurn)] = [], incomplete = false
        try database.rows("SELECT idx, data FROM gen_metadata NOT INDEXED") { row in
            guard sqlite3_column_type(row, 0) == SQLITE_INTEGER, let bytes = ReadOnlySQLite.blob(row, 1),
                  let turn = try AntigravityProtoReader.parseTurn(Array(bytes), checkCancellation: { try Task.checkCancellation() }) else {
                incomplete = true; return
            }
            rows.append((sqlite3_column_int64(row, 0), turn))
        }
        var steps: [AntigravityProtoReader.StepMetadata] = []
        if rows.contains(where: { $0.turn.usage != nil && $0.turn.timestampMs == nil }), (try? database.requireTable("steps")) != nil {
            try database.rows("SELECT metadata FROM steps NOT INDEXED") { row in
                guard let bytes = ReadOnlySQLite.blob(row, 0), let step = try AntigravityProtoReader.parseStepMetadata(Array(bytes)) else {
                    incomplete = true; return
                }
                steps.append(step)
            }
        }
        let id = "antigravity:\(url.deletingPathExtension().lastPathComponent)"
        let client = url.path.contains("antigravity-cli/") ? "Antigravity CLI" : "Antigravity"
        var session = ProviderSession(id: id, title: "\(client) · \(url.deletingPathExtension().lastPathComponent.prefix(8))", path: url.path, client: client)
        var identities: [String: ProviderEvent] = [:]
        let labels = Dictionary(grouping: rows.map(\.turn).filter { $0.label != nil && $0.model != nil }, by: { $0.label! })
            .mapValues { Set($0.compactMap(\.model)) }
        for row in rows {
            let turn = row.turn
            guard let usage = turn.usage else { continue }
            let timestamp = turn.timestampMs ?? matchedTimestamp(turn, generations: rows.map(\.turn), steps: steps)
            guard let timestamp else { incomplete = true; continue }
            let (input, overflowIn) = usage.systemPrompt.addingReportingOverflow(usage.newInput)
            let (output, overflowOut) = usage.output.addingReportingOverflow(usage.reasoning)
            guard !overflowIn, !overflowOut else { throw ProviderFailure.format }
            let mapped = turn.label.flatMap { labels[$0] }.flatMap { $0.count == 1 ? $0.first : nil }
            let model = turn.model ?? mapped ?? turn.label ?? "Unknown"
            let identity = id + ":" + (usage.responseID ?? "row-\(row.index)")
            let event = ProviderEvent(id: identity, model: model, timestamp: RecordCoding.date(timestamp), input: input, output: output, cacheRead: usage.cacheRead)
            if let previous = identities[identity], previous != event { incomplete = true }
            else { identities[identity] = event }
        }
        session.events = identities.values.sorted { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }
        return ProviderSessions(sessions: [session], notice: incomplete
            ? L10n.text("部分 Antigravity 记录缺少可验证的时间或用量，未计入统计", "Some Antigravity records lack verifiable timestamps or usage and were excluded") : nil)
    }

    /// Only exact, unique joins are accepted. Opaque agy timestamps and file modification times are never usage times.
    static func matchedTimestamp(_ turn: AntigravityProtoReader.ParsedTurn,
        generations: [AntigravityProtoReader.ParsedTurn], steps: [AntigravityProtoReader.StepMetadata]) -> Int64? {
        if let bot = turn.usage?.botID {
            guard generations.filter({ $0.usage?.botID == bot }).count == 1 else { return nil }
            let matches = steps.filter { $0.botID == bot }
            guard matches.count == 1, let step = matches.first,
                  turn.stepUUID == nil || step.stepUUID == turn.stepUUID else { return nil }
            return step.timestampMs
        }
        guard let uuid = turn.stepUUID, generations.filter({ $0.stepUUID == uuid }).count == 1 else { return nil }
        let matches = steps.filter { $0.stepUUID == uuid }
        return matches.count == 1 ? matches.first?.timestampMs : nil
    }
}
