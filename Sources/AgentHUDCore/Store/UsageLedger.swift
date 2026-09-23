import AgentHUDSupport
import Foundation

/// The Mac's local store of token observations and the 15-minute totals derived from them.
///
/// Providers write canonical events grouped into contributions, usually one session after overlapping logs are
/// resolved, and keep their parse positions here. Buckets change by exact deltas inside the same transaction,
/// so a pass touches only what it read, and expired rows are deleted instead of rewritten.
public actor UsageLedger {
    public static let bucketMilliseconds: Int64 = 900_000
    /// Rows outlive the thirty-day sync retention by the day that crosses it.
    public static let retention: TimeInterval = 31 * 86400
    public static var defaultURL: URL { AppSupport.directory.appendingPathComponent("usage-ledger.sqlite") }

    /// A provider's resumable position in one source file.
    public struct FileState: Hashable, Sendable {
        public var signature: String
        public var state: Data?
        /// Files of the same group describe one session, such as a rollout and its archived copy.
        public var group: String?
        public init(signature: String, state: Data? = nil, group: String? = nil) {
            self.signature = signature; self.state = state; self.group = group
        }
    }

    /// One token observation after the provider resolved duplicates; `key` is unique within its contribution.
    public struct Event: Hashable, Sendable {
        public let key: String
        public let timestamp: Date
        public let agentId: String
        public let tokensIn: Int
        public let tokensOut: Int
        public let cacheReadTokens: Int
        /// Estimated price by currency; nil for events that belong to no priced account.
        public let costs: [String: Decimal]?
        public let billingID: String?

        public init(key: String, timestamp: Date, agentId: String, tokensIn: Int, tokensOut: Int, cacheReadTokens: Int = 0,
                    billingID: String? = nil, costs: [String: Decimal]? = nil) {
            self.key = key; self.timestamp = timestamp; self.agentId = agentId
            self.tokensIn = tokensIn; self.tokensOut = tokensOut; self.cacheReadTokens = cacheReadTokens
            self.billingID = billingID; self.costs = costs
        }
    }

    private let storage: LedgerStorage
    private var passOpen = false
    private var expiredAt: Date?
    /// Changes when a failed pass rolled back writes that providers may already reflect in memory.
    public private(set) var generation = 0

    /// - expires: deletes rows older than `retention`, and ignores such rows on write. Fixtures with fixed dates keep everything.
    public init(url: URL?, expires: Bool = false) throws {
        storage = try LedgerStorage(url: url, retention: expires ? Self.retention : nil)
    }

    /// A private store for tests and previews.
    public static func inMemory() -> UsageLedger {
        // An in-memory database has no file to fail on.
        try! UsageLedger(url: nil)
    }

    /// The persistent store; an unreadable file is recreated, and without a usable file the store lives in memory.
    public static func open(url: URL = defaultURL) -> UsageLedger {
        if let ledger = try? UsageLedger(url: url, expires: true) { return ledger }
        for suffix in ["", "-wal", "-shm"] { try? FileManager.default.removeItem(atPath: url.path + suffix) }
        if let ledger = try? UsageLedger(url: url, expires: true) { return ledger }
        NSLog("[AgentHUD] Usage ledger unavailable at %@; keeping this session in memory", url.path)
        return try! UsageLedger(url: nil, expires: true)
    }

    // MARK: Writing

    /// Everything one pass reads commits together. Writes outside a pass commit on their own.
    public func beginPass() {
        guard !passOpen else { return }
        do {
            try storage.connection.execute("BEGIN IMMEDIATE")
            passOpen = true
        } catch { NSLog("[AgentHUD] Usage ledger pass could not start: %@", error.localizedDescription) }
    }

    public func commitPass() {
        guard passOpen else { return }
        passOpen = false
        do {
            try storage.connection.execute("COMMIT")
            // Expired rows leave in their own small transaction about once an hour.
            let now = Date()
            if storage.retention != nil, expiredAt.map({ now.timeIntervalSince($0) >= 3600 }) ?? true {
                expiredAt = now
                try? expire(now: now)
            }
        } catch {
            rollBackPass()
            NSLog("[AgentHUD] Usage ledger pass rolled back: %@", error.localizedDescription)
        }
    }

    /// Discards everything the open pass wrote; providers that remember any of it read their state again.
    func rollBackPass() {
        passOpen = false
        try? storage.connection.execute("ROLLBACK")
        storage.reset()
        generation += 1
    }

    /// Runs `body` atomically: inside an open pass it is a savepoint, otherwise its own transaction.
    public func write<T: Sendable>(_ body: @Sendable (LedgerWriter) throws -> T) throws -> T {
        let writer = LedgerWriter(storage: storage)
        if passOpen {
            try storage.connection.execute("SAVEPOINT provider")
            do {
                let value = try body(writer)
                try storage.connection.execute("RELEASE provider")
                return value
            } catch {
                try? storage.connection.execute("ROLLBACK TO provider")
                try? storage.connection.execute("RELEASE provider")
                storage.reset()
                throw error
            }
        }
        return try storage.connection.transaction { try body(writer) }
    }

    /// Deletes rows that left the retention window, aligned to a bucket so no bucket keeps half its events.
    public func expire(now: Date) throws {
        let cutoff = LedgerWriter.bucket(RecordCoding.milliseconds(now.addingTimeInterval(-Self.retention)))
        let samples = now.addingTimeInterval(-QuotaHistoryStore.retention).timeIntervalSinceReferenceDate
        _ = try write { writer in
            let connection = writer.storage.connection
            try connection.run("DELETE FROM usage_event WHERE timestamp_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM usage_bucket WHERE start_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM cost_amount WHERE timestamp_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM cost_bucket WHERE start_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM quota_sample WHERE observed_at < ?", [.real(samples)])
            try connection.run("""
                DELETE FROM contribution WHERE NOT EXISTS (SELECT 1 FROM usage_event WHERE contribution_id = contribution.id)
                AND NOT EXISTS (SELECT 1 FROM cost_amount WHERE contribution_id = contribution.id)
                """)
        }
    }

    // MARK: Reading

    public func fileStates(source: String) throws -> [String: FileState] {
        try LedgerWriter(storage: storage).fileStates(source: source)
    }

    /// Token buckets of the period holding `since` and later, ordered by start, account and consumer.
    /// Without `source` every source is included.
    public func buckets(since: Date, source: String? = nil) throws -> [UsageBucket] {
        var result: [UsageBucket] = []
        let start = RecordCoding.milliseconds(since) / Self.bucketMilliseconds * Self.bucketMilliseconds
        try storage.connection.query("""
            SELECT start_ms, account, agent, SUM(tokens_in), SUM(tokens_out), SUM(cache_read) FROM usage_bucket
            WHERE start_ms >= ? AND (? IS NULL OR source = ?) GROUP BY start_ms, account, agent
            """, [.integer(start), .nullable(source), .nullable(source)]) { row in
            result.append(UsageBucket(start: RecordCoding.date(row.int(0)), agentId: storage.agentName(row.int(2)),
                tokensIn: Int(row.int(3)), tokensOut: Int(row.int(4)), cacheReadTokens: Int(row.int(5)),
                account: row.text(1).flatMap { $0.isEmpty ? nil : $0 }))
        }
        return result.sorted { ($0.start, $0.account ?? "", $0.agentId) < ($1.start, $1.account ?? "", $1.agentId) }
    }

    /// Cost buckets by billing account; a currency missing from `amounts` had an unpriced event in that bucket.
    public func costBuckets(since: Date) throws -> [String: [CostBucket]] {
        let start = RecordCoding.milliseconds(since) / Self.bucketMilliseconds * Self.bucketMilliseconds
        var rows: [String: [Int64: (events: Int64, amounts: [String: (Int64, Int64)])]] = [:]
        try storage.connection.query("""
            SELECT billing, start_ms, currency, amount_pico, events FROM cost_bucket WHERE start_ms >= ?
            """, [.integer(start)]) { row in
            let billing = row.text(0) ?? "", bucket = row.int(1), currency = row.text(2) ?? ""
            var entry = rows[billing, default: [:]][bucket] ?? (0, [:])
            if currency.isEmpty { entry.events = row.int(4) } else { entry.amounts[currency] = (row.int(3), row.int(4)) }
            rows[billing, default: [:]][bucket] = entry
        }
        return rows.mapValues { buckets in
            buckets.keys.sorted().map { start in
                let entry = buckets[start]!
                let amounts = entry.amounts.filter { $0.value.1 == entry.events }.mapValues(LedgerWriter.decimal)
                return CostBucket(start: RecordCoding.date(start), amounts: amounts)
            }
        }
    }

    /// Estimated cost of each contribution by currency; a currency is missing when any of its events was unpriced.
    public func contributionCosts(source: String) throws -> [String: [String: Decimal]] {
        var events: [String: Int64] = [:], amounts: [String: [String: (Int64, Int64)]] = [:]
        try storage.connection.query("""
            SELECT c.key, a.currency, SUM(a.amount_pico), COUNT(*) FROM cost_amount a JOIN contribution c ON c.id = a.contribution_id
            WHERE c.source = ? GROUP BY c.key, a.currency
            """, [.text(source)]) { row in
            let key = row.text(0) ?? "", currency = row.text(1) ?? ""
            if currency.isEmpty { events[key] = row.int(3) } else { amounts[key, default: [:]][currency] = (row.int(2), row.int(3)) }
        }
        return amounts.reduce(into: [:]) { result, entry in
            let total = events[entry.key] ?? 0
            result[entry.key] = entry.value.filter { $0.value.1 == total }.mapValues(LedgerWriter.decimal)
        }
    }

    /// Input plus output tokens of each contribution at or after `since`.
    public func tokens(source: String, since: Date) throws -> [String: Int] {
        var result: [String: Int] = [:]
        try storage.connection.query("""
            SELECT c.key, SUM(e.tokens_in + e.tokens_out) FROM usage_event e JOIN contribution c ON c.id = e.contribution_id
            WHERE c.source = ? AND e.timestamp_ms >= ? GROUP BY c.key
            """, [.text(source), .integer(RecordCoding.milliseconds(since))]) { row in
            result[row.text(0) ?? ""] = Int(row.int(1))
        }
        return result
    }

    /// What each requested session spent, by model and 15-minute period. Sessions without recorded events are left out.
    public func sessionUsage(_ requests: [SessionUsageRequest]) throws -> [String: SessionUsage] {
        var result: [String: SessionUsage] = [:]
        for request in requests {
            var own: [Int64] = [], callLog: [Int64] = [], subagents: Set<Int64> = []
            for key in Set(request.keys) {
                try storage.connection.query("SELECT id FROM contribution WHERE key = ?", [.text(key)]) { row in
                    own.append(row.int(0))
                    if key == request.callLog { callLog.append(row.int(0)) }
                }
            }
            if let prefix = request.subagentPrefix, let last = prefix.unicodeScalars.last,
               let next = Unicode.Scalar(last.value + 1) {
                // Every key that starts with the prefix sorts at or after it and before the prefix with its last character raised.
                let end = String(prefix.unicodeScalars.dropLast()) + String(next)
                try storage.connection.query("SELECT id FROM contribution WHERE key >= ? AND key < ?", [.text(prefix), .text(end)]) {
                    subagents.insert($0.int(0))
                }
            }
            let ids = own + subagents
            guard !ids.isEmpty else { continue }
            let list = "(" + ids.map { _ in "?" }.joined(separator: ", ") + ")"
            struct Slot: Hashable { let start: Int64; let agent: String }
            var models: [String: SessionUsage.Tokens] = [:], periods: [Slot: SessionUsage.Tokens] = [:]
            var spent: SessionUsage.Tokens?, calls = 0
            try storage.connection.query("""
                SELECT contribution_id, timestamp_ms / ? * ?, agent, SUM(tokens_in), SUM(tokens_out), SUM(cache_read), COUNT(*)
                FROM usage_event WHERE contribution_id IN \(list) GROUP BY 1, 2, 3
                """, [.integer(Self.bucketMilliseconds), .integer(Self.bucketMilliseconds)] + ids.map { .integer($0) }) { row in
                let tokens = SessionUsage.Tokens(tokensIn: Int(row.int(3)), tokensOut: Int(row.int(4)), cacheReadTokens: Int(row.int(5)))
                let agent = storage.agentName(row.int(2))
                models[agent, default: .init()] += tokens
                periods[Slot(start: row.int(1), agent: agent), default: .init()] += tokens
                if subagents.contains(row.int(0)) { spent = (spent ?? .init()) + tokens }
                calls += Int(row.int(6))
            }
            guard calls > 0 else { continue }
            var context: Int?
            if !callLog.isEmpty {
                try storage.connection.query("""
                    SELECT tokens_in + cache_read FROM usage_event WHERE contribution_id IN (\(callLog.map { _ in "?" }.joined(separator: ", ")))
                    ORDER BY timestamp_ms DESC LIMIT 1
                    """, callLog.map { .integer($0) }) { context = Int($0.int(0)) }
            }
            var priced: Int64 = 0, amounts: [String: (Int64, Int64)] = [:]
            try storage.connection.query("""
                SELECT currency, SUM(amount_pico), COUNT(*) FROM cost_amount WHERE contribution_id IN \(list) GROUP BY currency
                """, ids.map { .integer($0) }) { row in
                let currency = row.text(0) ?? ""
                if currency.isEmpty { priced = row.int(2) } else { amounts[currency] = (row.int(1), row.int(2)) }
            }
            let costs = amounts.filter { $0.value.1 == priced }.mapValues(LedgerWriter.decimal)
            result[request.sessionID] = SessionUsage(
                models: models.map { SessionUsage.Model(agentId: $0.key, tokens: $0.value) }.sorted {
                    let left = $0.tokens.tokensIn + $0.tokens.tokensOut, right = $1.tokens.tokensIn + $1.tokens.tokensOut
                    return left == right ? $0.agentId < $1.agentId : left > right
                },
                periods: periods.keys.sorted { ($0.start, $0.agent) < ($1.start, $1.agent) }.map {
                    SessionUsage.Period(start: RecordCoding.date($0.start), agentId: $0.agent, tokens: periods[$0]!)
                },
                subagents: spent, calls: calls, contextTokens: context,
                costs: priced > 0 && !costs.isEmpty ? costs : nil)
        }
        return result
    }

    public func samples(scope: String, windowID: String, since: Date) throws -> [QuotaSample] {
        var result: [QuotaSample] = []
        try storage.connection.query("""
            SELECT observed_at, remaining FROM quota_sample WHERE scope = ? AND window_id = ? AND observed_at >= ? ORDER BY observed_at
            """, [.text(scope), .text(windowID), .real(since.timeIntervalSinceReferenceDate)]) { row in
            result.append(QuotaSample(agentId: windowID, timestamp: Date(timeIntervalSinceReferenceDate: row.double(0)), remainingPct: row.double(1)))
        }
        return result
    }

    public func sampleCount(scope: String) throws -> Int {
        var count = 0
        try storage.connection.query("SELECT COUNT(*) FROM quota_sample WHERE scope = ?", [.text(scope)]) { count = Int($0.int(0)) }
        return count
    }
}

/// Estimated money in one 15-minute period of one billing account.
public struct CostBucket: Hashable, Codable, Sendable {
    public let start: Date
    /// Exact sums by currency. A currency is absent when an event in the period had no price in it.
    public let amounts: [String: Decimal]

    public init(start: Date, amounts: [String: Decimal]) {
        self.start = start
        self.amounts = amounts
    }

    public var end: Date { start.addingTimeInterval(UsageBucket.duration) }
    public func overlaps(_ interval: DateInterval) -> Bool { end > interval.start && start < interval.end }
}

/// The ledger's connection and consumer catalog; confined to the ledger actor.
final class LedgerStorage {
    let connection: SQLiteConnection
    let retention: TimeInterval?
    private var agents: [String: Int64] = [:]
    private var names: [Int64: String] = [:]

    init(url: URL?, retention: TimeInterval?) throws {
        self.retention = retention
        connection = try SQLiteConnection(url: url)
        try connection.execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = OFF;")
        var version: Int64 = 0
        try connection.query("PRAGMA user_version") { version = $0.int(0) }
        if version < 1 {
            try connection.transaction {
                try connection.execute("""
                    CREATE TABLE IF NOT EXISTS source_file (
                        source TEXT NOT NULL, path TEXT NOT NULL, signature TEXT NOT NULL, state BLOB, file_group TEXT,
                        PRIMARY KEY (source, path)) WITHOUT ROWID;
                    CREATE TABLE IF NOT EXISTS contribution (
                        id INTEGER PRIMARY KEY, source TEXT NOT NULL, key TEXT NOT NULL, account TEXT NOT NULL DEFAULT '',
                        counted INTEGER NOT NULL DEFAULT 1, digest INTEGER, UNIQUE (source, key));
                    CREATE TABLE IF NOT EXISTS agent (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
                    CREATE TABLE IF NOT EXISTS usage_event (
                        contribution_id INTEGER NOT NULL, event INTEGER NOT NULL, timestamp_ms INTEGER NOT NULL, agent INTEGER NOT NULL,
                        tokens_in INTEGER NOT NULL, tokens_out INTEGER NOT NULL, cache_read INTEGER NOT NULL,
                        PRIMARY KEY (contribution_id, event)) WITHOUT ROWID;
                    CREATE INDEX IF NOT EXISTS usage_event_time ON usage_event (timestamp_ms);
                    CREATE TABLE IF NOT EXISTS usage_bucket (
                        start_ms INTEGER NOT NULL, source TEXT NOT NULL, account TEXT NOT NULL, agent INTEGER NOT NULL,
                        tokens_in INTEGER NOT NULL, tokens_out INTEGER NOT NULL, cache_read INTEGER NOT NULL,
                        PRIMARY KEY (start_ms, source, account, agent)) WITHOUT ROWID;
                    CREATE TABLE IF NOT EXISTS cost_amount (
                        contribution_id INTEGER NOT NULL, event INTEGER NOT NULL, currency TEXT NOT NULL, timestamp_ms INTEGER NOT NULL,
                        billing TEXT NOT NULL, amount_pico INTEGER NOT NULL,
                        PRIMARY KEY (contribution_id, event, currency)) WITHOUT ROWID;
                    CREATE INDEX IF NOT EXISTS cost_amount_time ON cost_amount (timestamp_ms);
                    CREATE TABLE IF NOT EXISTS cost_bucket (
                        billing TEXT NOT NULL, start_ms INTEGER NOT NULL, currency TEXT NOT NULL,
                        amount_pico INTEGER NOT NULL, events INTEGER NOT NULL,
                        PRIMARY KEY (billing, start_ms, currency)) WITHOUT ROWID;
                    CREATE TABLE IF NOT EXISTS quota_sample (
                        scope TEXT NOT NULL, window_id TEXT NOT NULL, observed_at REAL NOT NULL, remaining REAL NOT NULL,
                        PRIMARY KEY (scope, window_id, observed_at)) WITHOUT ROWID;
                    CREATE INDEX IF NOT EXISTS quota_sample_time ON quota_sample (observed_at);
                    PRAGMA user_version = 1;
                    """)
            }
        }
        if version < 2 {
            // A session's contributions are found by key alone: its log path or id, and the paths under its directory.
            try connection.execute("CREATE INDEX IF NOT EXISTS contribution_key ON contribution (key); PRAGMA user_version = 2;")
        }
        try connection.query("SELECT id, name FROM agent") { row in
            let id = row.int(0), name = row.text(1) ?? ""
            agents[name] = id
            names[id] = name
        }
    }

    func agentID(_ name: String) throws -> Int64 {
        if let id = agents[name] { return id }
        try connection.run("INSERT OR IGNORE INTO agent (name) VALUES (?)", [.text(name)])
        var id: Int64 = 0
        try connection.query("SELECT id FROM agent WHERE name = ?", [.text(name)]) { id = $0.int(0) }
        agents[name] = id
        names[id] = name
        return id
    }

    func agentName(_ id: Int64) -> String { names[id] ?? "" }

    /// A rolled-back transaction can leave catalog entries that no longer exist.
    func reset() {
        agents = [:]
        names = [:]
        try? connection.query("SELECT id, name FROM agent") { row in
            let id = row.int(0), name = row.text(1) ?? ""
            agents[name] = id
            names[id] = name
        }
    }
}

/// Statements of one atomic ledger write.
public struct LedgerWriter {
    let storage: LedgerStorage

    public func fileStates(source: String) throws -> [String: UsageLedger.FileState] {
        var result: [String: UsageLedger.FileState] = [:]
        try storage.connection.query("SELECT path, signature, state, file_group FROM source_file WHERE source = ?", [.text(source)]) { row in
            result[row.text(0) ?? ""] = UsageLedger.FileState(signature: row.text(1) ?? "", state: row.blob(2), group: row.text(3))
        }
        return result
    }

    public func setFile(source: String, path: String, state: UsageLedger.FileState) throws {
        try storage.connection.run("""
            INSERT INTO source_file (source, path, signature, state, file_group) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (source, path) DO UPDATE SET signature = excluded.signature, state = excluded.state, file_group = excluded.file_group
            """, [.text(source), .text(path), .text(state.signature), state.state.map { .blob($0) } ?? .null, .nullable(state.group)])
    }

    public func removeFile(source: String, path: String) throws {
        try storage.connection.run("DELETE FROM source_file WHERE source = ? AND path = ?", [.text(source), .text(path)])
    }

    /// Adds events or corrects events with the same key; other events of the contribution stay.
    /// A contribution that is not `counted` keeps its events out of the buckets, such as an older copy of a moved log.
    public func upsert(source: String, contribution: String, account: String? = nil, counted: Bool? = nil, events: [UsageLedger.Event]) throws {
        if let counted { try setCounted(source: source, contribution: contribution, account: account, counted: counted) }
        guard !events.isEmpty else { return }
        let id = try contributionID(source: source, key: contribution, account: account)
        try storage.connection.run("UPDATE contribution SET digest = NULL WHERE id = ?", [.integer(id.id)])
        let cutoff = cutoff(), billed = try hasCosts(id)
        for event in events where RecordCoding.milliseconds(event.timestamp) >= cutoff {
            try write(event, contribution: id, billed: billed)
        }
    }

    /// Makes `events` the whole contribution, or with `since` only its events from then on, keeping older ones that a
    /// reader no longer returns. An unchanged contribution is left untouched.
    public func replace(source: String, contribution: String, account: String? = nil, events: [UsageLedger.Event], since: Date? = nil) throws {
        let floor = max(cutoff(), since.map(RecordCoding.milliseconds) ?? .min)
        let kept = events.filter { RecordCoding.milliseconds($0.timestamp) >= floor }.sorted { $0.key < $1.key }
        let digest = Self.digest(kept, account: account, since: since)
        var stored: (id: Int64, account: String, digest: Int64?)?
        try storage.connection.query("SELECT id, account, digest FROM contribution WHERE source = ? AND key = ?",
                                     [.text(source), .text(contribution)]) { row in
            stored = (row.int(0), row.text(1) ?? "", row.isNull(2) ? nil : row.int(2))
        }
        if let stored, stored.digest == digest { return }
        if let stored {
            if since == nil || stored.account != (account ?? "") {
                try remove(source: source, contribution: contribution)
            } else {
                try removeEvents(of: try contributionID(source: source, key: contribution, account: account), since: floor)
            }
        }
        if kept.isEmpty {
            guard stored != nil else { return }
            // Nothing left to count: an emptied contribution goes, one that still holds older events remembers this answer.
            try storage.connection.run("""
                DELETE FROM contribution WHERE source = ? AND key = ? AND NOT EXISTS (SELECT 1 FROM usage_event WHERE contribution_id = contribution.id)
                AND NOT EXISTS (SELECT 1 FROM cost_amount WHERE contribution_id = contribution.id)
                """, [.text(source), .text(contribution)])
            try storage.connection.run("UPDATE contribution SET digest = ? WHERE source = ? AND key = ?", [.integer(digest), .text(source), .text(contribution)])
            return
        }
        let id = try contributionID(source: source, key: contribution, account: account)
        for event in kept { try write(event, contribution: id, billed: false) }
        try storage.connection.run("UPDATE contribution SET digest = ? WHERE id = ?", [.integer(digest), .integer(id.id)])
    }

    /// Moves a contribution's events into or out of the buckets without rewriting them.
    public func setCounted(source: String, contribution: String, account: String? = nil, counted: Bool) throws {
        let id = try contributionID(source: source, key: contribution, account: account)
        guard id.counted != counted else { return }
        try applyTotals(of: id, sign: counted ? 1 : -1)
        try storage.connection.run("UPDATE contribution SET counted = ? WHERE id = ?", [.integer(counted ? 1 : 0), .integer(id.id)])
    }

    public func remove(source: String, contribution: String) throws {
        var found: ContributionID?
        try storage.connection.query("SELECT id, account, counted FROM contribution WHERE source = ? AND key = ?", [.text(source), .text(contribution)]) { row in
            found = ContributionID(id: row.int(0), source: source, account: row.text(1) ?? "", counted: row.int(2) != 0)
        }
        guard let found else { return }
        if found.counted { try applyTotals(of: found, sign: -1) }
        try storage.connection.run("DELETE FROM usage_event WHERE contribution_id = ?", [.integer(found.id)])
        try storage.connection.run("DELETE FROM cost_amount WHERE contribution_id = ?", [.integer(found.id)])
        try storage.connection.run("DELETE FROM contribution WHERE id = ?", [.integer(found.id)])
    }

    /// Deletes a contribution's events from `since` on, with their share of the buckets.
    private func removeEvents(of found: ContributionID, since: Int64) throws {
        if found.counted { try applyTotals(of: found, sign: -1, since: since) }
        try storage.connection.run("DELETE FROM usage_event WHERE contribution_id = ? AND timestamp_ms >= ?", [.integer(found.id), .integer(since)])
        try storage.connection.run("DELETE FROM cost_amount WHERE contribution_id = ? AND timestamp_ms >= ?", [.integer(found.id), .integer(since)])
    }

    /// Adds (`sign` 1) or subtracts (-1) what a contribution holds from `since` on to the usage and cost buckets.
    private func applyTotals(of found: ContributionID, sign: Int64, since: Int64 = .min) throws {
        var usage: [(Int64, Int64, Int64, Int64, Int64)] = []
        try storage.connection.query("""
            SELECT timestamp_ms / ? * ?, agent, SUM(tokens_in), SUM(tokens_out), SUM(cache_read) FROM usage_event
            WHERE contribution_id = ? AND timestamp_ms >= ? GROUP BY 1, 2
            """, [.integer(UsageLedger.bucketMilliseconds), .integer(UsageLedger.bucketMilliseconds), .integer(found.id), .integer(since)]) { row in
            usage.append((row.int(0), row.int(1), row.int(2), row.int(3), row.int(4)))
        }
        for (start, agent, tokensIn, tokensOut, cacheRead) in usage {
            try addUsage(start: start, source: found.source, account: found.account, agent: agent,
                         tokensIn: sign * tokensIn, tokensOut: sign * tokensOut, cacheRead: sign * cacheRead)
        }
        var costs: [(String, Int64, String, Int64, Int64)] = []
        try storage.connection.query("""
            SELECT billing, timestamp_ms / ? * ?, currency, SUM(amount_pico), COUNT(*) FROM cost_amount
            WHERE contribution_id = ? AND timestamp_ms >= ? GROUP BY 1, 2, 3
            """, [.integer(UsageLedger.bucketMilliseconds), .integer(UsageLedger.bucketMilliseconds), .integer(found.id), .integer(since)]) { row in
            costs.append((row.text(0) ?? "", row.int(1), row.text(2) ?? "", row.int(3), row.int(4)))
        }
        for (billing, start, currency, amount, events) in costs {
            try addCost(billing: billing, start: start, currency: currency, amount: sign * amount, events: sign * events)
        }
    }

    /// Every contribution key the source has written.
    public func contributions(source: String) throws -> Set<String> {
        var result: Set<String> = []
        try storage.connection.query("SELECT key FROM contribution WHERE source = ?", [.text(source)]) { result.insert($0.text(0) ?? "") }
        return result
    }

    public func appendSamples(_ samples: [QuotaSample], scope: String) throws {
        for sample in samples {
            // Readings keep their exact time, in the date's own representation; cycle boundaries compare against it.
            try storage.connection.run("INSERT OR REPLACE INTO quota_sample (scope, window_id, observed_at, remaining) VALUES (?, ?, ?, ?)",
                [.text(scope), .text(sample.agentId), .real(sample.timestamp.timeIntervalSinceReferenceDate), .real(sample.remainingPct)])
        }
    }

    public func removeSamples(scope: String) throws {
        try storage.connection.run("DELETE FROM quota_sample WHERE scope = ?", [.text(scope)])
    }

    // MARK: Internals

    private struct ContributionID { let id: Int64; let source: String; let account: String; let counted: Bool }

    /// Only billed contributions pay for cost lookups; the rest never had a cost row.
    private func hasCosts(_ contribution: ContributionID) throws -> Bool {
        var found = false
        try storage.connection.query("SELECT 1 FROM cost_amount WHERE contribution_id = ? LIMIT 1", [.integer(contribution.id)]) { _ in found = true }
        return found
    }

    private func contributionID(source: String, key: String, account: String?) throws -> ContributionID {
        try storage.connection.run("INSERT OR IGNORE INTO contribution (source, key, account) VALUES (?, ?, ?)",
                                   [.text(source), .text(key), .text(account ?? "")])
        var found: ContributionID?
        try storage.connection.query("SELECT id, account, counted FROM contribution WHERE source = ? AND key = ?", [.text(source), .text(key)]) { row in
            found = ContributionID(id: row.int(0), source: source, account: row.text(1) ?? "", counted: row.int(2) != 0)
        }
        return found!
    }

    private func write(_ event: UsageLedger.Event, contribution: ContributionID, billed: Bool) throws {
        let key = Self.eventKey(event.key), timestamp = RecordCoding.milliseconds(event.timestamp)
        let agent = try storage.agentID(event.agentId)
        var old: (timestamp: Int64, agent: Int64, tokensIn: Int64, tokensOut: Int64, cacheRead: Int64)?
        try storage.connection.query("""
            SELECT timestamp_ms, agent, tokens_in, tokens_out, cache_read FROM usage_event WHERE contribution_id = ? AND event = ?
            """, [.integer(contribution.id), .integer(key)]) { row in
            old = (row.int(0), row.int(1), row.int(2), row.int(3), row.int(4))
        }
        let values = (timestamp, agent, Int64(event.tokensIn), Int64(event.tokensOut), Int64(event.cacheReadTokens))
        if let old, old == values {
            // Usage is unchanged; costs may still be new for an event first seen without a price.
        } else {
            if let old, contribution.counted {
                try addUsage(start: Self.bucket(old.timestamp), source: contribution.source, account: contribution.account, agent: old.agent,
                             tokensIn: -old.tokensIn, tokensOut: -old.tokensOut, cacheRead: -old.cacheRead)
            }
            try storage.connection.run("""
                INSERT OR REPLACE INTO usage_event (contribution_id, event, timestamp_ms, agent, tokens_in, tokens_out, cache_read)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """, [.integer(contribution.id), .integer(key), .integer(timestamp), .integer(agent),
                      .integer(values.2), .integer(values.3), .integer(values.4)])
            if contribution.counted {
                try addUsage(start: Self.bucket(timestamp), source: contribution.source, account: contribution.account, agent: agent,
                             tokensIn: values.2, tokensOut: values.3, cacheRead: values.4)
            }
        }
        if billed || event.billingID != nil { try writeCosts(event, key: key, timestamp: timestamp, contribution: contribution) }
    }

    private func writeCosts(_ event: UsageLedger.Event, key: Int64, timestamp: Int64, contribution: ContributionID) throws {
        var old: [String: (timestamp: Int64, billing: String, amount: Int64)] = [:]
        try storage.connection.query("""
            SELECT currency, timestamp_ms, billing, amount_pico FROM cost_amount WHERE contribution_id = ? AND event = ?
            """, [.integer(contribution.id), .integer(key)]) { row in
            old[row.text(0) ?? ""] = (row.int(1), row.text(2) ?? "", row.int(3))
        }
        // The empty currency counts every billed event, so a bucket can tell which currencies priced all of them.
        var new: [String: (timestamp: Int64, billing: String, amount: Int64)] = [:]
        if let billing = event.billingID {
            new[""] = (timestamp, billing, 0)
            for (currency, amount) in event.costs ?? [:] where !currency.isEmpty {
                new[currency] = (timestamp, billing, Self.pico(amount))
            }
        }
        for (currency, value) in old where new[currency].map({ $0 != value }) ?? true {
            if contribution.counted {
                try addCost(billing: value.billing, start: Self.bucket(value.timestamp), currency: currency, amount: -value.amount, events: -1)
            }
            try storage.connection.run("DELETE FROM cost_amount WHERE contribution_id = ? AND event = ? AND currency = ?",
                                       [.integer(contribution.id), .integer(key), .text(currency)])
        }
        for (currency, value) in new where old[currency].map({ $0 != value }) ?? true {
            try storage.connection.run("""
                INSERT INTO cost_amount (contribution_id, event, currency, timestamp_ms, billing, amount_pico) VALUES (?, ?, ?, ?, ?, ?)
                """, [.integer(contribution.id), .integer(key), .text(currency), .integer(value.timestamp), .text(value.billing), .integer(value.amount)])
            if contribution.counted {
                try addCost(billing: value.billing, start: Self.bucket(value.timestamp), currency: currency, amount: value.amount, events: 1)
            }
        }
    }

    private func addUsage(start: Int64, source: String, account: String, agent: Int64, tokensIn: Int64, tokensOut: Int64, cacheRead: Int64) throws {
        guard tokensIn != 0 || tokensOut != 0 || cacheRead != 0 else { return }
        try storage.connection.run("""
            INSERT INTO usage_bucket (start_ms, source, account, agent, tokens_in, tokens_out, cache_read) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (start_ms, source, account, agent) DO UPDATE SET tokens_in = tokens_in + excluded.tokens_in,
                tokens_out = tokens_out + excluded.tokens_out, cache_read = cache_read + excluded.cache_read
            """, [.integer(start), .text(source), .text(account), .integer(agent), .integer(tokensIn), .integer(tokensOut), .integer(cacheRead)])
        if tokensIn < 0 || tokensOut < 0 || cacheRead < 0 {
            try storage.connection.run("""
                DELETE FROM usage_bucket WHERE start_ms = ? AND source = ? AND account = ? AND agent = ?
                AND tokens_in = 0 AND tokens_out = 0 AND cache_read = 0
                """, [.integer(start), .text(source), .text(account), .integer(agent)])
        }
    }

    private func addCost(billing: String, start: Int64, currency: String, amount: Int64, events: Int64) throws {
        try storage.connection.run("""
            INSERT INTO cost_bucket (billing, start_ms, currency, amount_pico, events) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (billing, start_ms, currency) DO UPDATE SET amount_pico = amount_pico + excluded.amount_pico,
                events = events + excluded.events
            """, [.text(billing), .integer(start), .text(currency), .integer(amount), .integer(events)])
        if events < 0 {
            try storage.connection.run("DELETE FROM cost_bucket WHERE billing = ? AND start_ms = ? AND currency = ? AND events <= 0",
                                       [.text(billing), .integer(start), .text(currency)])
        }
    }

    static func bucket(_ milliseconds: Int64) -> Int64 {
        milliseconds / UsageLedger.bucketMilliseconds * UsageLedger.bucketMilliseconds
    }

    /// Events older than the retention would be deleted by the next expiry, so they are not written.
    private func cutoff() -> Int64 {
        storage.retention.map { Self.bucket(RecordCoding.milliseconds(Date().addingTimeInterval(-$0))) } ?? .min
    }

    /// Prices are exact decimals with far fewer than twelve fractional digits.
    static func pico(_ amount: Decimal) -> Int64 {
        NSDecimalNumber(decimal: amount * 1_000_000_000_000).int64Value
    }

    static func decimal(_ value: (Int64, Int64)) -> Decimal { Decimal(value.0) / 1_000_000_000_000 }

    /// FNV-1a: stable across launches, unlike `Hasher`.
    static func eventKey(_ key: String) -> Int64 {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in key.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return Int64(bitPattern: hash)
    }

    static func digest(_ events: [UsageLedger.Event], account: String?, since: Date?) -> Int64 {
        var text = (account ?? "") + "\u{4}" + (since.map { String(RecordCoding.milliseconds($0)) } ?? "")
        for event in events {
            text += "\u{1}\(event.key)\u{2}\(RecordCoding.milliseconds(event.timestamp))\u{2}\(event.agentId)\u{2}\(event.tokensIn)"
                + "\u{2}\(event.tokensOut)\u{2}\(event.cacheReadTokens)\u{2}\(event.billingID ?? "")"
            for currency in (event.costs ?? [:]).keys.sorted() { text += "\u{3}\(currency)=\(event.costs![currency]!)" }
        }
        return eventKey(text)
    }
}
