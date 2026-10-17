import AgentHUDSupport
import Foundation

/// Files parsed whole into sessions, for clients that keep databases or rewrite their logs. A poll lists every listing,
/// parses newest first each file whose modification time or size changed, or a related file's, while its time budget
/// lasts, and keeps each parse until the file changes or leaves the reading window. A file whose parse failed keeps its
/// last parse and is parsed again only once its pause is over (`FailedReads`). The client merges the parses into
/// sessions, which reach the ledger through `SessionLedger`.
struct WholeFileStore<Parsed: Sendable> {
    struct Listing {
        /// The notice key of the listing's problems.
        let name: String
        let files: LogFiles
        /// Files whose changes also invalidate a parse, such as a SQLite WAL.
        var related: (URL) -> [URL] = { _ in [] }
        /// The listing's files are parsed and merged before other listings' files, whatever their modification times.
        var leads = false
        /// Parses a file for a report reading from the given date.
        let parse: (URL, Date) throws -> Parsed
    }

    struct Pass {
        /// Parses of the files modified since the cutoff, in reading order.
        var files: [(path: String, parsed: Parsed)] = []
        /// Every file the listings found.
        var listed: Set<String> = []
        var indexing: IndexProgress?
        /// Listings that stopped at their limit, found unreadable entries or could not parse a file.
        var notices: [String: String] = [:]
        /// Changes whenever the parses change.
        var revision = 0

        func listedFiles(_ ids: (Parsed) -> [String]) -> ListedFiles {
            ListedFiles(paths: listed, sessions: Dictionary(uniqueKeysWithValues: files.map { ($0.path, ids($0.parsed)) }))
        }
    }

    static var timeBudget: TimeInterval { 1.5 }

    private let listings: [Listing]
    private var cache: [String: (signature: String, parsed: Parsed)] = [:]
    private var failures = FailedReads()
    private var revision = 0

    init(listings: [Listing]) {
        self.listings = listings
    }

    /// Paths from the collector's watch, as `LogFiles.noteChanges` takes them: after the first listing, a poll looks
    /// only at them.
    func noteChanges(_ paths: Set<String>?) {
        for listing in listings { listing.files.noteChanges(paths) }
    }

    mutating func index(since: Date) -> Pass {
        let started = Date()
        var pass = Pass(), seen = Set<String>()
        var candidates: [(path: String, listing: Int, signature: String, modified: Date)] = []
        for (index, listing) in listings.enumerated() {
            let gaps = listing.files.refresh(now: started)
            if gaps.truncated { pass.notices[listing.name] = ProviderFailure.limit.message }
            else if gaps.unreadable > 0 { pass.notices[listing.name] = ProviderFailure.local.message }
            for (path, file) in listing.files.files {
                pass.listed.insert(path)
                var modified = file.modified, signature = "\(file.modified.timeIntervalSince1970):\(file.size)"
                for sibling in listing.related(URL(fileURLWithPath: path)) {
                    if let values = try? sibling.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey]), let date = values.contentModificationDate {
                        modified = max(modified, date); signature += ":\(date.timeIntervalSince1970):\(values.fileSize ?? 0)"
                    }
                }
                guard modified >= since, seen.insert(path).inserted else { continue }
                candidates.append((path, index, signature, modified))
            }
        }
        candidates.sort { a, b in
            let (leadsA, leadsB) = (listings[a.listing].leads, listings[b.listing].leads)
            return leadsA != leadsB ? leadsA : a.modified > b.modified
        }
        var pending = 0, loaded = 0
        for candidate in candidates {
            if cache[candidate.path]?.signature == candidate.signature { continue }
            let listing = listings[candidate.listing]
            if failures.isPausing(candidate.path, at: started) {
                pass.notices[listing.name] = ProviderFailure.local.message
                continue
            }
            if loaded > 0 && Date().timeIntervalSince(started) >= Self.timeBudget { pending += 1; continue }
            loaded += 1
            do {
                try Task.checkCancellation()
                cache[candidate.path] = (candidate.signature, try listing.parse(URL(fileURLWithPath: candidate.path), since))
                failures.succeeded(candidate.path)
                revision += 1
            } catch {
                if !Task.isCancelled { failures.failed(candidate.path, at: Date()) }
                pass.notices[listing.name] = ProviderFailure.local.message
            }
        }
        failures.keep(seen)
        if cache.keys.contains(where: { !seen.contains($0) }) {
            cache = cache.filter { seen.contains($0.key) }
            revision += 1
        }
        pass.files = candidates.compactMap { candidate in cache[candidate.path].map { (candidate.path, $0.parsed) } }
        pass.indexing = pending > 0 ? IndexProgress(done: candidates.count - pending, total: candidates.count) : nil
        pass.revision = revision
        return pass
    }
}

/// The files a whole-file reader listed, and the sessions of those it holds a parse of.
struct ListedFiles: Sendable {
    var paths: Set<String> = []
    var sessions: [String: [String]] = [:]
}

/// The ledger write of readers that parse whole files or account records: one contribution per session, replaced from
/// the start of the reading window once the index is complete and something changed. The sessions of a stored file
/// that is missing from the listing, deleted in its client, keep their usage in the ledger until it expires; a file that
/// holds one of them later replaces it as any read does. After a rolled-back pass the stored files are read again and
/// every session is written again.
final class SessionLedger {
    let source: String
    private let ledger: UsageLedger
    /// The sessions each stored file held, and the running totals of their rows.
    private var stored: [String: FileRecord] = [:]
    private var loadedGeneration: Int?
    private var recorded: (revision: Int, account: String?, window: Date)?

    init(source: String, ledger: UsageLedger) {
        self.source = source
        self.ledger = ledger
    }

    /// - revision: changes whenever the sessions change; nil when the reader cannot tell, so every call writes.
    /// - runningTotals: each event is its row's total so far, which the client adds to in place: the ledger keeps the
    ///   total the row had when it was first read at the row's own time, and dates what it gains later at the 15-minute
    ///   period `now` falls in. Only sessions the listed files hold can count this way.
    func record(files: ListedFiles?, revision: Int?, account: String? = nil, window: Date, runningTotals: Bool = false,
                now: Date = Date(), sessions: () -> [(id: String, events: [UsageEvent])],
                isolation: isolated (any Actor)? = #isolation) async {
        let generation = await ledger.generation
        if loadedGeneration != generation {
            stored = ((try? await ledger.fileStates(source: source)) ?? [:]).compactMapValues(Self.record)
            recorded = nil
            loadedGeneration = generation
        }
        let unchanged = revision != nil && recorded.map { $0.revision == revision && $0.account == account && $0.window == window } == true
        let missing = files.map { files in stored.keys.filter { !files.paths.contains($0) } } ?? []
        if unchanged && missing.isEmpty { return }

        var current = sessions()
        var next = stored, changed: Set<String> = [], firstRead: [String: [UsageLedger.Event]] = [:]
        if !unchanged {
            for (path, ids) in files?.sessions ?? [:] {
                let held = Array(Set(ids)).sorted()
                if held != stored[path]?.sessions ?? [] { changed.insert(path); next[path, default: FileRecord()].sessions = held }
            }
            if runningTotals, let files {
                let totalled = Self.totals(current, files: files, stored: next, window: window, now: now)
                current = totalled.sessions
                firstRead = totalled.firstRead
                for (path, totals) in totalled.totals where totals != next[path]?.totals ?? [:] {
                    changed.insert(path)
                    next[path, default: FileRecord()].totals = totals
                }
            }
        }
        let contributions = unchanged ? [:] : SessionContributions.canonical(current)
        for path in missing { next[path] = nil }
        let source = source, writes = changed.map { ($0, next[$0]) }, firstReads = firstRead
        do {
            try await ledger.write { writer in
                for (session, events) in contributions {
                    try writer.replace(source: source, contribution: session, account: account, events: events, since: window)
                }
                // A row first read with a time before the window is not replaced, so it is written once here.
                for (session, events) in firstReads {
                    try writer.upsert(source: source, contribution: session, account: account, events: events)
                }
                for (path, record) in writes {
                    if let record, !record.isEmpty { try writer.setFile(source: source, path: path, state: Self.state(record)) }
                    else { try writer.removeFile(source: source, path: path) }
                }
                for path in missing { try writer.removeFile(source: source, path: path) }
            }
            stored = next.filter { !$0.value.isEmpty }
            if !unchanged { recorded = revision.map { ($0, account, window) } }
        } catch { /* The next poll writes the same sessions again. */ }
    }

    /// One row of a reader that counts in running totals, as far as the ledger holds it.
    struct RunningTotal: Codable, Equatable {
        /// The row's counts when it was first read, kept at the row's own time.
        var first: [Int]
        var firstAt: Date
        /// Everything the ledger holds for the row, its first counts and its growth together.
        var total: [Int]
        /// Growth by the start of the 15-minute period it was read in, in milliseconds, while the period is inside the
        /// window; growth before the window stays in the ledger as it was written.
        var growth: [String: [Int]] = [:]
        /// When the row was first read or last grew; a row that has not for longer than the ledger keeps usage is forgotten.
        var changedAt: Date
    }

    /// Running totals turned into what the ledger keeps: each session's rows as their first counts and their growth, the
    /// totals of each file's sessions, and the first counts of rows first read now whose time is before the window.
    static func totals(_ sessions: [(id: String, events: [UsageEvent])], files: ListedFiles, stored: [String: FileRecord],
                       window: Date, now: Date) -> (sessions: [(id: String, events: [UsageEvent])], totals: [String: [String: [String: RunningTotal]]],
                                                    firstRead: [String: [UsageLedger.Event]]) {
        var file: [String: String] = [:]
        for (path, ids) in files.sessions { for id in ids { file[id] = path } }
        // Rows of sessions no longer read keep their totals while the ledger can still hold what they grew by.
        var totals = stored.mapValues { $0.totals.compactMapValues { rows in
            let kept = rows.filter { now.timeIntervalSince($0.value.changedAt) < UsageLedger.retention }
            return kept.isEmpty ? nil : kept
        } }
        var firstRead: [String: [UsageLedger.Event]] = [:]
        let period = String(RecordCoding.milliseconds(now) / UsageLedger.bucketMilliseconds * UsageLedger.bucketMilliseconds)
        let result = sessions.map { session -> (id: String, events: [UsageEvent]) in
            guard let path = file[session.id] else { return session }
            var rows = totals[path]?[session.id] ?? [:]
            let events = session.events.flatMap { event -> [UsageEvent] in
                guard let id = event.eventID else { return [event] }
                let counts = [event.tokensIn, event.tokensOut, event.cacheReadTokens, event.cacheWriteTokens, event.reasoningTokens]
                var row = rows[id] ?? RunningTotal(first: counts, firstAt: event.timestamp, total: counts, changedAt: now)
                if rows[id] == nil, event.timestamp < window {
                    firstRead[session.id, default: []].append(UsageLedger.Event(key: id, timestamp: event.timestamp, agentId: event.agentId,
                        tokensIn: counts[0], tokensOut: counts[1], cacheReadTokens: counts[2], cacheWriteTokens: counts[3], reasoningTokens: counts[4]))
                }
                let gained = zip(counts, row.total).map { max(0, $0 - $1) }
                if gained.contains(where: { $0 > 0 }) {
                    row.growth[period] = zip(row.growth[period] ?? [0, 0, 0, 0, 0], gained).map { $0 + $1 }
                    row.total = zip(row.total, counts).map { max($0, $1) }
                    row.changedAt = now
                }
                row.growth = row.growth.filter { (Int64($0.key) ?? 0) >= RecordCoding.milliseconds(window) }
                rows[id] = row
                func part(_ counts: [Int], at date: Date, id: String) -> UsageEvent {
                    UsageEvent(timestamp: date, agentId: event.agentId, tokensIn: counts[0], tokensOut: counts[1], cacheReadTokens: counts[2],
                               cacheWriteTokens: counts[3], reasoningTokens: counts[4], eventID: id, origin: event.origin, attribution: event.attribution)
                }
                return [part(row.first, at: row.firstAt, id: id)] + row.growth.keys.sorted().map { start in
                    part(row.growth[start]!, at: RecordCoding.date(Int64(start) ?? 0), id: id + "@" + start)
                }
            }
            totals[path, default: [:]][session.id] = rows
            return (session.id, events)
        }
        return (result, totals, firstRead)
    }

    /// What the ledger keeps of one file: the sessions it held, and the running totals of their rows by session and event.
    struct FileRecord: Codable, Equatable {
        var sessions: [String] = []
        var totals: [String: [String: RunningTotal]] = [:]
        var isEmpty: Bool { sessions.isEmpty && totals.isEmpty }
    }

    private struct StoredState: Codable {
        static let version = 1
        let version: Int
        let sessions: [String]
        var totals: [String: [String: RunningTotal]]?
    }

    private static func state(_ record: FileRecord) -> UsageLedger.FileState {
        UsageLedger.FileState(signature: "", state: try? JSONEncoder().encode(StoredState(version: StoredState.version, sessions: record.sessions,
                                                                                       totals: record.totals.isEmpty ? nil : record.totals)))
    }

    private static func record(_ file: UsageLedger.FileState) -> FileRecord? {
        guard let data = file.state, let state = try? JSONDecoder().decode(StoredState.self, from: data),
              state.version == StoredState.version else { return nil }
        return FileRecord(sessions: state.sessions, totals: state.totals ?? [:])
    }
}
