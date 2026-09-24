import AgentHUDSupport
import Foundation
import SQLite3

// ZCode `model_usage` columns and token layouts follow Tokscale zcode.rs (MIT).
enum ZCodeSessions: LocalSessionLayout {
    static let installPaths = [".zcode/cli/db/db.sqlite"]
    static func roots(home: URL, environment: [String: String]) -> [URL] { [home.appendingPathComponent(".zcode/cli/db")] }
    static func accepts(_ url: URL) -> Bool { url.lastPathComponent == "db.sqlite" }
    static func related(_ url: URL) -> [URL] { [URL(fileURLWithPath: url.path + "-wal")] }

    /// The database keeps every request ever made; the report needs the last week.
    static func read(_ url: URL) throws -> ProviderSessions { try read(url, since: Date().addingTimeInterval(-8 * 86400)) }

    static func read(_ url: URL, since: Date) throws -> ProviderSessions {
        let database = try ReadOnlySQLite(url)
        try database.requireTable("model_usage")
        let usage = try columns("model_usage", in: database)
        let session = (try? database.requireTable("session")) != nil ? try columns("session", in: database) : []
        let joined = session.contains("id") && (session.contains("directory") || session.contains("path"))
        let workspace = joined ? ["directory", "path"].map { session.contains($0) ? "NULLIF(s.\($0), '')" : "NULL" }.joined(separator: ", ") : "NULL, NULL"
        let time = "COALESCE(mu.completed_at, mu.started_at)", limit = 10000
        // Without `computed_total_tokens` (older schema) every row is cache- and reasoning-inclusive.
        let sql = """
            SELECT mu.id, NULLIF(mu.session_id, ''), NULLIF(mu.model_id, ''), mu.started_at, mu.completed_at, mu.input_tokens, mu.output_tokens,
                mu.reasoning_tokens, mu.cache_read_input_tokens, mu.cache_creation_input_tokens, \(usage.contains("computed_total_tokens") ? "mu.computed_total_tokens" : "NULL"), \(workspace)
            FROM model_usage mu \(joined ? "LEFT JOIN session s ON s.id = mu.session_id" : "")
            WHERE \(time) >= CAST(? AS REAL) ORDER BY \(time) DESC, mu.id DESC LIMIT \(limit)
            """
        var sessions: [String: ProviderSession] = [:], count = 0, incomplete = false
        try database.rows(sql, strings: [String(RecordCoding.milliseconds(since))]) { row in
            count += 1
            guard let rowID = ReadOnlySQLite.text(row, 0), let date = ProviderDate.milliseconds(number(row, 4) ?? number(row, 3) ?? .null),
                  let counts = try? (5...10).map({ try counter(row, Int32($0)) }),
                  let tokens = try? self.tokens(input: counts[0] ?? 0, output: counts[1] ?? 0, reasoning: counts[2] ?? 0,
                                                cacheRead: counts[3] ?? 0, cacheWrite: counts[4] ?? 0, total: counts[5],
                                                inclusiveWithoutTotal: !usage.contains("computed_total_tokens")) else {
                incomplete = true; return
            }
            guard tokens.input > 0 || tokens.output > 0 || tokens.cache > 0 else { return }
            let rawID = ReadOnlySQLite.text(row, 1) ?? "unknown", id = "zcode:\(rawID)"
            let folder = ReadOnlySQLite.text(row, 11) ?? ReadOnlySQLite.text(row, 12)
            let name = folder.map { URL(fileURLWithPath: $0).lastPathComponent }.flatMap { $0.isEmpty ? nil : $0 }
            var session = sessions[id] ?? ProviderSession(id: id, title: name ?? "ZCode · \(rawID.prefix(8))", workspace: folder, path: url.path, client: "ZCode")
            let started = ProviderDate.milliseconds(number(row, 3) ?? .null) ?? date
            session.startedAt = min(session.startedAt ?? started, started)
            session.lastActivity = max(session.lastActivity ?? date, date)
            session.events.append(.init(id: rowID, model: ReadOnlySQLite.text(row, 2) ?? "Unknown", timestamp: date,
                                        input: tokens.input, output: tokens.output, cacheRead: tokens.cache,
                                        cacheWrite: counts[4] ?? 0, reasoning: counts[2] ?? 0))
            sessions[id] = session
        }
        let notices = [incomplete ? L10n.text("部分 ZCode 用量记录缺少时间或计数无法核对，未计入统计", "Some ZCode usage records lack a time or have inconsistent counts and were excluded") : nil,
                       count >= limit ? ProviderFailure.limit.message : nil].compactMap { $0 }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { key in
            sessions[key].map { var item = $0; item.events.sort { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }; return item }
        }, notice: notices.isEmpty ? nil : notices.joined(separator: " · "))
    }

    /// When the reported total equals input + output, input already holds both cache kinds and output holds reasoning;
    /// otherwise the five columns are additive. In keeps cache writes, Cache is cache reads only.
    static func tokens(input: Int, output: Int, reasoning: Int, cacheRead: Int, cacheWrite: Int, total: Int?,
                       inclusiveWithoutTotal: Bool) throws -> (input: Int, output: Int, cache: Int) {
        let inclusive = try total.map { try $0 == TokenCount.sum(input, output) } ?? inclusiveWithoutTotal
        guard inclusive else { return (try TokenCount.sum(input, cacheWrite), try TokenCount.sum(output, reasoning), cacheRead) }
        guard cacheRead <= input else { throw ProviderFailure.format }
        return (input - cacheRead, output, cacheRead)
    }

    private static func columns(_ table: String, in database: ReadOnlySQLite) throws -> Set<String> {
        var names = Set<String>()
        try database.rows("SELECT name FROM pragma_table_info(?)", strings: [table]) { row in
            if let name = ReadOnlySQLite.text(row, 0) { names.insert(name) }
        }
        return names
    }

    private static func number(_ row: OpaquePointer, _ column: Int32) -> ProviderJSON? {
        switch sqlite3_column_type(row, column) {
        case SQLITE_INTEGER: .integer(sqlite3_column_int64(row, column))
        case SQLITE_FLOAT: .number(sqlite3_column_double(row, column))
        default: nil
        }
    }

    private static func counter(_ row: OpaquePointer, _ column: Int32) throws -> Int? {
        guard sqlite3_column_type(row, column) != SQLITE_NULL else { return nil }
        guard let count = number(row, column)?.countValue else { throw ProviderFailure.format }
        return count
    }
}
