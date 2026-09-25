import XCTest
@testable import AgentHUDCore

/// Sources are read when they signal new data, and only the signalled sources are read.
final class CollectionSignalTests: XCTestCase, @unchecked Sendable {
    private actor Source: UsageProvider {
        nonisolated let watchedDirectories: [URL]?
        private let sessions: [LiveSession]
        private(set) var fetches = 0
        private(set) var changedPaths: [Set<String>?] = []
        init(directory: URL?, sessions: [LiveSession] = []) {
            watchedDirectories = directory.map { [$0] }
            self.sessions = sessions
        }
        nonisolated var accountRefreshSteps: [AccountRefreshStep] { [] }
        func fileChanges(_ paths: Set<String>?) async { changedPaths.append(paths) }
        func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
            fetches += 1
            return UsageReport(generatedAt: Date(), snapshots: [], sessions: sessions)
        }
    }

    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-signals-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url.resolvingSymlinksInPath()
    }

    /// File events for directories created just before the stream starts can still arrive; wait them out first.
    @MainActor
    private func start(_ provider: any UsageProvider) async throws -> UsageStore {
        try await Task.sleep(for: .seconds(1.5))
        let suite = "CollectionSignalTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        let store = UsageStore(provider: provider, settings: SettingsStore(defaults: defaults))
        addTeardownBlock { @MainActor in
            store.stop()
            defaults.removePersistentDomain(forName: suite)
        }
        store.start()
        return store
    }

    @MainActor
    private func wait(timeout: TimeInterval = 8, until condition: @MainActor () async -> Bool) async throws -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if await condition() { return true }
            try await Task.sleep(for: .milliseconds(100))
        }
        return await condition()
    }

    @MainActor
    func testAFileChangeReadsOnlyTheSourceThatOwnsIt() async throws {
        let first = try directory(), second = try directory()
        let a = Source(directory: first), b = Source(directory: second)
        let store = try await start(CombinedUsageProvider([.init("A", a), .init("B", b)]))
        let initial = try await wait { let x = await a.fetches; let y = await b.fetches; return x == 1 && y == 1 }
        XCTAssertTrue(initial, "the first pass reads every source")
        XCTAssertNotNil(store.report)
        try await Task.sleep(for: .seconds(1.5))
        let changedFile = first.appendingPathComponent("session.jsonl")
        try Data("{}\n".utf8).write(to: changedFile)
        let read = try await wait { await a.fetches >= 2 }
        XCTAssertTrue(read, "a change under A's directory reads A")
        let signalled = await a.changedPaths
        XCTAssertTrue(signalled.contains { paths in
            paths?.contains(where: { $0.hasSuffix("/session.jsonl") }) == true
        }, "the collector hands the affected path to the source before it reads")
        let others = await b.fetches
        XCTAssertEqual(others, 1, "B keeps its last result")
    }

    @MainActor
    func testQuietSourcesAreNotReadAgain() async throws {
        let a = Source(directory: try directory())
        _ = try await start(CombinedUsageProvider([.init("A", a)]))
        let initial = try await wait { await a.fetches == 1 }
        XCTAssertTrue(initial)
        try await Task.sleep(for: .seconds(3))
        let fetches = await a.fetches
        XCTAssertEqual(fetches, 1, "no signal, no read: there is no polling interval for a watched source")
    }

    @MainActor
    func testASourceIsReadAgainWhenItsActivityAges() async throws {
        let observed = Date().addingTimeInterval(-UsageRefresh.liveThreshold + 2)
        let live = LiveSession(id: "s", agentId: "a-model:x", task: "Task", terminal: nil, startedAt: observed.addingTimeInterval(-60),
                               pctOfWindow: nil, tokensIn: 1, tokensOut: 1, observedAt: observed)
        let a = Source(directory: try directory(), sessions: [live]), b = Source(directory: try directory())
        _ = try await start(CombinedUsageProvider([.init("A", a), .init("B", b)]))
        let initial = try await wait { let x = await a.fetches; let y = await b.fetches; return x == 1 && y == 1 }
        XCTAssertTrue(initial)
        let aged = try await wait { await a.fetches >= 2 }
        XCTAssertTrue(aged, "A's live session reaches the live threshold, so A is read again")
        let others = await b.fetches
        XCTAssertEqual(others, 1)
    }

    @MainActor
    func testASourceWithoutDirectoriesIsPolled() async throws {
        let a = Source(directory: nil), b = Source(directory: try directory())
        _ = try await start(CombinedUsageProvider([.init("A", a), .init("B", b)]))
        let polled = try await wait(timeout: 12) { await a.fetches >= 2 }
        XCTAssertTrue(polled, "a source that cannot name its directories is read every poll interval")
        let others = await b.fetches
        XCTAssertEqual(others, 1, "polling one source does not read the others")
    }

    func testNamedSourcesAreReadAndTheOthersKeepTheirLastResult() async throws {
        let a = Source(directory: nil), b = Source(directory: nil)
        let provider = CombinedUsageProvider([.init("A", a), .init("B", b)])
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        _ = try await provider.fetchUsage(agents: [], historyHours: 24, sources: ["A"])
        let (first, second) = (await a.fetches, await b.fetches)
        XCTAssertEqual(first, 2)
        XCTAssertEqual(second, 1)
        _ = try await provider.fetchUsage(agents: [], historyHours: 48, sources: ["A"])
        let reread = await b.fetches
        XCTAssertEqual(reread, 2, "a result read for other hours is not reused")
    }

    func testChangesNameWhatANewReportChanged() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let session = LiveSession(id: "s", agentId: "claude-model:opus", task: "Task", terminal: nil, startedAt: now, pctOfWindow: nil,
                                  tokensIn: 1, tokensOut: 1, observedAt: now)
        let base = UsageReport(generatedAt: now, snapshots: [UsageSnapshot(agentId: "claude", remainingPct: 80, updatedAt: now)],
                               sessions: [session], usage: [UsageBucket(start: now, agentId: "claude-model:opus", tokensIn: 1, tokensOut: 0)])
        XCTAssertFalse(UsageChanges(from: nil, to: base).isEmpty)
        XCTAssertTrue(UsageChanges(from: base, to: base).isEmpty)
        let checkedAgain = LiveSession(id: "s", agentId: "claude-model:opus", task: "Task", terminal: nil, startedAt: now, pctOfWindow: nil,
                                       tokensIn: 1, tokensOut: 1, observedAt: now.addingTimeInterval(5))
        let rechecked = UsageReport(generatedAt: now.addingTimeInterval(5), snapshots: base.snapshots, sessions: [checkedAgain], usage: base.usage)
        XCTAssertTrue(UsageChanges(from: base, to: rechecked).sessions.isEmpty, "checking a session again is not a change")
        let grown = LiveSession(id: "s", agentId: "claude-model:opus", task: "Task", terminal: nil, startedAt: now, pctOfWindow: nil,
                                tokensIn: 5, tokensOut: 1, observedAt: now)
        let later = UsageReport(generatedAt: now.addingTimeInterval(5), snapshots: base.snapshots, sessions: [grown],
                                usage: [UsageBucket(start: now, agentId: "claude-model:opus", tokensIn: 5, tokensOut: 0)])
        let changes = UsageChanges(from: base, to: later)
        XCTAssertTrue(changes.usage)
        XCTAssertFalse(changes.readings, "the same quota reading is not a change")
        XCTAssertEqual(changes.sessions, ["s"])
        let emptied = UsageReport(generatedAt: now, snapshots: [UsageSnapshot(agentId: "claude", remainingPct: 70, updatedAt: now)], sessions: [])
        let dropped = UsageChanges(from: later, to: emptied)
        XCTAssertTrue(dropped.readings)
        XCTAssertEqual(dropped.sessions, ["s"], "a removed session is a change")
    }

    @MainActor
    func testObserversReceiveChangesUntilCancelled() {
        let suite = "CollectionSignalTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: Source(directory: nil), settings: SettingsStore(defaults: defaults))
        var received: [UsageChanges] = []
        let observation = store.observeChanges { received.append($0) }
        let now = Date()
        let report = UsageReport(generatedAt: now, snapshots: [UsageSnapshot(agentId: "claude", remainingPct: 80, updatedAt: now)], sessions: [])
        store.replace(report: report)
        store.replace(report: report)
        XCTAssertEqual(received.count, 1, "an identical report changes nothing")
        XCTAssertTrue(received.first?.readings == true)
        observation.cancel()
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: []))
        XCTAssertEqual(received.count, 1)
    }
}
