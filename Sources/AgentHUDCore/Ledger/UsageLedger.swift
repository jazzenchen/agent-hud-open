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

        /// A log that left its folder, deleted in its client or moved away, whose usage the ledger keeps: only its group
        /// is stored, so a copy of the same session found later takes its place. It goes when its usage expires.
        public static func gone(group: String?) -> FileState { FileState(signature: goneSignature, group: group) }
        public var isGone: Bool { signature == Self.goneSignature }
        static let goneSignature = "gone"
    }

    /// One token observation after the provider resolved duplicates; `key` is unique within its contribution.
    public struct Event: Hashable, Sendable {
        public let key: String
        public let timestamp: Date
        public let agentId: String
        public let tokensIn: Int
        public let tokensOut: Int
        public let cacheReadTokens: Int
        /// The part of `tokensIn` written to the prompt cache, where the log tells it apart.
        public let cacheWriteTokens: Int
        /// The part of `tokensOut` spent reasoning, where the log tells it apart.
        public let reasoningTokens: Int
        /// The context window the client reported for this call.
        public let contextWindow: Int?
        /// Estimated price by currency; nil for events that belong to no priced account.
        public let costs: [String: Decimal]?
        public let billingID: String?

        public init(key: String, timestamp: Date, agentId: String, tokensIn: Int, tokensOut: Int, cacheReadTokens: Int = 0,
                    cacheWriteTokens: Int = 0, reasoningTokens: Int = 0, contextWindow: Int? = nil,
                    billingID: String? = nil, costs: [String: Decimal]? = nil) {
            self.key = key; self.timestamp = timestamp; self.agentId = agentId
            self.tokensIn = tokensIn; self.tokensOut = tokensOut; self.cacheReadTokens = cacheReadTokens
            self.cacheWriteTokens = min(max(0, cacheWriteTokens), tokensIn); self.reasoningTokens = min(max(0, reasoningTokens), tokensOut)
            self.contextWindow = contextWindow.flatMap { $0 > 0 ? $0 : nil }
            self.billingID = billingID; self.costs = costs
        }
    }

    /// A moment in a session's log that shapes its turns: a prompt starts one, a compaction shrinks the context.
    public struct Mark: Hashable, Codable, Sendable {
        public enum Kind: Int, Codable, Sendable { case prompt = 0, compaction = 1 }
        public let timestamp: Date
        public let kind: Kind
        public init(_ kind: Kind, at timestamp: Date) { self.kind = kind; self.timestamp = timestamp }
    }

    private let storage: LedgerStorage
    private var passOpen = false
    private var expiredAt: Date?
    /// What `recachedBuckets(since:)` last read, by log, and the write mark and generation it was read at.
    private var recachedRead: (mark: Int, generation: Int, byLog: [String: [UsageBucket]])?
    /// Changes when a failed pass rolled back writes that providers may already reflect in memory.
    public private(set) var generation = 0
    /// Grows with every change to a contribution; `changedKeys(after:)` names what changed since a mark taken earlier.
    public var writeMark: Int { storage.writes }

    /// The contribution keys changed since `mark`, as far back as this run of the ledger.
    public func changedKeys(after mark: Int) -> Set<String> {
        Set(storage.touched.lazy.filter { $0.value > mark }.map(\.key))
    }

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

    /// Moves the usage of the consumers keyed in `moves` to the ids they map to, adding it to usage recorded there.
    /// Writes only when the ledger knows one of them.
    func moveConsumers(_ moves: [String: String]) throws {
        let known = moves.filter { $0.key != $0.value && storage.knownAgentID($0.key) != nil }
        guard !known.isEmpty else { return }
        _ = try write { writer in
            for (old, new) in known.sorted(by: { $0.key < $1.key }) { try writer.moveConsumer(old, to: new) }
        }
    }

    /// Deletes rows that left the retention window, aligned to a bucket so no bucket keeps half its events.
    public func expire(now: Date) throws {
        let cutoff = LedgerWriter.bucket(RecordCoding.milliseconds(now.addingTimeInterval(-Self.retention)))
        let samples = now.addingTimeInterval(-QuotaHistoryStore.retention).timeIntervalSinceReferenceDate
        let balances = RecordCoding.milliseconds(now.addingTimeInterval(-BalanceTrend.lookback))
        _ = try write { writer in
            let connection = writer.storage.connection
            try connection.run("DELETE FROM usage_event WHERE timestamp_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM usage_bucket WHERE start_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM cost_amount WHERE timestamp_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM cost_bucket WHERE start_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM session_mark WHERE at_ms < ?", [.integer(cutoff)])
            try connection.run("DELETE FROM quota_sample WHERE observed_at < ?", [.real(samples)])
            try connection.run("DELETE FROM balance_sample WHERE observed_ms < ?", [.integer(balances)])
            try connection.run("""
                DELETE FROM contribution WHERE NOT EXISTS (SELECT 1 FROM usage_event WHERE contribution_id = contribution.id)
                AND NOT EXISTS (SELECT 1 FROM cost_amount WHERE contribution_id = contribution.id)
                AND NOT EXISTS (SELECT 1 FROM session_mark WHERE contribution_id = contribution.id)
                """)
            try connection.run("""
                DELETE FROM source_file WHERE signature = ?
                AND NOT EXISTS (SELECT 1 FROM contribution WHERE contribution.source = source_file.source AND contribution.key = source_file.path)
                """, [.text(FileState.goneSignature)])
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
            SELECT start_ms, account, agent, SUM(tokens_in), SUM(tokens_out), SUM(cache_read), SUM(cache_write), SUM(reasoning)
            FROM usage_bucket WHERE start_ms >= ? AND (? IS NULL OR source = ?) GROUP BY start_ms, account, agent
            """, [.integer(start), .nullable(source), .nullable(source)]) { row in
            result.append(UsageBucket(start: RecordCoding.date(row.int(0)), agentId: storage.agentName(row.int(2)),
                tokensIn: Int(row.int(3)), tokensOut: Int(row.int(4)), cacheReadTokens: Int(row.int(5)),
                cacheWriteTokens: Int(row.int(6)), reasoningTokens: Int(row.int(7)),
                account: row.text(1).flatMap { $0.isEmpty ? nil : $0 }))
        }
        return result.sorted { ($0.start, $0.account ?? "", $0.agentId) < ($1.start, $1.account ?? "", $1.agentId) }
    }

    /// Every model's tokens in each period ending at `now`, counted from the buckets as `buckets(since:)` returns them,
    /// with the part in DeepSeek's peak hours (`ModelCatalog.isPeak`, weekdays 9–12 and 14–18 Beijing time) apart.
    public func periods(endingAt now: Date, calendar: Calendar = .current) throws -> UsagePeriods {
        var periods = UsagePeriods()
        for period in UsagePeriods.Period.allCases {
            let start = RecordCoding.milliseconds(period.start(endingAt: now, calendar: calendar)) / Self.bucketMilliseconds * Self.bucketMilliseconds
            try storage.connection.query("""
                SELECT agent, SUM(tokens_in), SUM(tokens_out), SUM(cache_read), SUM(cache_write), SUM(reasoning),
                       CAST(strftime('%w', start_ms / 1000, 'unixepoch', '+8 hours') AS INTEGER) BETWEEN 1 AND 5
                       AND CAST(strftime('%H', start_ms / 1000, 'unixepoch', '+8 hours') AS INTEGER) IN (9, 10, 11, 14, 15, 16, 17) AS peak
                FROM usage_bucket WHERE start_ms >= ? GROUP BY agent, peak
                """, [.integer(start)]) { row in
                let agent = storage.agentName(row.int(0))
                let kinds = TokenKinds(tokensIn: Int(row.int(1)), tokensOut: Int(row.int(2)), cacheRead: Int(row.int(3)),
                                       cacheWrite: Int(row.int(4)), reasoning: Int(row.int(5)))
                periods.tokens[period, default: [:]][agent, default: TokenKinds()] += kinds
                if row.int(6) == 1, ModelCatalog.model(for: agent)?.peakHours == true {
                    periods.peak[period, default: [:]][agent, default: TokenKinds()] += kinds
                }
            }
        }
        return periods
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

    /// Prompt tokens sent again because the cache no longer held them, in the periods of `since` and later, by 15-minute
    /// period and the model of the call that sent them: cache writes where that call wrote to the cache, input otherwise.
    /// Every log that marks its prompts counts, whether or not a report lists its session, by the rule a session's turns
    /// count them (`SessionUsageBuilder.recached`). Logs are read again only once the ledger wrote to them.
    public func recachedBuckets(since: Date) throws -> [UsageBucket] {
        let mark = storage.writes
        var byLog: [String: [UsageBucket]]
        if let read = recachedRead, read.generation == generation {
            byLog = read.byLog
            let changed = changedKeys(after: read.mark)
            if !changed.isEmpty {
                for key in changed { byLog[key] = nil }
                byLog.merge(try readRecached(logs: changed), uniquingKeysWith: +)
            }
        } else {
            byLog = try readRecached(logs: nil)
        }
        recachedRead = (mark, generation, byLog)
        let start = RecordCoding.date(RecordCoding.milliseconds(since) / Self.bucketMilliseconds * Self.bucketMilliseconds)
        var sums: [Date: [String: (sent: Int, written: Int)]] = [:]
        for bucket in byLog.values.joined() where bucket.start >= start {
            var sum = sums[bucket.start, default: [:]][bucket.agentId] ?? (0, 0)
            sum.sent += bucket.tokensIn; sum.written += bucket.cacheWriteTokens
            sums[bucket.start, default: [:]][bucket.agentId] = sum
        }
        return sums.keys.sorted().flatMap { start in
            sums[start]!.keys.sorted().map { agentId in
                let sum = sums[start]![agentId]!
                return UsageBucket(start: start, agentId: agentId, tokensIn: sum.sent, tokensOut: 0, cacheWriteTokens: sum.written)
            }
        }
    }

    /// Each call in a counted log that marks its prompts, against the call before it in that log; nil `logs` reads every log.
    private func readRecached(logs: Set<String>?) throws -> [String: [UsageBucket]] {
        var result: [String: [UsageBucket]] = [:]
        var keys = SQLiteConnection.Value.null
        if let logs { keys = .text(String(decoding: try JSONEncoder().encode(logs.sorted()), as: UTF8.self)) }
        try storage.connection.query("""
            SELECT c.key, r.start_ms, r.agent, r.wrote, SUM(r.tokens_in) FROM (
                SELECT e.contribution_id, e.timestamp_ms / 900000 * 900000 AS start_ms, e.agent, e.tokens_in, e.cache_read,
                       e.cache_write > 0 AS wrote,
                       LAG(e.tokens_in + e.cache_read) OVER (PARTITION BY e.contribution_id ORDER BY e.timestamp_ms, e.event) AS previous
                FROM usage_event e WHERE e.contribution_id IN (
                    SELECT c.id FROM contribution c WHERE c.counted = 1
                    AND (?1 IS NULL OR c.key IN (SELECT value FROM json_each(?1)))
                    AND EXISTS (SELECT 1 FROM session_mark m WHERE m.contribution_id = c.id AND m.kind = 0))
            ) r JOIN contribution c ON c.id = r.contribution_id
            WHERE r.previous IS NOT NULL AND r.cache_read * 2 < r.previous AND r.tokens_in * 2 >= r.previous
            GROUP BY c.key, r.start_ms, r.agent, r.wrote
            """, [keys]) { row in
            let sent = Int(row.int(4))
            result[row.text(0) ?? "", default: []].append(UsageBucket(start: RecordCoding.date(row.int(1)), agentId: storage.agentName(row.int(2)),
                tokensIn: sent, tokensOut: 0, cacheWriteTokens: row.int(3) == 1 ? sent : 0))
        }
        return result
    }

    /// What each requested session spent, by model, 15-minute period and turn. Sessions without recorded events are left out.
    public func sessionUsage(_ requests: [SessionUsageRequest]) throws -> [String: SessionUsage] {
        var result: [String: SessionUsage] = [:]
        for request in requests {
            let (own, callLog, subagents) = try contributions(of: request)
            let ids = own + subagents
            guard !ids.isEmpty else { continue }
            // Only the session's own log marks its turns; a sub-agent's prompts are steps of the turn that started it.
            var prompts: [Date] = [], compactions: [Date] = []
            if !own.isEmpty {
                try storage.connection.query("""
                    SELECT at_ms, kind FROM session_mark WHERE contribution_id IN (SELECT value FROM json_each(?))
                    """, [Self.idList(own)]) { row in
                    switch UsageLedger.Mark.Kind(rawValue: Int(row.int(1))) {
                    case .prompt: prompts.append(RecordCoding.date(row.int(0)))
                    case .compaction: compactions.append(RecordCoding.date(row.int(0)))
                    case nil: break
                    }
                }
            }
            var builder = SessionUsageBuilder(prompts: prompts, compactions: compactions)
            var latestAgent: Int64?
            try storage.connection.query("""
                SELECT contribution_id, timestamp_ms, agent, tokens_in, tokens_out, cache_read, cache_write, reasoning, context_window
                FROM usage_event WHERE contribution_id IN (SELECT value FROM json_each(?)) ORDER BY timestamp_ms
                """, [Self.idList(ids)]) { row in
                let contribution = row.int(0), isOwn = !subagents.contains(contribution)
                if isOwn { latestAgent = row.int(2) }
                builder.add(.init(timestamp: RecordCoding.date(row.int(1)), agentId: storage.agentName(row.int(2)),
                    tokens: .init(tokensIn: Int(row.int(3)), tokensOut: Int(row.int(4)), cacheReadTokens: Int(row.int(5)),
                                  cacheWriteTokens: Int(row.int(6)), reasoningTokens: Int(row.int(7))),
                    own: isOwn, callLog: callLog.contains(contribution), contextWindow: row.int(8) > 0 ? Int(row.int(8)) : nil))
            }
            guard !builder.isEmpty else { continue }
            var priced: Int64 = 0, amounts: [String: (Int64, Int64)] = [:]
            try storage.connection.query("""
                SELECT currency, SUM(amount_pico), COUNT(*) FROM cost_amount
                WHERE contribution_id IN (SELECT value FROM json_each(?)) GROUP BY currency
                """, [Self.idList(ids)]) { row in
                let currency = row.text(0) ?? ""
                if currency.isEmpty { priced = row.int(2) } else { amounts[currency] = (row.int(1), row.int(2)) }
            }
            let costs = amounts.filter { $0.value.1 == priced }.mapValues(LedgerWriter.decimal)
            let largest = latestAgent.flatMap { storage.largestContext[$0] }.map(Int.init)
            result[request.sessionID] = builder.build(costs: priced > 0 && !costs.isEmpty ? costs : nil) { agentId, reported in
                ModelCatalog.contextWindow(agentId: agentId, reported: reported, largestSeen: largest)
            }
        }
        return result
    }

    /// One turn's calls, oldest first: every call of the session and its sub-agents from `start` through `end`, the span
    /// a turn of `sessionUsage` covers.
    public func turnCalls(_ request: SessionUsageRequest, from start: Date, through end: Date) throws -> [TurnCall] {
        let (own, callLog, subagents) = try contributions(of: request)
        let ids = own + subagents
        guard !ids.isEmpty else { return [] }
        let from = RecordCoding.milliseconds(start), through = RecordCoding.milliseconds(end)
        // The call before the turn tells whether its first call found the prompt in the cache.
        var previous: Int?
        if !callLog.isEmpty {
            try storage.connection.query("""
                SELECT tokens_in + cache_read FROM usage_event WHERE contribution_id IN (SELECT value FROM json_each(?)) AND timestamp_ms < ?
                ORDER BY timestamp_ms DESC LIMIT 1
                """, [Self.idList(Array(callLog)), .integer(from)]) { previous = Int($0.int(0)) }
        }
        var calls: [TurnCall] = []
        try storage.connection.query("""
            SELECT e.contribution_id, c.source, c.key, e.timestamp_ms, e.agent, e.tokens_in, e.tokens_out, e.cache_read, e.cache_write,
                   e.reasoning
            FROM usage_event e JOIN contribution c ON c.id = e.contribution_id
            WHERE e.contribution_id IN (SELECT value FROM json_each(?)) AND e.timestamp_ms BETWEEN ? AND ? ORDER BY e.timestamp_ms
            """, [Self.idList(ids), .integer(from), .integer(through)]) { row in
            let contribution = row.int(0), agentId = storage.agentName(row.int(4))
            let tokens = SessionUsage.Tokens(tokensIn: Int(row.int(5)), tokensOut: Int(row.int(6)), cacheReadTokens: Int(row.int(7)),
                                             cacheWriteTokens: Int(row.int(8)), reasoningTokens: Int(row.int(9)))
            var context: Int?, recached: Int?
            if callLog.contains(contribution) {
                let sent = SessionUsageBuilder.recached(tokens, after: previous)
                recached = sent > 0 ? sent : nil
                context = tokens.tokensIn + tokens.cacheReadTokens
                previous = context
            }
            calls.append(TurnCall(timestamp: RecordCoding.date(row.int(3)), agentId: agentId, tokens: tokens, own: !subagents.contains(contribution),
                                  log: row.text(2) ?? "", source: row.text(1) ?? "", context: context, recached: recached,
                                  listCost: ModelCatalog.cost(agentId: agentId, kinds: tokens.kinds)?.amount))
        }
        return calls
    }

    /// The contributions that hold a session: its own logs, among them the one that records every call, and its sub-agents' logs.
    private func contributions(of request: SessionUsageRequest) throws -> (own: [Int64], callLog: Set<Int64>, subagents: Set<Int64>) {
        var own: [Int64] = [], callLog: Set<Int64> = [], subagents: Set<Int64> = []
        for key in Set(request.keys) {
            try storage.connection.query("SELECT id FROM contribution WHERE key = ?", [.text(key)]) { row in
                own.append(row.int(0))
                if key == request.callLog { callLog.insert(row.int(0)) }
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
        for key in Set(request.subagentKeys) {
            try storage.connection.query("SELECT id FROM contribution WHERE key = ?", [.text(key)]) { subagents.insert($0.int(0)) }
        }
        return (own, callLog, subagents)
    }

    /// Row ids as one JSON parameter for `IN (SELECT value FROM json_each(?))`. The connection keeps every statement it
    /// prepares, by its text, for as long as it lives; one text serves any number of ids, where an `IN (?, …)` list
    /// would add statements for every length a session with a growing number of sub-agents passes through.
    private static func idList(_ ids: some Sequence<Int64>) -> SQLiteConnection.Value {
        .text("[" + ids.map(String.init).joined(separator: ",") + "]")
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

    /// A billing account's readings of its balance in `currency` from `since` on, oldest first. The ledger keeps a day of them.
    public func balanceSamples(billing: String, currency: String, since: Date) throws -> [BalanceSample] {
        var result: [BalanceSample] = []
        try storage.connection.query("""
            SELECT observed_ms, amount_pico FROM balance_sample WHERE billing = ? AND currency = ? AND observed_ms >= ? ORDER BY observed_ms
            """, [.text(billing), .text(currency), .integer(RecordCoding.milliseconds(since))]) { row in
            result.append(BalanceSample(timestamp: RecordCoding.date(row.int(0)), amount: Decimal(row.int(1)) / 1_000_000_000_000))
        }
        return result
    }

    public func sampleCount(scope: String) throws -> Int {
        var count = 0
        try storage.connection.query("SELECT COUNT(*) FROM quota_sample WHERE scope = ?", [.text(scope)]) { count = Int($0.int(0)) }
        return count
    }
}
