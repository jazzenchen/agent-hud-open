import AgentHUDSupport
import Foundation
import SQLite3

// Per-model usage rows and the session-total fallback follow Tokscale hermes.rs (MIT).
enum HermesSessions: LocalSessionLayout {
    static let installPaths = [".hermes/state.db"]

    static func roots(home: URL, environment: [String: String]) -> [URL] {
        guard let value = environment["HERMES_HOME"]?.trimmingCharacters(in: .whitespaces), !value.isEmpty else { return [home.appendingPathComponent(".hermes")] }
        return [URL(fileURLWithPath: value)]
    }

    /// The Hermes home also holds skills, logs and often a source checkout; only `state.db` and `profiles/<name>/state.db` are visited.
    static func skips(_ url: URL) -> Bool {
        !(accepts(url) || url.lastPathComponent == "profiles" || url.deletingLastPathComponent().lastPathComponent == "profiles")
    }

    static func accepts(_ url: URL) -> Bool { url.lastPathComponent == "state.db" }

    static func related(_ url: URL) -> [URL] { [URL(fileURLWithPath: url.path + "-wal")] }

    static func read(_ url: URL) throws -> ProviderSessions { try read(url, since: Date().addingTimeInterval(-8 * 86400)) }

    /// One event per usage row, keyed by the row's primary key. Hermes adds to a row's counters in place, so its id stays
    /// fixed while the counts grow; the row is dated by its first use because there is no per-call time to difference.
    static func read(_ url: URL, since: Date) throws -> ProviderSessions {
        let db = try ReadOnlySQLite(url)
        var tables = Set<String>()
        try db.rows("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('sessions', 'session_model_usage')") { row in
            if let name = ReadOnlySQLite.text(row, 0) { tables.insert(name) }
        }
        guard tables.contains("sessions") else { throw ProviderFailure.format }
        func columns(_ table: String) throws -> Set<String> {
            var names = Set<String>()
            try db.rows("SELECT name FROM pragma_table_info(?)", strings: [table]) { if let name = ReadOnlySQLite.text($0, 0) { names.insert(name) } }
            return names
        }
        // Older databases lack later columns; a missing one reads as its default.
        let s = try columns("sessions"), u = tables.contains("session_model_usage") ? try columns("session_model_usage") : []
        func column(_ alias: String, _ names: Set<String>, _ name: String, _ fallback: String = "NULL") -> String {
            names.contains(name) ? "\(alias).\(name)" : fallback
        }
        let counts = ["input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens"]
        let lastSeen = u.contains("last_seen") ? "(SELECT MAX(last_seen) FROM session_model_usage WHERE session_id = s.id)" : "NULL"
        let activity = "MAX(s.started_at, COALESCE(\(column("s", s, "ended_at")), 0), COALESCE(\(column("s", s, "last_activity_at")), 0), COALESCE(\(lastSeen), 0))"
        let usage = u.isEmpty ? Array(repeating: "NULL", count: 11)
            : ["u.session_id", "u.model"] + ["billing_provider", "billing_base_url", "billing_mode", "task"].map { column("u", u, $0, "''") }
                + counts.map { column("u", u, $0, "0") } + [column("u", u, "first_seen")]
        let fields = ["s.id", "s.started_at", activity] + ["title", "cwd", "source", "profile_name"].map { column("s", s, $0) }
            // Hermes seeds a usage row from exactly these session values when it adds session_model_usage.
            + ["COALESCE(\(column("s", s, "model")), 'unknown')"] + ["billing_provider", "billing_base_url", "billing_mode"].map { "COALESCE(\(column("s", s, $0)), '')" }
            + counts.map { column("s", s, $0, "0") } + usage
        let join = u.isEmpty ? "" : "LEFT JOIN session_model_usage u ON u.session_id = s.id"
        let folder = url.deletingLastPathComponent()
        let profile = folder.deletingLastPathComponent().lastPathComponent == "profiles" ? folder.lastPathComponent : nil
        var sessions: [String: ProviderSession] = [:]
        try db.rows("SELECT \(fields.joined(separator: ", ")) FROM sessions s \(join) WHERE \(activity) >= CAST(? AS REAL)",
                    strings: [String(since.timeIntervalSince1970)]) { row in
            guard let raw = ReadOnlySQLite.text(row, 0), let started = seconds(row, 1) else { throw ProviderFailure.format }
            let id = "hermes:" + (profile.map { "\($0):" } ?? "") + raw
            if sessions[id] == nil {
                let name = ReadOnlySQLite.text(row, 6).flatMap { $0.isEmpty || $0 == "default" ? nil : $0 } ?? profile
                sessions[id] = ProviderSession(id: id, title: ReadOnlySQLite.text(row, 3) ?? "Hermes · \(ReadOnlySQLite.text(row, 5) ?? String(raw.prefix(8)))",
                    workspace: ReadOnlySQLite.text(row, 4), path: url.path, client: name.map { "Hermes Agent · \($0)" } ?? "Hermes Agent",
                    startedAt: started, lastActivity: seconds(row, 2))
            }
            // A session without usage rows counts its totals under the key Hermes would seed for it, so an upgrade keeps the id.
            let rows = sqlite3_column_type(row, 15) != SQLITE_NULL
            let key = rows ? (Int32(16)...20).map { ReadOnlySQLite.text(row, $0) ?? "" } : (Int32(7)...10).map { ReadOnlySQLite.text(row, $0) ?? "" } + [""]
            let offset: Int32 = rows ? 21 : 11
            let input = try count(row, offset), output = try count(row, offset + 1), read = try count(row, offset + 2), write = try count(row, offset + 3)
            guard try TokenCount.sum(input, output, read, write) > 0 else { return }
            // Input excludes cache reads and writes; reasoning is already inside output.
            sessions[id]?.events.append(ProviderEvent(id: "usage:" + RecordCoding.hash([id] + key), model: key[0],
                timestamp: rows ? seconds(row, 25) ?? started : started, input: try TokenCount.sum(input, write), output: output, cacheRead: read,
                cacheWrite: write))
        }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { sessions[$0] })
    }

    private static func seconds(_ row: OpaquePointer, _ column: Int32) -> Date? {
        let type = sqlite3_column_type(row, column)
        guard type == SQLITE_INTEGER || type == SQLITE_FLOAT else { return nil }
        let value = sqlite3_column_double(row, column)
        return value > 0 ? Date(timeIntervalSince1970: value) : nil
    }

    private static func count(_ row: OpaquePointer, _ column: Int32) throws -> Int {
        switch sqlite3_column_type(row, column) {
        case SQLITE_NULL: return 0
        case SQLITE_INTEGER where sqlite3_column_int64(row, column) >= 0: return Int(sqlite3_column_int64(row, column))
        default: throw ProviderFailure.format
        }
    }
}
