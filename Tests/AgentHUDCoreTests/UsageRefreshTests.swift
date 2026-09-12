import XCTest
@testable import AgentHUDCore

final class UsageRefreshTests: XCTestCase, @unchecked Sendable {
    @MainActor
    func testSlowAccountQueryDoesNotBlockLocalRefreshOrStartDuplicateQueries() async throws {
        let gate = AsyncStream<Void>.makeStream()
        defer { gate.continuation.finish() }
        let started = expectation(description: "account request started")
        started.assertForOverFulfill = true
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let codex = CodexUsageProvider(readLimits: {
            started.fulfill()
            for await _ in gate.stream { break }
            throw UsageProviderError("account offline")
        }, transcripts: CodexTranscriptStore(roots: [directory]), history: QuotaHistoryStore(fileURL: nil))
        let combined = CombinedUsageProvider([.init("Codex", codex), .init("Local", DemoUsageProvider())])
        let suite = "UsageRefreshTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: RetainedUsageProvider(provider: combined), settings: SettingsStore(defaults: defaults))
        defer { store.stop() }
        let local = expectation(description: "two local polls finish while account query is pending")
        let refresh = Task { @MainActor in
            await store.refresh()
            await store.refresh()
            local.fulfill()
        }
        await fulfillment(of: [started, local], timeout: 2)
        gate.continuation.finish()
        await refresh.value
        XCTAssertFalse(store.sessions.isEmpty)
        XCTAssertTrue(store.hasLiveSession)
    }

    func testAccountResultAppearsOnNextLocalPollWithItsObservationTime() async throws {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let gate = AsyncStream<Void>.makeStream()
        defer { gate.continuation.finish() }
        let started = expectation(description: "quota started")
        let provider = AdditionalUsageProvider(source: .grok, readQuota: {
            started.fulfill()
            for await _ in gate.stream { break }
            return ProviderQuota(windows: [.init(id: "grok", label: "Credits", remaining: 80)])
        }, readSessions: { _ in .init() }, history: QuotaHistoryStore(fileURL: nil), clock: { now })
        let request = Task { await provider.refreshAccountUsage(historyHours: 24) }
        await fulfillment(of: [started], timeout: 2)
        let local = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertTrue(local.snapshots.isEmpty)
        gate.continuation.finish()
        await request.value
        let updated = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(updated.snapshots.first?.remainingPct, 80)
        XCTAssertEqual(updated.snapshots.first?.updatedAt, now)
    }

    func testRemoteSessionStatisticsDoNotBlockLocalHooks() async throws {
        let gate = AsyncStream<Void>.makeStream()
        defer { gate.continuation.finish() }
        let started = expectation(description: "remote session request started")
        let completion = SessionCompletion(sessionID: "cursor:s", vendor: "Cursor", turnID: "t",
            task: "Task", model: "model", startedAt: nil, completedAt: Date())
        let provider = AdditionalUsageProvider(source: .cursor, readQuota: { ProviderQuota() },
            readSessions: { _ in ProviderSessions() }, history: QuotaHistoryStore(fileURL: nil),
            readCompletions: { _ in [completion] }, refreshSessions: { hours in
                XCTAssertEqual(hours, 169)
                started.fulfill()
                for await _ in gate.stream { break }
            })
        let request = Task { await provider.refreshAccountUsage(historyHours: 169) }
        await fulfillment(of: [started], timeout: 2)
        let local = try await provider.fetchUsage(agents: [], historyHours: 169)
        XCTAssertEqual(local.completions, [completion])
        gate.continuation.finish()
        await request.value
    }
}
