import AgentHUDSupport
import Foundation

/// Reads append-only logs into the usage ledger, one contribution per log. A poll lists the logs, reads what changed
/// newest first from each saved position while its time budget lasts, and writes usage, positions and summaries in one
/// ledger write; a partially written final line is read once it is complete. A log that shrank, or changed without
/// growing, is read again as a log seen for the first time is, and replaces what it recorded at once. One whose stored
/// state has another version is read again too, even when it is older than the cutoff, while the ledger still holds its
/// usage; what that version recorded counts on until this read has covered the same times (`CarriedUsage`). A stored
/// log missing from the listing, deleted in its client or moved away, keeps its usage in the ledger until it expires,
/// unless another copy of it is listed and read, which then counts in its place; a copy read later takes its place too
/// (`UsageLedger.FileState.gone`). One that is listed keeps its contribution even when it cannot be read, and is tried
/// again once its pause is over (`FailedReads`). A log its client compressed where it lies is the same
/// log under another name (`CompressedLogs`): its state and contribution move to that name without a read, and it is
/// read from its start, through its decoded text, only where the ledger does not hold all of it.
final class TailLogStore<Log: TailLog> {
    struct Entry {
        var modified: Date
        var size: Int
        /// Just past the last line read.
        var offset = 0
        /// Just past the last complete line the last read saw; beyond `offset` while complete lines wait.
        var committed = 0
        var summary: Log.Summary
        /// Where decoding can start again without losing the line at `offset`, for a log that has to be decoded.
        var restart: DecodedPosition?
        /// Where the history not read yet ends; nil once it is read back to the log's start or past the ledger's
        /// retention, and for a log read from its start.
        var back: Int?
        /// While usage an earlier version of the reader recorded still counts in the ledger, the times this version has
        /// written so far (`CarriedUsage`); nil once the log has been read whole, or when nothing was carried.
        var carried: CarriedUsage?
    }

    /// One poll's work.
    struct Pass {
        /// Logs modified since the cutoff.
        var logs: [(path: String, file: LogFiles.File)] = []
        /// Logs whose entries this pass changed or dropped; after a rolled-back pass every entry was reloaded.
        var changed: [String] = [], removed: [String] = [], reloaded = false
        /// Logs the budget did not reach or finish, and changes the ledger did not take.
        var pending = 0
        /// Why logs could not be read, at this poll or at their last attempt; each is read again once its pause is over.
        var failures: [any Error] = []
        var gaps = LogFiles.Gaps()
        var filesRead = 0, bytesRead = 0
    }

    static var chunkSize: Int { 4 << 20 }
    /// Lines a page of history holds at least, before it reaches back to the start of a turn.
    static var pageLines: Int { 200 }
    /// Lines looked back over for the state a page starts from.
    static var seedLines: Int { 2000 }
    /// How much of a log's start is read for its head.
    static var headLength: Int { 256 << 10 }

    private typealias Update = (entry: Entry, events: [UsageLedger.Event], marks: [UsageLedger.Mark], reset: Bool)

    private let ledger: UsageLedger
    private let files: LogFiles
    private var entries: [String: Entry] = [:]
    /// Stored states not decoded yet; most logs are older than the cutoff and never need it.
    private var stored: [String: UsageLedger.FileState] = [:]
    /// Logs whose stored state has another version or cannot be decoded, to read again, with that state.
    private var outdated: [String: UsageLedger.FileState] = [:]
    /// Logs whose stored state says their last read stopped before their end, as a time budget or a quit can leave it;
    /// until read on, which an older log is only from here.
    private var unfinished: Set<String> = []
    /// Sessions and modification times of grouped logs, for choosing the copy that counts.
    private var groups: [String: String] = [:]
    private var modified: [String: Date] = [:]
    /// Logs that left the listing whose usage the ledger keeps, with their groups ("" for none).
    private var gone: [String: String] = [:]
    private var loadedGeneration: Int?
    private var failed = FailedReads()
    /// How long a poll reads history, apart from what logs gained: a small share of the processor while history lasts.
    var historyBudget: TimeInterval = 0.1
    private var failureReasons: [String: any Error] = [:]
    private var compressed = Log.compressedSuffix.map(CompressedLogs.init(suffix:))

    /// - watchesChanges: after the first listing, polls look only at logs a directory watch reports changed.
    init(roots: [URL], ledger: UsageLedger, watchesChanges: Bool, accepts: @escaping (URL) -> Bool) {
        self.ledger = ledger
        files = LogFiles(roots: roots, watchesChanges: watchesChanges, accepts: accepts)
    }

    func noteChanges(_ paths: Set<String>?) { files.noteChanges(paths) }

    func entry(_ path: String) -> Entry? {
        if let entry = entries[path] { return entry }
        guard let file = stored.removeValue(forKey: path) else { return nil }
        guard let entry = Self.entry(file) else {
            outdated[path] = file
            return nil
        }
        entries[path] = entry
        return entry
    }

    func index(since cutoff: Date, timeBudget: TimeInterval, isolation: isolated (any Actor)? = #isolation) async -> Pass {
        var pass = Pass()
        let started = Date(), deadline = started.addingTimeInterval(timeBudget)
        // Logs modified since then can hold usage the ledger keeps.
        let retained = started.addingTimeInterval(-UsageLedger.retention)
        // Stored states are read once, and again after a failed pass rolled back what this store had written.
        let generation = await ledger.generation
        if loadedGeneration != generation {
            let states = (try? await ledger.fileStates(source: Log.source)) ?? [:]
            gone = states.filter(\.value.isGone).mapValues { $0.group ?? "" }
            stored = states.filter { !$0.value.isGone }
            entries = [:]
            // Only the version and position are decoded, and only of logs recent enough to matter.
            let recent = stored.filter { (LedgerCopies.signature($0.value.signature)?.modified ?? .distantPast) >= retained }
            let progress = recent.mapValues(Self.progress)
            outdated = recent.filter { progress[$0.key]??.version != Log.version }
            unfinished = Set(progress.filter { $0.value.map { $0.version == Log.version && $0.offset < $0.committed } == true }.keys)
            groups = stored.compactMapValues(\.group)
            modified = stored.compactMapValues { $0.group == nil ? nil : LedgerCopies.signature($0.signature)?.modified }
            loadedGeneration = generation
            pass.reloaded = true
        }
        pass.gaps = files.refresh(now: started)
        let listing = Self.counting(compressed?.listing(files.files) ?? files.files)
        let handovers = handOver(listing)
        var changing: [(path: String, file: LogFiles.File)] = []
        for (path, file) in listing where file.modified >= cutoff {
            pass.logs.append((path, file))
            if let entry = entry(path), entry.size == file.size, LedgerCopies.same(entry.modified, file.modified), entry.offset >= entry.committed { continue }
            changing.append((path, file))
        }
        // An older log whose state another version wrote may hold usage that version counted differently, and one whose
        // last read stopped short has lines waiting: both are read on over as many polls as that takes.
        for path in Set(outdated.keys).union(unfinished).union(entries.filter { $0.value.offset < $0.value.committed }.keys) {
            if let file = listing[path], file.modified < cutoff, file.modified >= retained { changing.append((path, file)) }
        }

        // A log that could not be read waits out its pause, and is neither read nor waited for meanwhile.
        failed.keep(Set(listing.keys))
        failureReasons = failureReasons.filter { listing[$0.key] != nil }
        let pausing = changing.filter { failed.isPausing($0.path, at: started) }
        pass.failures = pausing.compactMap { failureReasons[$0.path] }
        let reading = changing.filter { !failed.isPausing($0.path, at: started) }
        // Always make progress on the newest log, then keep going while the budget lasts.
        var updates: [String: Update] = [:]
        for (index, log) in reading.sorted(by: { $0.file.modified > $1.file.modified }).enumerated() {
            if index > 0, Date() >= deadline {
                pass.pending += reading.count - index
                break
            }
            do {
                let update = try await read(log.path, file: log.file, deadline: deadline, pass: &pass)
                updates[log.path] = update
                failed.succeeded(log.path)
                failureReasons[log.path] = nil
                if update.entry.offset < update.entry.committed { pass.pending += 1 }
            } catch {
                if !Task.isCancelled { failed.failed(log.path, at: Date()) }
                failureReasons[log.path] = error
                pass.failures.append(error)
            }
        }

        // History on a budget of its own: a page at a time, the most recently written log first. A log whose new lines
        // were not all read, or that could not be read, waits.
        let historyDeadline = Date().addingTimeInterval(historyBudget)
        for (path, file) in listing.sorted(by: { $0.value.modified > $1.value.modified }) where file.modified >= retained {
            let waiting = changing.contains { $0.path == path } && updates[path] == nil
            guard !waiting, !failed.isPausing(path, at: started), let current = updates[path]?.entry ?? entry(path),
                  current.back != nil else { continue }
            var update = updates[path] ?? (entry: current, events: [], marks: [], reset: false)
            do {
                while update.entry.back != nil, Date() < historyDeadline {
                    try readPage(path, into: &update, retained: retained, pass: &pass)
                }
                updates[path] = update
            } catch {
                if !Task.isCancelled { failed.failed(path, at: Date()) }
                failureReasons[path] = error
                pass.failures.append(error)
            }
            if update.entry.back != nil { pass.pending += 1 }
        }

        let removed = Set(entries.keys).union(stored.keys).union(outdated.keys).filter { listing[$0] == nil }
        guard !updates.isEmpty || !removed.isEmpty || !handovers.isEmpty else { return pass }
        var nextGroups = groups, nextModified = modified, replaced: [String: [ClosedRange<Int64>]] = [:]
        for path in updates.keys { replaced[path] = Self.replacing(&updates[path]!) }
        for (path, update) in updates {
            nextGroups[path] = Log.group(update.entry.summary)
            nextModified[path] = nextGroups[path] == nil ? nil : update.entry.modified
        }
        for path in removed { nextGroups[path] = nil; nextModified[path] = nil }
        // A log that left keeps its usage unless a copy of it that was read is listed, one of the same session or under
        // the same name (`Log.copyName`), and counts in its place: a newer copy also leaves an older one out of the
        // listing. A kept log gives way to such a copy read later, and one listed again is read as new and replaces
        // what it kept.
        let listedGroups = Set(nextGroups.values.filter { !$0.isEmpty })
        let listedNames = Set(listing.keys.filter { updates[$0] != nil || entries[$0] != nil || stored[$0] != nil }.compactMap(Log.copyName))
        func copied(_ path: String, group: String) -> Bool {
            (!group.isEmpty && listedGroups.contains(group)) || Log.copyName(path).map(listedNames.contains) == true
        }
        let kept = Dictionary(uniqueKeysWithValues: removed.compactMap { path -> (String, String)? in
            let group = groups[path] ?? ""
            return copied(path, group: group) ? nil : (path, group)
        })
        let superseded = Set(gone.filter { path, group in updates[path] != nil || copied(path, group: group) }.keys)
        let counted = LedgerCopies.counted(touched: Set(updates.keys).union(removed), previous: groups, members: nextGroups, modified: nextModified)
        let writes = updates.map { path, update in
            (path: path, reset: update.reset, replaced: replaced[path] ?? [], events: update.events, marks: update.marks,
             counted: nextGroups[path].map { _ in counted[path] ?? false }, state: Self.state(update.entry))
        }
        let source = Log.source, written = Set(updates.keys)
        do {
            try await ledger.write { writer in
                for move in handovers {
                    try writer.rename(source: source, contribution: move.from, to: move.to)
                    try writer.renameFile(source: source, path: move.from, to: move.to)
                }
                for path in superseded {
                    try writer.remove(source: source, contribution: path)
                    try writer.removeFile(source: source, path: path)
                }
                for write in writes {
                    if write.reset { try writer.remove(source: source, contribution: write.path) }
                    for times in write.replaced { try writer.removeUsage(source: source, contribution: write.path, at: times) }
                    try writer.upsert(source: source, contribution: write.path, counted: write.counted, events: write.events)
                    try writer.addMarks(source: source, contribution: write.path, marks: write.marks)
                    try writer.setFile(source: source, path: write.path, state: write.state)
                }
                for path in removed {
                    if let group = kept[path] {
                        try writer.setFile(source: source, path: path, state: .gone(group: group.isEmpty ? nil : group))
                    } else {
                        try writer.remove(source: source, contribution: path)
                        try writer.removeFile(source: source, path: path)
                    }
                }
                for (path, value) in counted where !written.contains(path) {
                    try writer.setCounted(source: source, contribution: path, counted: value)
                }
            }
            for (path, update) in updates { entries[path] = update.entry }
            for path in removed { entries[path] = nil; stored[path] = nil }
            for path in superseded { gone[path] = nil }
            gone.merge(kept) { _, new in new }
            for path in written.union(removed) { outdated[path] = nil }
            unfinished.subtract(written.union(removed))
            groups = nextGroups
            modified = nextModified
            pass.changed = Array(Set(updates.keys).union(handovers.map(\.to)))
            pass.removed = Array(removed.union(handovers.map(\.from)))
        } catch {
            // Positions stay where the ledger has them, so the next poll reads the same bytes again. Names this store
            // moved ahead of the ledger are read from it again.
            pass.pending += updates.count
            if !handovers.isEmpty { loadedGeneration = nil }
        }
        return pass
    }

    /// Moves the state of each stored log that changed form to its new name, ahead of the ledger write that renames its
    /// contribution and stored state (`CompressedLogs.handovers`).
    private func handOver(_ listing: [String: LogFiles.File]) -> [(from: String, to: String)] {
        guard let compressed else { return [] }
        let moves = compressed.handovers(stored: Set(entries.keys).union(stored.keys).union(outdated.keys), listing: listing)
        for (from, to) in moves {
            entries[to] = entries.removeValue(forKey: from)
            stored[to] = stored.removeValue(forKey: from)
            if let state = outdated.removeValue(forKey: from) { outdated[to] = state }
            if unfinished.remove(from) != nil { unfinished.insert(to) }
            groups[to] = groups.removeValue(forKey: from)
            modified[to] = modified.removeValue(forKey: from)
        }
        return moves
    }

    /// Reads what the log gained since its entry, or, once rewritten, all of it: from its start, or for a log that tells
    /// where its turns start, from its newest turn, its history left to `readPage`. Each read ingests at least one chunk.
    /// A log read again only because another version of the reader wrote its state keeps that version's usage counting.
    private func read(_ path: String, file: LogFiles.File, deadline: Date, pass: inout Pass,
                      isolation: isolated (any Actor)? = #isolation) async throws -> Update {
        let url = URL(fileURLWithPath: path), previous = entry(path), earlier = outdated[path]
        var entry = Entry(modified: file.modified, size: file.size, summary: Log.summary(for: url))
        var events: [UsageLedger.Event] = [], marks: [UsageLedger.Mark] = []
        let last = previous.map { (modified: $0.modified, size: $0.size) } ?? earlier.flatMap { LedgerCopies.signature($0.signature) }
        let rewritten = last.map { file.size < $0.size || (file.size == $0.size && !LedgerCopies.same($0.modified, file.modified)) } == true
        var reset = rewritten || earlier != nil
        let isCompressed = compressed?.isCompressed(path) == true
        // A log that has to be decoded is decoded from the last place before the first line not read yet.
        var contents = isCompressed ? nil
            : try await Log.contents(of: url, from: reset ? DecodedPosition() : previous?.restart ?? DecodedPosition())
        if let previous, !reset, (contents.map { $0.start.decoded + $0.data.count } ?? file.size) < previous.offset {
            // What was read before is no longer there: the log was rewritten.
            reset = true
            if contents != nil { contents = try await Log.contents(of: url, from: DecodedPosition()) }
        }
        if let previous, !reset {
            entry.offset = previous.offset
            entry.summary = previous.summary
            entry.restart = previous.restart
            entry.back = previous.back
            entry.carried = previous.carried
        } else {
            if earlier != nil, !rewritten { entry.carried = CarriedUsage() }
            if contents == nil, !isCompressed, let startsTurn = Log.startsTurn {
                let handle = try FileHandle(forReadingFrom: url)
                defer { try? handle.close() }
                Log.head(try Self.head(of: handle, size: file.size), into: &entry.summary)
                let pager = LogPager(handle: handle, startsTurn: startsTurn)
                entry.offset = try pager.lastTurnStart(before: try pager.completeEnd(of: file.size))
                entry.back = entry.offset > 0 ? entry.offset : nil
                try Self.seed(&entry.summary, before: entry.offset, with: pager)
            }
        }
        pass.filesRead += 1
        if let contents {
            // Offsets in the decoded text; the data holds it from its start's decoded byte.
            let base = contents.start.decoded, data = contents.data
            pass.bytesRead += base + data.count - entry.offset
            entry.committed = max(entry.offset, base + (data.lastIndex(of: 0x0A).map { $0 - data.startIndex + 1 } ?? 0))
            repeat {
                guard entry.offset < entry.committed else { break }
                let limit = min(entry.committed, entry.offset + Self.chunkSize)
                let end = data[(data.startIndex + limit - base - 1)..<(data.startIndex + entry.committed - base)].firstIndex(of: 0x0A)! + 1
                events += try Log.ingest(data[(data.startIndex + entry.offset - base)..<end], at: entry.offset, into: &entry.summary)
                marks += Log.drainMarks(&entry.summary)
                entry.offset = base + end - data.startIndex
            } while Date() < deadline
            entry.restart = contents.restarts.last { $0.decoded <= entry.offset } ?? contents.start
        } else if isCompressed {
            let read = try compressed?.read(path, file: file, from: entry.offset, chunkSize: Self.chunkSize, deadline: deadline) { lines, at in
                events += try Log.ingest(lines, at: at, into: &entry.summary)
                marks += Log.drainMarks(&entry.summary)
            }
            pass.bytesRead += read?.decoded ?? 0
            entry.offset = read?.offset ?? entry.offset
            // Text left after the budget waits as complete lines do; a partial last line waits for the log to change.
            entry.committed = read?.waiting == true ? max(file.size, entry.offset + 1) : entry.offset
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
                events += try Log.ingest(carry[...newline], at: entry.offset, into: &entry.summary)
                marks += Log.drainMarks(&entry.summary)
                entry.offset += newline + 1
                carry = Data(carry[(newline + 1)...])
            } while Date() < deadline
            // A read the budget stopped before the listed size leaves the rest of the file for the next poll.
            entry.committed = entry.offset + carry.count >= file.size ? entry.offset : file.size
        }
        // A read the budget stopped short goes on at the next poll with all it knew, such as the responses it counted.
        if entry.offset >= entry.committed { Log.finishRead(&entry.summary, now: Date()) }
        return (entry, events, marks, reset && entry.carried == nil)
    }

    /// Reads the page of history that ends where the update's history does, and takes it into the update: the page's
    /// usage and marks, and its summary into the summary of everything after it. History stops at the log's start, or at
    /// a page whose usage is all older than the ledger keeps.
    private func readPage(_ path: String, into update: inout Update, retained: Date, pass: inout Pass) throws {
        guard let back = update.entry.back, let startsTurn = Log.startsTurn else { return }
        let url = URL(fileURLWithPath: path)
        if let compressed, compressed.isCompressed(path) {
            // A log compressed before its history was read: the rest of its history is one page from its start.
            var page = Log.summary(for: url), events: [UsageLedger.Event] = [], marks: [UsageLedger.Mark] = []
            pass.bytesRead += try compressed.readHistory(path, before: back, headLength: Self.headLength, chunkSize: Self.chunkSize,
                                                         head: { Log.head($0, into: &page) }) { lines, at in
                events += try Log.ingest(lines, at: at, into: &page)
                marks += Log.drainMarks(&page)
            }
            pass.filesRead += 1
            Log.merge(older: page, into: &update.entry.summary)
            update.events += events
            update.marks += marks
            update.entry.back = nil
            return
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let pager = LogPager(handle: handle, startsTurn: startsTurn)
        let start = try pager.pageStart(before: back, minimumLines: Self.pageLines)
        var page = Log.summary(for: url), events: [UsageLedger.Event] = []
        Log.head(try Self.head(of: handle, size: update.entry.size), into: &page)
        try Self.seed(&page, before: start, with: pager)
        try handle.seek(toOffset: UInt64(start))
        var offset = start, carry = Data()
        while offset + carry.count < back {
            guard let chunk = try handle.read(upToCount: min(Self.chunkSize, back - offset - carry.count)), !chunk.isEmpty else { break }
            pass.bytesRead += chunk.count
            carry.append(chunk)
            guard let newline = carry.lastIndex(of: 0x0A) else { continue }
            events += try Log.ingest(carry[...newline], at: offset, into: &page)
            update.marks += Log.drainMarks(&page)
            offset += newline + 1
            carry = Data(carry[(newline + 1)...])
        }
        pass.filesRead += 1
        Log.merge(older: page, into: &update.entry.summary)
        update.events += events
        let done = start == 0 || (!events.isEmpty && events.allSatisfy { $0.timestamp < retained })
        update.entry.back = done ? nil : start
    }

    /// The times whose earlier usage a write of the update replaces, leaving in the update what it carries on after the
    /// write: nothing once the log is read whole, back to its start or the ledger's retention and up to its end.
    private static func replacing(_ update: inout Update) -> [ClosedRange<Int64>] {
        guard let carried = update.entry.carried else { return [] }
        let times = update.events.map { RecordCoding.milliseconds($0.timestamp) } + update.marks.map { RecordCoding.milliseconds($0.timestamp) }
        let next = carried.writing(times, whole: update.entry.back == nil && update.entry.offset >= update.entry.committed)
        update.entry.carried = next.carried
        return next.replaced
    }

    /// Seeds a summary that starts at `start` with the state the newest line before it carries.
    private static func seed(_ summary: inout Log.Summary, before start: Int, with pager: LogPager) throws {
        guard start > 0, let carries = Log.carriesState,
              let line = try pager.lastLine(before: start, limit: seedLines, matching: carries) else { return }
        Log.seed(line, into: &summary)
    }

    /// The complete lines among a log's first `headLength` bytes.
    private static func head(of handle: FileHandle, size: Int) throws -> Data {
        try handle.seek(toOffset: 0)
        let data = try handle.read(upToCount: min(size, headLength)) ?? Data()
        return data.lastIndex(of: 0x0A).map { Data(data[...$0]) } ?? Data()
    }

    /// The listed logs that count: of the copies sharing a `Log.copyName`, the most recently modified.
    private static func counting(_ files: [String: LogFiles.File]) -> [String: LogFiles.File] {
        var result: [String: LogFiles.File] = [:], newest: [String: String] = [:]
        for (path, file) in files {
            guard let name = Log.copyName(path) else {
                result[path] = file
                continue
            }
            if let other = newest[name], let kept = result[other] {
                guard (file.modified, path) > (kept.modified, other) else { continue }
                result[other] = nil
            }
            newest[name] = path
            result[path] = file
        }
        return result
    }

    /// A stored state's version, and where its last read stopped against the end of the complete lines it saw; nil for a
    /// state that cannot be decoded.
    private static func progress(_ file: UsageLedger.FileState) -> (version: Int, offset: Int, committed: Int)? {
        guard let stored = file.state.flatMap({ try? JSONDecoder().decode(StoredProgress.self, from: $0) }) else { return nil }
        return (stored.version, stored.offset ?? 0, stored.committedSize ?? LedgerCopies.signature(file.signature)?.size ?? 0)
    }

    private static func state(_ entry: Entry) -> UsageLedger.FileState {
        let data = try? JSONEncoder().encode(StoredState(offset: entry.offset, committed: entry.committed, summary: entry.summary,
                                                         restart: entry.restart, back: entry.back, carried: entry.carried))
        return UsageLedger.FileState(signature: LedgerCopies.signature(modified: entry.modified, size: entry.size), state: data,
                                     group: Log.group(entry.summary))
    }

    private static func entry(_ file: UsageLedger.FileState) -> Entry? {
        guard let parts = LedgerCopies.signature(file.signature), let data = file.state,
              let state = try? JSONDecoder().decode(StoredState.self, from: data) else { return nil }
        return Entry(modified: parts.modified, size: parts.size, offset: state.offset, committed: state.committed ?? parts.size, summary: state.summary,
                     restart: state.restart, back: state.back, carried: state.carried)
    }

    /// `{"version", "offset", "committedSize", <summaryKey>, "restart", "back"}`. A state without `committedSize` has
    /// complete lines waiting unless its log was read to the end; one without `restart` decodes its log from the start;
    /// one without `back` has no history left to read. A log that still carries an earlier version's usage keeps all but
    /// its version, and `carried`, under `carrying`: a build from before `CarriedUsage` that reads the same version cannot
    /// decode it, so it reads the log again from scratch instead of counting that usage twice.
    private struct StoredState: Codable {
        let offset: Int
        let committed: Int?
        let summary: Log.Summary
        let restart: DecodedPosition?
        let back: Int?
        let carried: CarriedUsage?

        private struct Key: CodingKey {
            let stringValue: String
            var intValue: Int? { nil }
            init(_ stringValue: String) { self.stringValue = stringValue }
            init?(stringValue: String) { self.stringValue = stringValue }
            init?(intValue: Int) { nil }
        }

        init(offset: Int, committed: Int?, summary: Log.Summary, restart: DecodedPosition?, back: Int?, carried: CarriedUsage?) {
            self.offset = offset; self.committed = committed; self.summary = summary; self.restart = restart; self.back = back
            self.carried = carried
        }

        init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: Key.self)
            guard try container.decode(Int.self, forKey: Key("version")) == Log.version else {
                throw DecodingError.dataCorruptedError(forKey: Key("version"), in: container, debugDescription: "Another state version")
            }
            let state = container.contains(Key("carrying")) ? try container.nestedContainer(keyedBy: Key.self, forKey: Key("carrying")) : container
            offset = try state.decode(Int.self, forKey: Key("offset"))
            committed = try state.decodeIfPresent(Int.self, forKey: Key("committedSize"))
            summary = try state.decode(Log.Summary.self, forKey: Key(Log.summaryKey))
            restart = try state.decodeIfPresent(DecodedPosition.self, forKey: Key("restart"))
            back = try state.decodeIfPresent(Int.self, forKey: Key("back"))
            carried = try state.decodeIfPresent(CarriedUsage.self, forKey: Key("carried"))
        }

        func encode(to encoder: any Encoder) throws {
            var container = encoder.container(keyedBy: Key.self)
            try container.encode(Log.version, forKey: Key("version"))
            var state = carried == nil ? container : container.nestedContainer(keyedBy: Key.self, forKey: Key("carrying"))
            try state.encode(offset, forKey: Key("offset"))
            try state.encodeIfPresent(committed, forKey: Key("committedSize"))
            try state.encode(summary, forKey: Key(Log.summaryKey))
            try state.encodeIfPresent(restart, forKey: Key("restart"))
            try state.encodeIfPresent(back, forKey: Key("back"))
            try state.encodeIfPresent(carried, forKey: Key("carried"))
        }
    }
}

/// The version of a stored state and its position, read without its summary; a carrying state keeps its position under
/// `carrying`.
private struct StoredProgress: Decodable {
    let version: Int
    let offset: Int?
    let committedSize: Int?

    private enum Keys: String, CodingKey { case version, offset, committedSize, carrying }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: Keys.self)
        version = try container.decode(Int.self, forKey: .version)
        let state = container.contains(.carrying) ? try container.nestedContainer(keyedBy: Keys.self, forKey: .carrying) : container
        offset = try state.decodeIfPresent(Int.self, forKey: .offset)
        committedSize = try state.decodeIfPresent(Int.self, forKey: .committedSize)
    }
}
