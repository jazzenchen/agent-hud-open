import Foundation

/// An append-only log format: what a client supplies to `TailLogStore`.
protocol TailLog {
    /// What the store keeps of one log between reads, such as counters and turn state; never conversation text.
    associatedtype Summary: Codable & Sendable
    /// The ledger source that owns the logs' contributions and stored states.
    static var source: String { get }
    /// The summary's key in the stored JSON state, and that state's version; a state of another version is read again
    /// from its log.
    static var summaryKey: String { get }
    static var version: Int { get }
    static func summary(for url: URL) -> Summary
    /// The decoded log, for a format that cannot be read from an offset; nil reads the file from where the last read stopped.
    static func contents(of url: URL) async throws -> Data?
    /// Reads one or more complete lines, each ending in a newline, and returns the usage they added.
    static func ingest(_ lines: Data, into summary: inout Summary) throws -> [UsageLedger.Event]
    /// The prompts and compactions read since the last call.
    static func drainMarks(_ summary: inout Summary) -> [UsageLedger.Mark]
    /// Runs once a read of the log ends.
    static func finishRead(_ summary: inout Summary, now: Date)
    /// The session of a log that can be copied to several places, where only the newest copy counts; "" for such a log
    /// without a session. Nil for a format whose logs all count.
    static func group(_ summary: Summary) -> String?
}

extension TailLog {
    static func contents(of url: URL) async throws -> Data? { nil }
    static func drainMarks(_ summary: inout Summary) -> [UsageLedger.Mark] { [] }
    static func finishRead(_ summary: inout Summary, now: Date) {}
    static func group(_ summary: Summary) -> String? { nil }
}

/// Reads append-only logs into the usage ledger, one contribution per log. A poll lists the logs, reads what changed
/// newest first from each saved position while its time budget lasts, and writes usage, positions and summaries in one
/// ledger write; a partially written final line is read once it is complete. A log that shrank, or changed without
/// growing, is read again from the start and replaces what it recorded. A stored log missing from the listing leaves the
/// ledger; one that is listed keeps its contribution even when it cannot be read.
final class TailLogStore<Log: TailLog> {
    struct Entry {
        var modified: Date
        var size: Int
        /// Just past the last line read.
        var offset = 0
        /// Just past the last complete line the last read saw; beyond `offset` while complete lines wait.
        var committed = 0
        var summary: Log.Summary
    }

    /// One poll's work.
    struct Pass {
        /// Logs modified since the cutoff.
        var logs: [(path: String, file: LogFiles.File)] = []
        /// Logs whose entries this pass changed or dropped; after a rolled-back pass every entry was reloaded.
        var changed: [String] = [], removed: [String] = [], reloaded = false
        /// Logs the budget did not reach or finish, and changes the ledger did not take.
        var pending = 0
        /// Logs that could not be read; they are read again on the next poll.
        var failures: [any Error] = []
        var gaps = LogFiles.Gaps()
        var filesRead = 0, bytesRead = 0
    }

    static var chunkSize: Int { 4 << 20 }

    private let ledger: UsageLedger
    private let files: LogFiles
    private var entries: [String: Entry] = [:]
    /// Stored states not decoded yet; most logs are older than the cutoff and never need it.
    private var stored: [String: UsageLedger.FileState] = [:]
    /// Sessions and modification times of grouped logs, for choosing the copy that counts.
    private var groups: [String: String] = [:]
    private var modified: [String: Date] = [:]
    private var loadedGeneration: Int?

    /// - watchesChanges: after the first listing, polls look only at logs a directory watch reports changed.
    init(roots: [URL], ledger: UsageLedger, watchesChanges: Bool, accepts: @escaping (URL) -> Bool) {
        self.ledger = ledger
        files = LogFiles(roots: roots, watchesChanges: watchesChanges, accepts: accepts)
    }

    func entry(_ path: String) -> Entry? {
        if let entry = entries[path] { return entry }
        guard let file = stored.removeValue(forKey: path), let entry = Self.entry(file) else { return nil }
        entries[path] = entry
        return entry
    }

    func index(since cutoff: Date, timeBudget: TimeInterval, isolation: isolated (any Actor)? = #isolation) async -> Pass {
        var pass = Pass()
        // Stored states are read once, and again after a failed pass rolled back what this store had written.
        let generation = await ledger.generation
        if loadedGeneration != generation {
            stored = (try? await ledger.fileStates(source: Log.source)) ?? [:]
            entries = [:]
            groups = stored.compactMapValues(\.group)
            modified = stored.compactMapValues { $0.group == nil ? nil : LedgerCopies.signature($0.signature)?.modified }
            loadedGeneration = generation
            pass.reloaded = true
        }
        let started = Date(), deadline = started.addingTimeInterval(timeBudget)
        pass.gaps = files.refresh(now: started)
        var changing: [(path: String, file: LogFiles.File)] = []
        for (path, file) in files.files where file.modified >= cutoff {
            pass.logs.append((path, file))
            if let entry = entry(path), entry.size == file.size, LedgerCopies.same(entry.modified, file.modified), entry.offset >= entry.committed { continue }
            changing.append((path, file))
        }

        // Always make progress on the newest log, then keep going while the budget lasts.
        var updates: [String: (entry: Entry, events: [UsageLedger.Event], marks: [UsageLedger.Mark], reset: Bool)] = [:]
        for (index, log) in changing.sorted(by: { $0.file.modified > $1.file.modified }).enumerated() {
            if index > 0, Date() >= deadline {
                pass.pending += changing.count - index
                break
            }
            do {
                let update = try await read(log.path, file: log.file, deadline: deadline, pass: &pass)
                updates[log.path] = update
                if update.entry.offset < update.entry.committed { pass.pending += 1 }
            } catch {
                pass.failures.append(error)
            }
        }

        let removed = Set(entries.keys).union(stored.keys).filter { files.files[$0] == nil }
        guard !updates.isEmpty || !removed.isEmpty else { return pass }
        var nextGroups = groups, nextModified = modified
        for (path, update) in updates {
            nextGroups[path] = Log.group(update.entry.summary)
            nextModified[path] = nextGroups[path] == nil ? nil : update.entry.modified
        }
        for path in removed { nextGroups[path] = nil; nextModified[path] = nil }
        let counted = LedgerCopies.counted(touched: Set(updates.keys).union(removed), previous: groups, members: nextGroups, modified: nextModified)
        let writes = updates.map { path, update in
            (path: path, reset: update.reset, events: update.events, marks: update.marks,
             counted: nextGroups[path].map { _ in counted[path] ?? false }, state: Self.state(update.entry))
        }
        let source = Log.source, written = Set(updates.keys)
        do {
            try await ledger.write { writer in
                for write in writes {
                    if write.reset { try writer.remove(source: source, contribution: write.path) }
                    try writer.upsert(source: source, contribution: write.path, counted: write.counted, events: write.events)
                    try writer.addMarks(source: source, contribution: write.path, marks: write.marks)
                    try writer.setFile(source: source, path: write.path, state: write.state)
                }
                for path in removed {
                    try writer.remove(source: source, contribution: path)
                    try writer.removeFile(source: source, path: path)
                }
                for (path, value) in counted where !written.contains(path) {
                    try writer.setCounted(source: source, contribution: path, counted: value)
                }
            }
            for (path, update) in updates { entries[path] = update.entry }
            for path in removed { entries[path] = nil; stored[path] = nil }
            groups = nextGroups
            modified = nextModified
            pass.changed = Array(updates.keys)
            pass.removed = Array(removed)
        } catch {
            // Positions stay where the ledger has them, so the next poll reads the same bytes again.
            pass.pending += updates.count
        }
        return pass
    }

    /// Reads what the log gained since its entry, or all of it once rewritten. Each read ingests at least one chunk.
    private func read(_ path: String, file: LogFiles.File, deadline: Date, pass: inout Pass,
                      isolation: isolated (any Actor)? = #isolation) async throws
        -> (entry: Entry, events: [UsageLedger.Event], marks: [UsageLedger.Mark], reset: Bool) {
        let url = URL(fileURLWithPath: path), previous = entry(path)
        let contents = try await Log.contents(of: url)
        var entry = Entry(modified: file.modified, size: file.size, summary: Log.summary(for: url))
        var events: [UsageLedger.Event] = [], marks: [UsageLedger.Mark] = [], reset = false
        if let previous {
            if file.size < previous.size || (file.size == previous.size && !LedgerCopies.same(previous.modified, file.modified))
                || (contents?.count ?? file.size) < previous.offset {
                reset = true
            } else {
                entry.offset = previous.offset
                entry.summary = previous.summary
            }
        }
        pass.filesRead += 1
        if let contents {
            pass.bytesRead += contents.count - entry.offset
            entry.committed = contents.lastIndex(of: 0x0A).map { $0 + 1 } ?? 0
            repeat {
                guard entry.offset < entry.committed else { break }
                let limit = min(entry.committed, entry.offset + Self.chunkSize)
                let end = contents[(limit - 1)..<entry.committed].firstIndex(of: 0x0A)! + 1
                events += try Log.ingest(contents[entry.offset..<end], into: &entry.summary)
                marks += Log.drainMarks(&entry.summary)
                entry.offset = end
            } while Date() < deadline
        } else {
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            try handle.seek(toOffset: UInt64(entry.offset))
            var carry = Data()
            repeat {
                guard let chunk = try handle.read(upToCount: Self.chunkSize), !chunk.isEmpty else { break }
                pass.bytesRead += chunk.count
                carry.append(chunk)
                guard let newline = carry.lastIndex(of: 0x0A) else { continue }
                events += try Log.ingest(carry[...newline], into: &entry.summary)
                marks += Log.drainMarks(&entry.summary)
                entry.offset += newline + 1
                carry = Data(carry[(newline + 1)...])
            } while Date() < deadline
            // A read the budget stopped before the listed size leaves the rest of the file for the next poll.
            entry.committed = entry.offset + carry.count >= file.size ? entry.offset : file.size
        }
        Log.finishRead(&entry.summary, now: Date())
        return (entry, events, marks, reset)
    }

    private static func state(_ entry: Entry) -> UsageLedger.FileState {
        let data = try? JSONEncoder().encode(StoredState(offset: entry.offset, committed: entry.committed, summary: entry.summary))
        return UsageLedger.FileState(signature: LedgerCopies.signature(modified: entry.modified, size: entry.size), state: data,
                                     group: Log.group(entry.summary))
    }

    private static func entry(_ file: UsageLedger.FileState) -> Entry? {
        guard let parts = LedgerCopies.signature(file.signature), let data = file.state,
              let state = try? JSONDecoder().decode(StoredState.self, from: data) else { return nil }
        return Entry(modified: parts.modified, size: parts.size, offset: state.offset, committed: state.committed ?? parts.size, summary: state.summary)
    }

    /// `{"version", "offset", "committedSize", <summaryKey>}`. A state without `committedSize` has complete lines waiting
    /// unless its log was read to the end.
    private struct StoredState: Codable {
        let offset: Int
        let committed: Int?
        let summary: Log.Summary

        private struct Key: CodingKey {
            let stringValue: String
            var intValue: Int? { nil }
            init(_ stringValue: String) { self.stringValue = stringValue }
            init?(stringValue: String) { self.stringValue = stringValue }
            init?(intValue: Int) { nil }
        }

        init(offset: Int, committed: Int?, summary: Log.Summary) {
            self.offset = offset; self.committed = committed; self.summary = summary
        }

        init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: Key.self)
            guard try container.decode(Int.self, forKey: Key("version")) == Log.version else {
                throw DecodingError.dataCorruptedError(forKey: Key("version"), in: container, debugDescription: "Another state version")
            }
            offset = try container.decode(Int.self, forKey: Key("offset"))
            committed = try container.decodeIfPresent(Int.self, forKey: Key("committedSize"))
            summary = try container.decode(Log.Summary.self, forKey: Key(Log.summaryKey))
        }

        func encode(to encoder: any Encoder) throws {
            var container = encoder.container(keyedBy: Key.self)
            try container.encode(Log.version, forKey: Key("version"))
            try container.encode(offset, forKey: Key("offset"))
            try container.encodeIfPresent(committed, forKey: Key("committedSize"))
            try container.encode(summary, forKey: Key(Log.summaryKey))
        }
    }
}
