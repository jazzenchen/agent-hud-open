import Foundation
import SQLite3

/// Normal SQLite read transactions include active WAL data; no copying or immutable reads of live databases.
final class ReadOnlySQLite {
    private let database: OpaquePointer
    private let deadline: Date

    init(_ url: URL) throws {
        var handle: OpaquePointer?
        guard sqlite3_open_v2(url.path, &handle, SQLITE_OPEN_READONLY, nil) == SQLITE_OK, let handle else {
            if let handle { sqlite3_close(handle) }
            throw ProviderFailure.local
        }
        database = handle
        deadline = Date().addingTimeInterval(3)
        sqlite3_busy_timeout(handle, 250)
        sqlite3_limit(handle, SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024)
        sqlite3_progress_handler(handle, 1000, { pointer in
            guard let pointer else { return 1 }
            let reader = Unmanaged<ReadOnlySQLite>.fromOpaque(pointer).takeUnretainedValue()
            return Date() > reader.deadline || Task.isCancelled ? 1 : 0
        }, Unmanaged.passUnretained(self).toOpaque())
        guard sqlite3_exec(handle, "BEGIN", nil, nil, nil) == SQLITE_OK else { throw ProviderFailure.local }
    }

    deinit {
        sqlite3_progress_handler(database, 0, nil, nil)
        sqlite3_exec(database, "ROLLBACK", nil, nil, nil)
        sqlite3_close(database)
    }

    func rows(_ sql: String, strings: [String] = [], consume: (OpaquePointer) throws -> Void) throws {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK, let statement else { throw ProviderFailure.local }
        defer { sqlite3_finalize(statement) }
        for (index, value) in strings.enumerated() {
            _ = value.withCString { sqlite3_bind_text(statement, Int32(index + 1), $0, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self)) }
        }
        var count = 0, bytes = 0
        while true {
            try Task.checkCancellation()
            let result = sqlite3_step(statement)
            if result == SQLITE_DONE { return }
            guard result == SQLITE_ROW else { throw ProviderFailure.local }
            count += 1
            for index in 0..<sqlite3_column_count(statement) { bytes += Int(sqlite3_column_bytes(statement, index)) }
            guard count <= 10000, bytes <= 64 * 1024 * 1024, Date() <= deadline else { throw ProviderFailure.limit }
            try consume(statement)
        }
    }

    func requireTable(_ name: String) throws {
        var table = false
        try rows("SELECT type, sql FROM sqlite_master WHERE name = ?", strings: [name]) { row in
            let sql = Self.text(row, 1)?.uppercased() ?? ""
            table = Self.text(row, 0) == "table" && !sql.contains("VIRTUAL TABLE")
        }
        guard table else { throw ProviderFailure.format }
    }

    static func text(_ row: OpaquePointer, _ column: Int32) -> String? {
        guard sqlite3_column_type(row, column) != SQLITE_NULL else { return nil }
        let count = Int(sqlite3_column_bytes(row, column))
        guard let pointer = sqlite3_column_blob(row, column) else { return nil }
        let data = Data(bytes: pointer, count: count)
        if data.contains(0), let value = String(data: data, encoding: .utf16LittleEndian), !value.contains("\0") { return value }
        return String(data: data, encoding: .utf8)
    }
    static func blob(_ row: OpaquePointer, _ column: Int32) -> Data? {
        guard sqlite3_column_type(row, column) == SQLITE_BLOB, let pointer = sqlite3_column_blob(row, column) else { return nil }
        return Data(bytes: pointer, count: Int(sqlite3_column_bytes(row, column)))
    }
}
