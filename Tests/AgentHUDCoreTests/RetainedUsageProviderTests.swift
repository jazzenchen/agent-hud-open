import XCTest
@testable import AgentHUDCore

final class RetainedUsageProviderTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testPartialFailureKeepsQuotaBalanceCreditsAndObservationTimes() async throws {
        let good = report(at: now, remaining: 64, balance: 12, credits: 2)
        let partial = report(at: now.addingTimeInterval(3600), remaining: nil, balance: nil, credits: nil)
        let provider = RetainedUsageProvider(provider: SequenceProvider([good, partial]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let result = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(result.snapshots, good.snapshots)
        XCTAssertEqual(result.billing.first?.balances, good.billing.first?.balances)
        XCTAssertEqual(result.billing.first?.updatedAt, now)
        XCTAssertEqual(result.codexResetCredits?.availableCount, 2)
        XCTAssertEqual(result.codexResetCreditsObservedAt, now)
        XCTAssertEqual(result.subscriptions["Codex"], "Pro")
    }

    func testSuccessfulZeroReplacesPreviousValues() async throws {
        let provider = RetainedUsageProvider(provider: SequenceProvider([
            report(at: now, remaining: 64, balance: 12, credits: 2),
            report(at: now.addingTimeInterval(3600), remaining: 0, balance: 0, credits: 0)
        ]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let result = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(result.snapshots.first?.remainingPct, 0)
        XCTAssertEqual(result.snapshots.first?.updatedAt, now.addingTimeInterval(3600))
        XCTAssertEqual(result.billing.first?.balances.first?.total, 0)
        XCTAssertEqual(result.codexResetCredits?.availableCount, 0)
    }

    func testOfflineRestartRestoresTheLastSuccessfulReport() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("report.json")
        let good = report(at: now, remaining: 64, balance: 12, credits: 2)
        let first = RetainedUsageProvider(provider: SequenceProvider([good]), cacheURL: file)
        _ = try await first.fetchUsage(agents: [], historyHours: 24)
        let restarted = RetainedUsageProvider(provider: SequenceProvider([]), cacheURL: file)
        XCTAssertEqual(restarted.initialReport, good)
        do {
            _ = try await restarted.fetchUsage(agents: [], historyHours: 24)
            XCTFail("A cached report must not turn a failed refresh into success")
        } catch { XCTAssertEqual(error.localizedDescription, "offline") }
        XCTAssertEqual(restarted.initialReport, good)
    }

    func testOfflineRestartPreservesCachedPlansAndTokenHistory() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("report.json")
        let pool = OpenAgentCredentials.credential(.kimi, token: "fixture-key", client: "Pi").pool
        let id = pool.windowID("weekly")
        let usage = UsageEvent(timestamp: now, agentId: "pi-model", tokensIn: 10, tokensOut: 20)
        let saved = UsageReport(generatedAt: now, snapshots: [.init(agentId: id, remainingPct: 90, updatedAt: now)],
            sessions: [], history: [], activity: .empty, insights: .empty,
            discoveredAgents: [.init(id: id, vendor: "Kimi", model: "7d", source: "Pi", enabled: true, billingPool: pool)],
            consumers: [.init(id: "pi-model", vendor: "Pi", model: "model", source: "local", enabled: true)],
            consumption: [usage], subscriptions: [pool.id: "Allegretto"],
            services: [.init(client: "Pi", provider: "Kimi", product: .plan, accountID: pool.id)])
        try JSONEncoder().encode(saved).write(to: file)
        let restarted = RetainedUsageProvider(provider: SequenceProvider([]), cacheURL: file)
        XCTAssertEqual(restarted.initialReport, saved)
    }

    @MainActor
    func testFailedRefreshKeepsVisibleReadingsAndExposesTheError() async {
        let suite = "RetainedUsageTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let good = report(at: now, remaining: 64, balance: 12, credits: 2)
        let provider = RetainedUsageProvider(provider: SequenceProvider([good]))
        let store = UsageStore(provider: provider, settings: SettingsStore(defaults: defaults))
        await store.refresh()
        await store.refresh()
        XCTAssertEqual(store.report, good)
        XCTAssertEqual(store.lastError, "offline")
        store.stop()
    }

    @MainActor
    func testRetainedSessionDoesNotBecomeFreshWhenAnotherSourceUpdates() async throws {
        let current = Date(), old = current.addingTimeInterval(-86400)
        let agent = AgentDescriptor(id: "pi-model:test", vendor: "Pi", model: "test", source: "local", enabled: true)
        let session = LiveSession(id: "pi:old", agentId: agent.id, task: "old task", terminal: nil,
            startedAt: old, pctOfWindow: nil, tokensIn: 10, tokensOut: 2, observedAt: old)
        let previous = UsageReport(generatedAt: old, snapshots: [], sessions: [session], history: [],
            activity: .empty, insights: .empty, consumers: [agent])
        let incoming = UsageReport(generatedAt: current, snapshots: [], sessions: [], history: [],
            activity: .empty, insights: .empty, sourceNotices: ["Pi": "local read failed"])
        let provider = RetainedUsageProvider(provider: SequenceProvider([previous, incoming]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let retained = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(retained.sessions, [session])
        XCTAssertNil(retained.sessions.first?.endedAt, "Failure is not an observed completion")
        XCTAssertEqual(try JSONDecoder().decode(UsageReport.self, from: JSONEncoder().encode(retained)).sessions, [session])
        let suite = "StaleSessionTests.\(UUID())", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: provider, settings: SettingsStore(defaults: defaults))
        store.replace(report: retained)
        XCTAssertEqual(store.sessions.count, 1)
        XCTAssertFalse(store.hasLiveSession)
        XCTAssertEqual(store.sessionStatusLabel(session), L10n.text("状态待更新", "Status out of date"))
    }

    private func report(at date: Date, remaining: Double?, balance: Decimal?, credits: Int?) -> UsageReport {
        let descriptor = AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: "", enabled: true)
        return UsageReport(generatedAt: date,
            snapshots: remaining.map { [.init(agentId: "codex", remainingPct: $0, resetAt: now.addingTimeInterval(60), updatedAt: date)] } ?? [],
            sessions: [], history: [], activity: UsageAnalytics.activityGrid(usage: [], since: date, calendar: .current),
            insights: .empty, discoveredAgents: [descriptor], subscriptions: remaining == nil ? [:] : ["Codex": "Pro"],
            sourceNotices: remaining == nil ? ["Codex": "offline"] : [:],
            billing: [.init(vendor: "DeepSeek", balances: balance.map { [.init(currency: "CNY", total: $0, granted: 0, toppedUp: $0)] } ?? [],
                isAvailable: balance.map { $0 > 0 }, updatedAt: balance == nil ? nil : date, costs: [], notice: nil)],
            codexResetCredits: credits.map { .init(availableCount: $0, credits: nil) },
            codexResetCreditsObservedAt: credits == nil ? nil : date)
    }
}

private actor SequenceProvider: UsageProvider {
    var reports: [UsageReport]
    init(_ reports: [UsageReport]) { self.reports = reports }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) throws -> UsageReport {
        guard !reports.isEmpty else { throw UsageProviderError("offline") }
        return reports.removeFirst()
    }
}
