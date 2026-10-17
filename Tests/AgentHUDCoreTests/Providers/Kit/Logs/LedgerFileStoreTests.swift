import XCTest
@testable import AgentHUDCore

final class LedgerFileStoreTests: XCTestCase, @unchecked Sendable {
    private static let base = Date(timeIntervalSince1970: 1_800_000_000)

    /// One `<key> <tokens>` line per request.
    private enum Requests: TailLog {
        static let source = "fixture"
        static let summaryKey = "keys"
        static let version = 1
        static func summary(for url: URL) -> [String] { [] }
        static func ingest(_ lines: Data, at offset: Int, into keys: inout [String]) -> [UsageLedger.Event] {
            lines.split(separator: 0x0A).map { line in
                let parts = String(decoding: line, as: UTF8.self).split(separator: " ").map(String.init)
                keys.append(parts[0])
                return UsageLedger.Event(key: parts[0], timestamp: LedgerFileStoreTests.base, agentId: "fixture-model:m", tokensIn: Int(parts[1])!, tokensOut: 0)
            }
        }
    }

    /// The same logs read by a later version that skips `x` lines.
    private enum RequestsWithoutX: TailLog {
        static let source = Requests.source
        static let summaryKey = Requests.summaryKey
        static let version = 2
        static func summary(for url: URL) -> [String] { [] }
        static func ingest(_ lines: Data, at offset: Int, into keys: inout [String]) -> [UsageLedger.Event] {
            Requests.ingest(Data(lines.split(separator: 0x0A).filter { $0.first != UInt8(ascii: "x") }.joined(separator: [0x0A])), at: offset, into: &keys)
        }
    }

    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private func tokens(_ ledger: UsageLedger, source: String = Requests.source) async throws -> Int {
        try await ledger.buckets(since: .distantPast, source: source).reduce(0) { $0 + $1.tokensIn }
    }

    func testAMissingLogKeepsItsUsageAndAListedUnreadableLogStays() async throws {
        let root = try directory(), ledger = UsageLedger.inMemory()
        let kept = root.appendingPathComponent("kept.log"), gone = root.appendingPathComponent("gone.log")
        try "a 10\n".write(to: kept, atomically: true, encoding: .utf8)
        try "b 5\n".write(to: gone, atomically: true, encoding: .utf8)
        let store = TailLogStore<Requests>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        _ = await store.index(since: .distantPast, timeBudget: 5)
        var total = try await tokens(ledger)
        XCTAssertEqual(total, 15)
        try "a 10\nc 1\n".write(to: kept, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: kept.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: kept.path) }
        try FileManager.default.removeItem(at: gone)
        let pass = await store.index(since: .distantPast, timeBudget: 5)
        XCTAssertEqual(pass.failures.count, 1)
        total = try await tokens(ledger)
        XCTAssertEqual(total, 15, "the deleted log keeps its usage; the unreadable one keeps what it recorded")
        var files = try await ledger.fileStates(source: Requests.source)
        XCTAssertEqual(Set(files.keys.map { URL(fileURLWithPath: $0).lastPathComponent }), ["kept.log", "gone.log"])
        XCTAssertEqual(files.filter(\.value.isGone).keys.map { URL(fileURLWithPath: $0).lastPathComponent }, ["gone.log"],
                       "only that it left is stored")

        // Listed again, it is read as new and replaces what it kept.
        try "b 5\nd 2\n".write(to: gone, atomically: true, encoding: .utf8)
        _ = await store.index(since: .distantPast, timeBudget: 5)
        total = try await tokens(ledger)
        XCTAssertEqual(total, 17)
        // Kept once more, it goes when its usage expires.
        try FileManager.default.removeItem(at: gone)
        _ = await store.index(since: .distantPast, timeBudget: 5)
        try await ledger.expire(now: Self.base.addingTimeInterval(UsageLedger.retention + 86400))
        files = try await ledger.fileStates(source: Requests.source)
        XCTAssertFalse(files.keys.contains { $0.hasSuffix("/gone.log") })
    }

    func testALogOfAnotherStateVersionIsReadAgainAndReplacesItsUsage() async throws {
        let root = try directory(), ledger = UsageLedger.inMemory(), now = Date()
        let log = root.appendingPathComponent("s.log"), expired = root.appendingPathComponent("expired.log")
        try "a 10\nx 5\n".write(to: log, atomically: true, encoding: .utf8)
        try "b 1\nx 2\n".write(to: expired, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-10 * 86400)], ofItemAtPath: log.path)
        try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-UsageLedger.retention - 86400)], ofItemAtPath: expired.path)
        let first = await TailLogStore<Requests>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }.index(since: .distantPast, timeBudget: 5)
        let path = try XCTUnwrap(first.logs.first { $0.path.hasSuffix("/s.log") }?.path)
        var total = try await tokens(ledger)
        XCTAssertEqual(total, 18)

        let upgraded = TailLogStore<RequestsWithoutX>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        let pass = await upgraded.index(since: now.addingTimeInterval(-7 * 86400), timeBudget: 5)
        XCTAssertEqual(pass.logs.count, 0, "both logs are older than the cutoff")
        XCTAssertEqual(pass.filesRead, 1, "a log the ledger no longer holds usage of is left as it is")
        total = try await tokens(ledger)
        XCTAssertEqual(total, 13, "the log was read again from the start and replaced, in the same write, what the other version counted")
        XCTAssertNil(upgraded.entry(path)?.carried, "a log read whole in one pass carries nothing of the other version")
        let again = await upgraded.index(since: now.addingTimeInterval(-7 * 86400), timeBudget: 5)
        XCTAssertEqual(again.filesRead, 0)
    }

    func testAListingStoppedByItsLimitKeepsTheFilesItDidNotReach() throws {
        let root = try directory()
        for name in ["a", "b", "c"] { try "a 1\n".write(to: root.appendingPathComponent("\(name).log"), atomically: true, encoding: .utf8) }
        let files = LogFiles(roots: [root], watchesChanges: false, limit: 3) { _ in true }
        XCTAssertFalse(files.refresh(now: Date()).truncated)
        for name in ["d", "e"] { try "a 1\n".write(to: root.appendingPathComponent("\(name).log"), atomically: true, encoding: .utf8) }
        try FileManager.default.removeItem(at: root.appendingPathComponent("a.log"))
        XCTAssertTrue(files.refresh(now: Date()).truncated)
        let names = Set(files.files.keys.map { URL(fileURLWithPath: $0).lastPathComponent })
        XCTAssertTrue(names.isSuperset(of: ["b.log", "c.log"]), "a file past the limit is not gone")
        XCTAssertFalse(names.contains("a.log"), "a file that is gone leaves")
    }

    func testAFileWhoseParseFailedWaitsBeforeItIsParsedAgain() throws {
        let root = try directory(), file = root.appendingPathComponent("s.db")
        try "a".write(to: file, atomically: true, encoding: .utf8)
        final class Calls { var count = 0 }
        let calls = Calls()
        var store = WholeFileStore<Int>(listings: [.init(name: "fixture", files: LogFiles(roots: [root], watchesChanges: false) { _ in true },
                                                         parse: { _, _ in calls.count += 1; throw ProviderFailure.limit })])
        XCTAssertNotNil(store.index(since: .distantPast).notices["fixture"])
        try "ab".write(to: file, atomically: true, encoding: .utf8)
        let pass = store.index(since: .distantPast)
        XCTAssertEqual(calls.count, 1, "a file that failed waits out its pause, changed or not")
        XCTAssertNotNil(pass.notices["fixture"], "and says so meanwhile")
        XCTAssertNil(pass.indexing, "without counting as still to read")
    }

    func testRolledBackPassIsReadAndWrittenAgain() async throws {
        let root = try directory(), ledger = UsageLedger.inMemory()
        try "a 10\n".write(to: root.appendingPathComponent("s.log"), atomically: true, encoding: .utf8)
        let store = TailLogStore<Requests>(roots: [root], ledger: ledger, watchesChanges: false) { _ in true }
        await ledger.beginPass()
        _ = await store.index(since: .distantPast, timeBudget: 5)
        await ledger.rollBackPass()
        var total = try await tokens(ledger)
        XCTAssertEqual(total, 0)
        let pass = await store.index(since: .distantPast, timeBudget: 5)
        XCTAssertTrue(pass.reloaded)
        total = try await tokens(ledger)
        XCTAssertEqual(total, 10, "an unchanged log is read again once the ledger lost its position")
    }

    func testASessionIsNotReplacedBeforeWhereItsReaderStarts() async throws {
        let ledger = UsageLedger.inMemory(), recorder = SessionLedger(source: "fixture", ledger: ledger)
        let day = SessionContributions.windowStart(Self.base)
        XCTAssertEqual(SessionContributions.nextDayStart(day.addingTimeInterval(1)), day.addingTimeInterval(86400))
        XCTAssertEqual(SessionContributions.nextDayStart(day), day)
        func event(_ id: String, at hours: Double) -> UsageEvent {
            UsageEvent(timestamp: day.addingTimeInterval(hours * 3600), agentId: "fixture-model:m", tokensIn: 10, tokensOut: 0, eventID: id)
        }
        await recorder.record(files: nil, revision: 1, window: SessionContributions.windowStart(day.addingTimeInterval(-86400))) {
            [("s", [event("a", at: 1), event("b", at: 9)])]
        }
        // A reader whose history starts eight hours into the day, as Cursor's does from local midnight west of UTC.
        await recorder.record(files: nil, revision: 2, window: SessionContributions.windowStart(day.addingTimeInterval(3600),
                                                                                                 readerStart: day.addingTimeInterval(8 * 3600))) {
            [("s", [event("b", at: 9)])]
        }
        let total = try await tokens(ledger)
        XCTAssertEqual(total, 20, "what the reader did not return is not gone")
    }

    func testRunningTotalsKeepTheirFirstCountsAtTheRowsTimeAndDateTheirGrowthWhenRead() async throws {
        let ledger = UsageLedger.inMemory(), files = ListedFiles(paths: ["/state.db"], sessions: ["/state.db": ["h"]])
        // The row began a day before the window, and grows while it is read.
        let window = Self.base.addingTimeInterval(86400), first = Self.base
        func row(_ input: Int) -> [(id: String, events: [UsageEvent])] {
            [("h", [UsageEvent(timestamp: first, agentId: "fixture-model:m", tokensIn: input, tokensOut: 0, eventID: "fixture:r")])]
        }
        func buckets() async throws -> [Date: Int] {
            Dictionary(try await ledger.buckets(since: .distantPast, source: "fixture").map { ($0.start, $0.tokensIn) }, uniquingKeysWith: +)
        }
        let read = window.addingTimeInterval(3 * 3600), period = Date(timeIntervalSince1970: (read.timeIntervalSince1970 / 900).rounded(.down) * 900)
        let firstPeriod = Date(timeIntervalSince1970: (first.timeIntervalSince1970 / 900).rounded(.down) * 900)
        await SessionLedger(source: "fixture", ledger: ledger).record(files: files, revision: 1, window: window, runningTotals: true,
                                                                      now: window.addingTimeInterval(3600)) { row(100) }
        var recorded = try await buckets()
        XCTAssertEqual(recorded, [firstPeriod: 100], "a row first read before the window counts at its own time")
        let recorder = SessionLedger(source: "fixture", ledger: ledger)
        await recorder.record(files: files, revision: 2, window: window, runningTotals: true, now: read) { row(140) }
        recorded = try await buckets()
        XCTAssertEqual(recorded, [firstPeriod: 100, period: 40], "what it gained counts when it was read")
        await recorder.record(files: files, revision: 3, window: window, runningTotals: true, now: read.addingTimeInterval(60)) { row(140) }
        await SessionLedger(source: "fixture", ledger: ledger).record(files: files, revision: 1, window: window, runningTotals: true,
                                                                      now: read.addingTimeInterval(120)) { row(150) }
        recorded = try await buckets()
        XCTAssertEqual(recorded, [firstPeriod: 100, period: 50], "a restart reads on from the totals the ledger kept")
    }

    func testSessionsOfAMissingFileKeepTheirUsageAndRollbacksAreWrittenAgain() async throws {
        let ledger = UsageLedger.inMemory(), recorder = SessionLedger(source: "fixture", ledger: ledger), window = Date(timeIntervalSince1970: 0)
        func session(_ id: String, _ tokens: Int) -> (id: String, events: [UsageEvent]) {
            (id, [UsageEvent(timestamp: Self.base, agentId: "fixture-model:m", tokensIn: tokens, tokensOut: 0, eventID: id)])
        }
        await recorder.record(files: ListedFiles(paths: ["/a", "/b"], sessions: ["/a": ["s1"], "/b": ["s2"]]), revision: 1, window: window) {
            [session("s1", 10), session("s2", 5)]
        }
        var total = try await tokens(ledger)
        XCTAssertEqual(total, 15)
        // `/a` was deleted; `/b` is still listed but has no parse at hand, as after a restart with an unreadable file.
        await recorder.record(files: ListedFiles(paths: ["/b"]), revision: 2, window: window) { [] }
        total = try await tokens(ledger)
        XCTAssertEqual(total, 15, "a deleted file's sessions keep their usage")
        await ledger.beginPass()
        let added = ListedFiles(paths: ["/b", "/c"], sessions: ["/c": ["s3"]])
        await recorder.record(files: added, revision: 3, window: window) { [session("s3", 7)] }
        await ledger.rollBackPass()
        await recorder.record(files: added, revision: 3, window: window) { [session("s3", 7)] }
        total = try await tokens(ledger)
        XCTAssertEqual(total, 22, "what the rolled-back pass wrote is written again")
        await recorder.record(files: ListedFiles(paths: []), revision: 3, window: window) { [] }
        total = try await tokens(ledger)
        XCTAssertEqual(total, 22)
    }
}
