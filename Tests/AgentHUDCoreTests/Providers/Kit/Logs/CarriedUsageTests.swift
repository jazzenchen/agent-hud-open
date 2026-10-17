import XCTest
@testable import AgentHUDCore

/// A log read again because a new version of its reader replaced the state an earlier one wrote: what the earlier
/// version recorded keeps counting until the new read has covered the same times, so day totals, a session's breakdown
/// and its share of a window never drop while the history is read again, and nothing counts twice.
final class CarriedUsageTests: XCTestCase, @unchecked Sendable {
    /// What a fixture reader keeps of a log: the places of the requests it read, and the prompts not handed over yet.
    private struct Read: Codable, Sendable {
        var places: [Int] = []
        var marks: [UsageLedger.Mark] = []

        /// One `<mark> <tokens> <seconds since 1970> [padding]` line per request; a `T` line is a prompt, which starts a turn.
        mutating func take(_ lines: Data, at offset: Int, key: (_ place: Int, _ count: Int) -> String) -> [UsageLedger.Event] {
            var events: [UsageLedger.Event] = [], place = offset
            for line in lines.split(separator: 0x0A, omittingEmptySubsequences: false) {
                defer { place += line.count + 1 }
                let parts = String(decoding: line.prefix(64), as: UTF8.self).split(separator: " ")
                guard parts.count >= 3, let tokens = Int(parts[1]), let seconds = Double(parts[2]) else { continue }
                let time = Date(timeIntervalSince1970: seconds)
                places.append(place)
                if parts[0] == "T" { marks.append(.init(.prompt, at: time)) }
                events.append(.init(key: key(place, places.count), timestamp: time, agentId: "fixture-model:m", tokensIn: tokens, tokensOut: 0))
            }
            return events
        }

        mutating func drainMarks() -> [UsageLedger.Mark] {
            defer { marks = [] }
            return marks
        }
    }

    /// The earlier reader: from the log's start, each request keyed by its count, as Claude Code usage without a response
    /// id was before version 5.
    private enum Earlier: TailLog {
        static let source = "fixture", summaryKey = "read"
        static let version = 1
        static func summary(for url: URL) -> Read { Read() }
        static func ingest(_ lines: Data, at offset: Int, into read: inout Read) -> [UsageLedger.Event] {
            read.take(lines, at: offset) { _, count in "n:\(count)" }
        }
        static func drainMarks(_ read: inout Read) -> [UsageLedger.Mark] { read.drainMarks() }
    }

    /// The current reader: from the newest turn, then back a page at a time, each request keyed by its place in the log.
    /// Every read takes longer than `historyBudget`, so a pass reads at most one page of history.
    private enum Current: TailLog {
        static let source = Earlier.source, summaryKey = Earlier.summaryKey
        static let version = 2
        static func summary(for url: URL) -> Read { Read() }
        static func ingest(_ lines: Data, at offset: Int, into read: inout Read) -> [UsageLedger.Event] {
            Thread.sleep(forTimeInterval: 2 * CarriedUsageTests.historyBudget)
            return read.take(lines, at: offset) { place, _ in "o:\(place)" }
        }
        static let startsTurn: LogPager.StartsTurn? = { prefix, _ in prefix.first == UInt8(ascii: "T") }
        static func merge(older: Read, into read: inout Read) { read.places = older.places + read.places }
        static func drainMarks(_ read: inout Read) -> [UsageLedger.Mark] { read.drainMarks() }
    }

    /// A version after `Current`, which keys requests another way again.
    private enum Later: TailLog {
        static let source = Earlier.source, summaryKey = Earlier.summaryKey
        static let version = 3
        static func summary(for url: URL) -> Read { Read() }
        static func ingest(_ lines: Data, at offset: Int, into read: inout Read) -> [UsageLedger.Event] {
            Thread.sleep(forTimeInterval: 2 * CarriedUsageTests.historyBudget)
            return read.take(lines, at: offset) { place, _ in "p:\(place)" }
        }
        static let startsTurn = Current.startsTurn
        static func merge(older: Read, into read: inout Read) { read.places = older.places + read.places }
        static func drainMarks(_ read: inout Read) -> [UsageLedger.Mark] { read.drainMarks() }
    }

    private static let historyBudget: TimeInterval = 0.01

    /// What a reader's day totals, a session's breakdown and its share of a window are made of.
    private struct Figures: Equatable {
        let buckets: [UsageBucket]
        let breakdown: SessionUsage?
        let window: Int?
    }

    private let started = Date()
    /// Two days ago, at the start of a 15-minute period.
    private var recent: Date { Date(timeIntervalSince1970: ((started.timeIntervalSince1970 - 2 * 86400) / 900).rounded(.down) * 900) }

    /// A turn older than the ledger keeps, then three turns two days ago, a request a minute; each turn fills a page.
    private func lines(tokens: (Int) -> Int = { $0 % 5 + 1 }) -> [String] {
        let old = started.addingTimeInterval(-40 * 86400).timeIntervalSince1970
        let first = (0..<210).map { "\($0 == 0 ? "T" : "-") 1 \(Int(old) + $0 * 60)" }
        let rest = (0..<630).map { "\($0 % 210 == 0 ? "T" : "-") \(tokens($0)) \(Int(recent.timeIntervalSince1970) + $0 * 60)" }
        return first + rest
    }

    private func log(_ lines: [String]) throws -> (root: URL, url: URL) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let url = root.appendingPathComponent("session.log")
        try Data(lines.map { $0 + "\n" }.joined().utf8).write(to: url)
        return (root, url)
    }

    private func figures(_ ledger: UsageLedger, path: String) async throws -> Figures {
        // The window holds the newest turn and half of the one before it.
        Figures(buckets: try await ledger.buckets(since: .distantPast, source: Earlier.source),
                breakdown: try await ledger.sessionUsage([SessionUsageRequest(sessionID: "s", keys: [path], callLog: path)])["s"],
                window: try await ledger.tokens(source: Earlier.source, since: recent.addingTimeInterval(5 * 3600))[path])
    }

    /// Reads the log as the earlier version did, then with `store`: its newest turn first, then a page of history a pass,
    /// for `pages` passes or until none is left. Returns the figures the ledger held before the new read and after each
    /// of its passes.
    private func reread<Log: TailLog>(_ root: URL, in ledger: UsageLedger, with store: TailLogStore<Log>, pages: Int = 50) async throws
        -> (path: String, before: Figures, passes: [Figures]) {
        let (path, before) = try await readAsEarlier(root, into: ledger)
        store.historyBudget = 0
        var pass = await store.index(since: .distantPast, timeBudget: 5)
        var passes = [try await figures(ledger, path: path)]
        store.historyBudget = Self.historyBudget
        while pass.pending > 0, passes.count <= pages {
            pass = await store.index(since: .distantPast, timeBudget: 5)
            passes.append(try await figures(ledger, path: path))
        }
        return (path, before, passes)
    }

    /// Reads the log as the earlier version did, and returns the path the store keys it by and the figures that left.
    private func readAsEarlier(_ root: URL, into ledger: UsageLedger) async throws -> (path: String, before: Figures) {
        let earlier = await TailLogStore<Earlier>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
            .index(since: .distantPast, timeBudget: 5)
        let path = try XCTUnwrap(earlier.logs.first?.path, "the store keys a log by the path its listing gives")
        return (path, try await figures(ledger, path: path))
    }

    /// The figures of the same log read by `Log` alone.
    private func freshRead<Log: TailLog>(_ root: URL, path: String, as log: Log.Type) async throws -> Figures {
        let ledger = try UsageLedger(url: nil, expires: true)
        let store = TailLogStore<Log>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        store.historyBudget = 5
        while await store.index(since: .distantPast, timeBudget: 5).pending > 0 {}
        return try await figures(ledger, path: path)
    }

    func testTheSpanGrowsByWhatEachWriteAddsAndTheRestLeavesOnceTheLogIsReadWhole() throws {
        let fresh = CarriedUsage()
        XCTAssertTrue(fresh.writing([], whole: false).replaced.isEmpty)
        XCTAssertEqual(fresh.writing([], whole: true).replaced, [Int64.min...Int64.max], "a log that holds no usage now leaves none behind")
        var carried = try XCTUnwrap(fresh.writing([20, 10, 15], whole: false).carried)
        XCTAssertEqual(fresh.writing([20, 10, 15], whole: false).replaced, [10...20], "the newest turn replaces what was recorded at its times")
        XCTAssertEqual(carried.span, 10...20)
        XCTAssertEqual(carried.writing([3, 8], whole: false).replaced, [3...9], "an older page replaces the times before the span")
        carried = try XCTUnwrap(carried.writing([3, 8], whole: false).carried)
        XCTAssertEqual(carried.writing([25], whole: false).replaced, [21...25], "new lines replace the times after it")
        XCTAssertEqual(carried.writing([5, 12], whole: false).replaced, [], "times the span holds have nothing of the earlier version left")
        XCTAssertEqual(carried.writing([1, 30], whole: false).carried?.span, 1...30)
        let finished = carried.writing([1], whole: true)
        XCTAssertNil(finished.carried)
        XCTAssertEqual(finished.replaced, [Int64.min...2, 21...Int64.max], "everything outside the span goes once the log is read whole")

        XCTAssertEqual(String(decoding: try JSONEncoder().encode(carried), as: UTF8.self), #"{"span":[3,20]}"#)
        XCTAssertEqual(try JSONDecoder().decode(CarriedUsage.self, from: Data(#"{"span":[3,20]}"#.utf8)), carried)
        XCTAssertEqual(try JSONDecoder().decode(CarriedUsage.self, from: Data("{}".utf8)), fresh)
    }

    func testAVersionBumpKeepsEveryFigureWhilePagesCommitAndEndsAsAFreshRead() async throws {
        let (root, _) = try log(lines()), ledger = try UsageLedger(url: nil, expires: true)
        let store = TailLogStore<Current>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        let (path, before, passes) = try await reread(root, in: ledger, with: store)
        XCTAssertEqual(before.buckets.reduce(0) { $0 + $1.tokensIn }, (0..<630).reduce(0) { $0 + $1 % 5 + 1 })
        XCTAssertEqual(before.breakdown?.turns.count, 3, "the turn older than the ledger keeps was never recorded")
        XCTAssertNotNil(before.window)
        XCTAssertEqual(passes.first, before, "right after the first read the newest turn counts once, and the rest still counts")
        XCTAssertGreaterThan(passes.count, 3, "each page of history commits on its own pass")
        XCTAssertNil(store.entry(path)?.back, "the history is read in the end")
        for (index, figures) in passes.enumerated() {
            XCTAssertEqual(figures.buckets, before.buckets, "pass \(index): no 15-minute total drops or counts twice")
            XCTAssertEqual(figures.breakdown, before.breakdown, "pass \(index): the session's breakdown holds")
            XCTAssertEqual(figures.window, before.window, "pass \(index): its share of the window holds")
        }
        let fresh = try await freshRead(root, path: path, as: Current.self)
        XCTAssertEqual(passes.last, fresh, "the ledger ends as a read by the current version alone")
        XCTAssertNil(store.entry(path)?.carried, "nothing of the earlier version is left once the log is read whole")
        let state = try await ledger.fileStates(source: Earlier.source)[path]?.state
        XCTAssertNotNil(try state.flatMap { try JSONSerialization.jsonObject(with: $0) as? [String: Any] }?["offset"], "and its state is stored as any other")
    }

    func testAnotherBumpWhileTheFirstIsCarriedKeepsTheFiguresToo() async throws {
        let (root, _) = try log(lines()), ledger = try UsageLedger(url: nil, expires: true)
        // The current version reads the newest turn, and no history, before a later version replaces it in turn.
        let current = TailLogStore<Current>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        let (path, before, _) = try await reread(root, in: ledger, with: current, pages: 0)
        let later = TailLogStore<Later>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        later.historyBudget = Self.historyBudget
        var pass = await later.index(since: .distantPast, timeBudget: 5), checked = 0
        while true {
            let figures = try await figures(ledger, path: path)
            XCTAssertEqual(figures, before, "pass \(checked): usage of both earlier versions counts once")
            checked += 1
            guard pass.pending > 0, checked < 50 else { break }
            pass = await later.index(since: .distantPast, timeBudget: 5)
        }
        let fresh = try await freshRead(root, path: path, as: Later.self)
        let ended = try await figures(ledger, path: path)
        XCTAssertEqual(ended, fresh)
        XCTAssertNil(later.entry(path)?.carried)
    }

    func testALogOlderThanTheCutoffIsReadToItsEndBeforeTheEarlierUsageLeaves() async throws {
        // A sub-agent's log has no turn starts, so it is read from its start; at a megabyte a line, a pass on no time
        // budget reads three of its six lines (`TailLogStore.chunkSize`).
        let written = started.addingTimeInterval(-10 * 86400), padding = String(repeating: "p", count: 1 << 20)
        let (root, url) = try log((0..<6).map { "- \($0 + 1) \(Int(written.timeIntervalSince1970) + $0 * 60) \(padding)" })
        try FileManager.default.setAttributes([.modificationDate: written.addingTimeInterval(600)], ofItemAtPath: url.path)
        let ledger = try UsageLedger(url: nil, expires: true)
        let (path, before) = try await readAsEarlier(root, into: ledger)
        let store = TailLogStore<Current>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        let cutoff = started.addingTimeInterval(-7 * 86400)
        var pass = await store.index(since: cutoff, timeBudget: 0)
        XCTAssertEqual(pass.logs.count, 0, "the log is older than the cutoff")
        XCTAssertEqual(pass.pending, 1, "its read stopped after the first chunk")
        var held = try await figures(ledger, path: path)
        XCTAssertEqual(held, before, "what the read has not reached yet still counts")
        XCTAssertNotNil(store.entry(path)?.carried)
        pass = await store.index(since: cutoff, timeBudget: 0)
        XCTAssertEqual(pass.pending, 0, "the read goes on to the log's end")
        held = try await figures(ledger, path: path)
        XCTAssertEqual(held, before)
        XCTAssertEqual(held.buckets.reduce(0) { $0 + $1.tokensIn }, 21)
        XCTAssertNil(store.entry(path)?.carried, "and then nothing of the earlier version is left")
    }

    func testAMissingLogKeepsWhatItHeldTheEarlierUsageIncluded() async throws {
        let (root, url) = try log(lines()), ledger = try UsageLedger(url: nil, expires: true)
        let store = TailLogStore<Current>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        let (path, before, _) = try await reread(root, in: ledger, with: store, pages: 0)
        XCTAssertNotNil(store.entry(path)?.carried, "the history is still to read")
        let state = try await ledger.fileStates(source: Earlier.source)[path]?.state
        XCTAssertEqual(try state.map { Set(try XCTUnwrap(JSONSerialization.jsonObject(with: $0) as? [String: Any]).keys) }, ["version", "carrying"],
                       "a build from before this rule that reads the same version cannot decode it, and reads the log again from scratch")
        try FileManager.default.removeItem(at: url)
        _ = await store.index(since: .distantPast, timeBudget: 5)
        let figures = try await figures(ledger, path: path)
        XCTAssertFalse(before.buckets.isEmpty)
        XCTAssertEqual(figures.buckets, before.buckets, "a deleted log keeps its usage, the earlier version's included")
    }

    func testALogRewrittenSinceTheEarlierReadReplacesItsUsageAtOnce() async throws {
        for rewrite in ["shrank", "kept its size"] {
            let (root, url) = try log(lines()), ledger = try UsageLedger(url: nil, expires: true)
            _ = await TailLogStore<Earlier>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }.index(since: .distantPast, timeBudget: 5)
            // The same requests a turn shorter, or each at a token more, in a log written a minute later.
            let shorter = Array(lines().dropLast(210)), heavier = lines { $0 % 5 + 2 }
            try Data((rewrite == "shrank" ? shorter : heavier).map { $0 + "\n" }.joined().utf8).write(to: url)
            try FileManager.default.setAttributes([.modificationDate: started.addingTimeInterval(60)], ofItemAtPath: url.path)
            let store = TailLogStore<Current>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
            store.historyBudget = 0
            let pass = await store.index(since: .distantPast, timeBudget: 5)
            let path = try XCTUnwrap(pass.logs.first?.path)
            let total = try await ledger.buckets(since: .distantPast, source: Earlier.source).reduce(0) { $0 + $1.tokensIn }
            let newest = rewrite == "shrank" ? (210..<420).reduce(0) { $0 + $1 % 5 + 1 } : (420..<630).reduce(0) { $0 + $1 % 5 + 2 }
            XCTAssertEqual(total, newest, "a log that \(rewrite) counts only what the new read found")
            XCTAssertNil(store.entry(path)?.carried, "a log that \(rewrite) carries nothing over")
        }
    }
}
