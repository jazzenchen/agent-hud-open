import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// What each surface says about the same quota window: the hover hint, the island row's burn-rate labels and the rate
/// the window's tokens are spent at.
final class QuotaOutlookTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    private let hour: TimeInterval = 3600

    override func setUp() {
        super.setUp()
        L10n.setLanguage(.en)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    /// One window: what is left, when it resets, how long its cycle is, and its burn rate and seconds to exhaustion.
    private struct Window {
        let name: String
        let remaining: Double
        let resetIn: TimeInterval?
        let duration: TimeInterval?
        let insights: (burn: Double?, exhaust: TimeInterval?)?
    }

    private var grid: [Window] {
        [
            .init(name: "untimed", remaining: 50, resetIn: nil, duration: 5 * hour, insights: (10, 5 * hour)),
            .init(name: "exhausted", remaining: 0, resetIn: 2 * hour, duration: 5 * hour, insights: (10, nil)),
            .init(name: "nearlyExhausted", remaining: 0.5, resetIn: 2 * hour, duration: 5 * hour, insights: (10, 3 * 60)),
            .init(name: "noCycle", remaining: 50, resetIn: 2 * hour, duration: nil, insights: (10, 5 * hour)),
            .init(name: "noInsights", remaining: 50, resetIn: 2 * hour, duration: 5 * hour, insights: nil),
            .init(name: "noUsage", remaining: 50, resetIn: 2 * hour, duration: 5 * hour, insights: (0, nil)),
            .init(name: "insufficientData", remaining: 50, resetIn: 2 * hour, duration: 5 * hour, insights: (nil, nil)),
            .init(name: "zeroTimeToExhaust", remaining: 50, resetIn: 2 * hour, duration: 5 * hour, insights: (10, 0)),
            .init(name: "infiniteTime", remaining: 50, resetIn: 2 * hour, duration: 5 * hour, insights: (10, .infinity)),
            .init(name: "exhaustsBeforeReset", remaining: 10, resetIn: 2 * hour, duration: 5 * hour, insights: (10, hour)),
            .init(name: "exhaustsAfterReset", remaining: 50, resetIn: 2 * hour, duration: 5 * hour, insights: (10, 5 * hour)),
            .init(name: "resetPassed", remaining: 50, resetIn: -10 * 60, duration: 5 * hour, insights: (10, 5 * hour)),
            .init(name: "weekly", remaining: 50, resetIn: 3 * 86400, duration: 7 * 86400, insights: (1, 50 * hour)),
            .init(name: "fullWindow", remaining: 100, resetIn: 2 * hour, duration: 5 * hour, insights: (nil, nil)),
        ]
    }

    /// What the surfaces say about one window.
    private struct Outlook: Equatable {
        /// The hover hint.
        var hint: String?
        /// How far ahead the island row puts the window running out before its reset.
        var exhaustsIn: TimeInterval?
        /// The share the island row projects to be used by the reset of a window that lasts past it.
        var projected: Double?
        /// The island row's burn-rate detail.
        var detail: String
        var tokensPerHour: Double?
    }

    @MainActor
    func testEverySurfaceOverTheSameWindows() throws {
        let hour = hour
        /// The weekday and time `interval` from now, as the island row writes a nearby exhaustion in this Mac's time zone.
        func at(_ interval: TimeInterval) -> String { ChartData.weekdayTime(now.addingTimeInterval(interval)) }
        // The row says what the hint says, except that it gives the time a window runs out at before its reset.
        let expected: [(String, Outlook)] = [
            ("untimed", .init(hint: nil, exhaustsIn: nil, projected: nil, detail: "—", tokensPerHour: nil)),
            ("exhausted", .init(hint: "Exhausted", exhaustsIn: nil, projected: nil, detail: "Exhausted", tokensPerHour: 1000)),
            // Half a point left is exhausted.
            ("nearlyExhausted", .init(hint: "Exhausted", exhaustsIn: nil, projected: nil, detail: "Exhausted", tokensPerHour: 1000)),
            ("noCycle", .init(hint: "No estimate", exhaustsIn: nil, projected: nil, detail: "No estimate", tokensPerHour: nil)),
            ("noInsights", .init(hint: "Insufficient data", exhaustsIn: nil, projected: nil, detail: "Insufficient data", tokensPerHour: 1000)),
            ("noUsage", .init(hint: "No usage", exhaustsIn: nil, projected: nil, detail: "No usage", tokensPerHour: 1000)),
            ("insufficientData", .init(hint: "Insufficient data", exhaustsIn: nil, projected: nil, detail: "Insufficient data", tokensPerHour: 1000)),
            ("zeroTimeToExhaust", .init(hint: "Insufficient data", exhaustsIn: nil, projected: nil, detail: "Insufficient data", tokensPerHour: 1000)),
            ("infiniteTime", .init(hint: "Insufficient data", exhaustsIn: nil, projected: nil, detail: "Insufficient data", tokensPerHour: 1000)),
            ("exhaustsBeforeReset", .init(hint: "Exhausts ~1h", exhaustsIn: hour, projected: nil, detail: at(hour), tokensPerHour: 1000)),
            ("exhaustsAfterReset", .init(hint: "70% by reset", exhaustsIn: nil, projected: 70, detail: "70% by reset", tokensPerHour: 1000)),
            // A reading whose reset passed shows no level, so the row gives it no burn rate or token rate.
            ("resetPassed", .init(hint: "Insufficient data", exhaustsIn: nil, projected: nil, detail: "—", tokensPerHour: nil)),
            ("weekly", .init(hint: "Exhausts ~50h", exhaustsIn: 50 * hour, projected: nil, detail: at(50 * hour), tokensPerHour: 1167)),
            ("fullWindow", .init(hint: "Insufficient data", exhaustsIn: nil, projected: nil, detail: "Insufficient data", tokensPerHour: 1000)),
        ]
        XCTAssertEqual(expected.map { $0.0 }, grid.map(\.name))
        try withStore(grid) { store in
            for (window, (name, outlook)) in zip(grid, expected) {
                let snapshot = try XCTUnwrap(store.report?.snapshot(for: window.name))
                let insights = store.report?.insightsByAgent[window.name]
                let metrics = try self.metrics(window.name, in: store)
                XCTAssertEqual(Outlook(hint: QuotaForecast.hint(snapshot: snapshot, insights: insights, now: now),
                                       exhaustsIn: metrics.exhaustsBeforeReset?.timeIntervalSince(now),
                                       projected: metrics.projectedAtReset,
                                       detail: metrics.detail(.burnRate, isLoading: false),
                                       tokensPerHour: store.quotaTokensPerHour(for: window.name)), outlook, name)
            }
            // The burn rate wears the warning colour, and its projection fills the track, for a window that ran out or runs
            // out before its reset.
            XCTAssertEqual(try grid.map(\.name).filter { try metrics($0, in: store).runsOut },
                           ["exhausted", "nearlyExhausted", "exhaustsBeforeReset", "weekly"])
        }
    }

    /// The row's other measures on one window of the grid, and a window that runs out a week or more ahead, which the
    /// row gives as a date rather than a weekday.
    @MainActor
    func testTheRowsOtherMeasuresAndAFarExhaustion() throws {
        let far = Window(name: "monthly", remaining: 50, resetIn: 20 * 86400, duration: 30 * 86400, insights: (0.2, 10 * 86400))
        try withStore([grid[9], far]) { store in
            let row = try metrics("exhaustsBeforeReset", in: store)
            XCTAssertEqual(IslandQuotaMetric.allCases.map { row.value($0) }, ["90%", "10.0%/h", "1.0k/h"])
            XCTAssertEqual(IslandQuotaMetric.allCases.map { row.detail($0, isLoading: false) }, ["2h 00m", ChartData.weekdayTime(now.addingTimeInterval(hour)), "24k / day"])
            let date = now.addingTimeInterval(10 * 86400)
            XCTAssertEqual(try metrics("monthly", in: store).exhaustionTimeLabel, date.formatted(Date.FormatStyle().month(.abbreviated).day()
                .hour(.twoDigits(amPM: .omitted)).minute(.twoDigits).locale(Locale(identifier: "en_GB"))))
        }
    }

    /// A window whose reading shows no level enters no calculation: its row keeps the share used and its reset, but shows
    /// no burn rate, projection or token rate, and neither the island nor the menu gives it a hint.
    @MainActor
    func testAWindowWhoseReadingShowsNoLevelHasNoForecast() throws {
        try withStore([grid[9]], quotaNotices: ["Codex": "Codex quota could not be read"]) { store in
            let row = try metrics("exhaustsBeforeReset", in: store)
            XCTAssertNil(row.row.level)
            XCTAssertEqual(IslandQuotaMetric.allCases.map { row.value($0) }, ["90%", nil, nil])
            XCTAssertEqual(IslandQuotaMetric.allCases.map { row.detail($0, isLoading: false) }, ["2h 00m", "—", "—"])
            XCTAssertNil(row.exhaustsBeforeReset)
            XCTAssertNil(row.projectedAtReset)
            XCTAssertNil(store.quotaForecastHint(for: "exhaustsBeforeReset"))
            XCTAssertNil(store.quotaTokensPerHour(for: "exhaustsBeforeReset"))
            XCTAssertNil(store.maxUsedPct)
        }
    }

    /// An explicit unknown reading is distinct from both known zero usage and a row awaiting its first reading.
    @MainActor
    func testUnknownQuotaShowsNAWhileKnownZeroAndInitialRowsKeepTheirMeaning() throws {
        let suite = "QuotaOutlookTests.unknown.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let agent = AgentDescriptor(id: "grok", vendor: "Grok", model: "Weekly usage limit", source: "Test", enabled: true)
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: [agent]))
        store.replace(report: UsageReport(generatedAt: now,
            snapshots: [.init(agentId: agent.id, remainingPct: nil, resetAt: now.addingTimeInterval(2 * hour), updatedAt: now)],
            sessions: [], discoveredAgents: [agent],
            insightsByAgent: [agent.id: .init(burnRatePctPerHour: 10, timeToExhaust: hour, weeklyCapHits: 0,
                weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil)]))
        store.now = now
        let unknown = try metrics(agent.id, in: store)
        XCTAssertEqual(IslandQuotaMetric.allCases.map { unknown.value($0) }, ["N/A", nil, nil])
        XCTAssertEqual(unknown.detail(.quota, isLoading: false), "2h 00m")
        XCTAssertNil(unknown.row.usedPct)
        XCTAssertNil(unknown.row.level)
        XCTAssertNil(unknown.projectedAtReset)

        store.replace(report: UsageReport(generatedAt: now,
            snapshots: [.init(agentId: agent.id, remainingPct: 100, updatedAt: now)], sessions: [], discoveredAgents: [agent]))
        XCTAssertEqual(try metrics(agent.id, in: store).value(.quota), "0%")
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: [], discoveredAgents: [agent]))
        XCTAssertEqual(try metrics(agent.id, in: store).value(.quota), "—")
    }

    /// A store showing `windows` at `now`, each counting the tokens of one model spent before, around and after now.
    @MainActor
    private func withStore(_ windows: [Window], quotaNotices: [String: String] = [:], _ body: (UsageStore) throws -> Void) throws {
        let suite = "QuotaOutlookTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let agents = windows.map { AgentDescriptor(id: $0.name, vendor: "Codex", model: $0.name, source: "", enabled: true) }
        let usage = [(-6 * hour, "m", 4000), (-3 * hour, "m", 2000), (-hour, "m", 1000), (60, "m", 500), (-hour, "other", 9000)].map {
            UsageBucket(start: now.addingTimeInterval($0.0), agentId: $0.1, tokensIn: $0.2, tokensOut: 0)
        }
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: agents))
        store.replace(report: UsageReport(
            generatedAt: now,
            snapshots: windows.map {
                UsageSnapshot(agentId: $0.name, remainingPct: $0.remaining, resetAt: $0.resetIn.map(now.addingTimeInterval),
                              windowDuration: $0.duration, updatedAt: now)
            },
            sessions: [], discoveredAgents: agents, usage: usage,
            insightsByAgent: Dictionary(uniqueKeysWithValues: windows.compactMap { window in
                window.insights.map {
                    (window.name, UsageInsights(burnRatePctPerHour: $0.burn, timeToExhaust: $0.exhaust, weeklyCapHits: 0, weeklyWaitTotal: 0,
                                                weeklyWaitLongest: 0, weeklyWaitLongestAt: nil))
                }
            }),
            sourceNotices: quotaNotices, quotaNotices: quotaNotices,
            consumerIdsByQuota: Dictionary(uniqueKeysWithValues: windows.map { ($0.name, ["m"]) })))
        store.now = now
        try body(store)
    }

    /// The island row's measures of one window, built from the store as the panel builds them.
    @MainActor
    private func metrics(_ id: String, in store: UsageStore) throws -> QuotaRowMetrics {
        QuotaRowMetrics(row: try XCTUnwrap(store.row(for: id)), insights: store.report?.insightsByAgent[id],
                        outlook: store.view.outlook(for: id), now: store.now, tokensPerHour: store.quotaTokensPerHour(for: id))
    }
}
