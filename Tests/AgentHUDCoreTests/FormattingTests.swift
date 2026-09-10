import XCTest
@testable import AgentHUDCore

final class CountdownTests: XCTestCase {
    override func setUp() {
        super.setUp()
        L10n.setLanguage(.zhHans)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    func testHoursAndMinutes() {
        XCTAssertEqual(Countdown.format(2 * 3600 + 14 * 60), "2h 14m")
        XCTAssertEqual(Countdown.format(4 * 3600 + 2 * 60), "4h 02m")
        XCTAssertEqual(Countdown.format(6 * 3600 + 40 * 60), "6h 40m")
    }

    func testMinutesOnlyUnderAnHour() {
        XCTAssertEqual(Countdown.format(51 * 60), "51m")
        XCTAssertEqual(Countdown.format(59 * 60 + 30), "59m")
        XCTAssertEqual(Countdown.format(0), "0m")
    }

    func testNegativeClampsToZero() {
        XCTAssertEqual(Countdown.format(-500), "0m")
    }

    func testCompactDropsSpaces() {
        XCTAssertEqual(Countdown.compact(2 * 3600 + 14 * 60), "2h14m")
        XCTAssertEqual(Countdown.compact(51 * 60), "51m")
    }

    func testUntilHandlesUnknown() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        XCTAssertEqual(Countdown.until(nil, now: now), "—")
        XCTAssertEqual(Countdown.until(now.addingTimeInterval(3600), now: now), "1h 00m")
    }

    func testUpdatedLabel() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        XCTAssertEqual(Countdown.updatedLabel(since: now.addingTimeInterval(-10), now: now), "刚刚更新")
        XCTAssertEqual(Countdown.updatedLabel(since: now.addingTimeInterval(-120), now: now), "2 分钟前更新")
        XCTAssertEqual(Countdown.updatedLabel(since: now.addingTimeInterval(-7200), now: now), "2 小时前更新")
        XCTAssertEqual(Countdown.updatedLabel(since: nil, now: now), "尚未更新")
    }

    func testSessionLabels() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        let live = LiveSession(id: "a", agentId: "x", task: "t", terminal: nil, startedAt: now.addingTimeInterval(-27 * 60), pctOfWindow: 1, tokensIn: 0, tokensOut: 0)
        XCTAssertEqual(Countdown.sessionLabel(live, now: now), "27m 进行中")
        let ended = LiveSession(id: "b", agentId: "x", task: "t", terminal: nil, startedAt: now.addingTimeInterval(-7200), endedAt: now.addingTimeInterval(-51 * 60), pctOfWindow: 1, tokensIn: 0, tokensOut: 0)
        XCTAssertEqual(Countdown.sessionLabel(ended, now: now), "结束于 51m 前")
        let endedHoursAgo = LiveSession(id: "c", agentId: "x", task: "t", terminal: nil, startedAt: now.addingTimeInterval(-9000), endedAt: now.addingTimeInterval(-7200), pctOfWindow: 1, tokensIn: 0, tokensOut: 0)
        XCTAssertEqual(Countdown.sessionLabel(endedHoursAgo, now: now), "结束于 2h 前")
    }

    func testFormatRough() {
        XCTAssertEqual(Countdown.formatRough(7200), "2h")
        XCTAssertEqual(Countdown.formatRough(7260), "2h 01m")
        XCTAssertEqual(Countdown.formatRough(1800), "30m")
    }
}

final class TokenFormatTests: XCTestCase {
    func testShort() {
        XCTAssertEqual(TokenFormat.short(48_000), "48k")
        XCTAssertEqual(TokenFormat.short(1_260), "1.3k")
        XCTAssertEqual(TokenFormat.short(950), "950")
        XCTAssertEqual(TokenFormat.short(2_400_000), "2.4M")
        XCTAssertEqual(TokenFormat.short(84_000), "84k")
    }

    func testInOut() {
        XCTAssertEqual(TokenFormat.inOut(in: 48_000, out: 12_000), "48k ↓ 12k ↑")
    }

    func testPercent() {
        XCTAssertEqual(TokenFormat.percent(72), "72%")
        XCTAssertEqual(TokenFormat.percent(6.6), "7%")
        XCTAssertEqual(TokenFormat.percent1(6.2), "6.2%")
    }
}

final class ResetLabelTests: XCTestCase {
    override func setUp() {
        super.setUp()
        L10n.setLanguage(.zhHans)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    func testCountdownInsideADayWeekdayBeyond() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Shanghai")!
        let now = calendar.date(from: DateComponents(year: 2026, month: 9, day: 7, hour: 15, minute: 0))! // Monday
        XCTAssertEqual(Countdown.resetLabel(now.addingTimeInterval(3 * 3600 + 10 * 60), now: now, calendar: calendar), "3h 10m")
        XCTAssertEqual(Countdown.resetLabelCompact(now.addingTimeInterval(3 * 3600 + 10 * 60), now: now, calendar: calendar), "3h10m")
        let sunday = calendar.date(from: DateComponents(year: 2026, month: 9, day: 13, hour: 2, minute: 0))!
        XCTAssertEqual(Countdown.resetLabel(sunday, now: now, calendar: calendar), "周日 02:00")
        XCTAssertEqual(Countdown.resetLabel(nil, now: now, calendar: calendar), "—")
    }

    func testQuotaWindowRowsFollowUsageScreenOrder() {
        let usage = ClaudeUsage(
            fiveHour: ClaudeUsageWindow(utilizationPct: 23, resetsAt: nil),
            sevenDay: ClaudeUsageWindow(utilizationPct: 71, resetsAt: nil),
            modelWeekly: ["fable": ClaudeUsageWindow(utilizationPct: 45, resetsAt: nil)]
        )
        XCTAssertEqual(usage.rows.map(\.id), ["claude-session", "claude-weekly", "claude-weekly-fable"])
        XCTAssertEqual(usage.rows.map(\.label), ["window.session", "window.weekly", "window.weekly.Fable"], "labels are persisted as language-neutral keys")
        XCTAssertEqual(usage.rows.map { L10n.modelLabel($0.label) }, ["当前会话 · 5h", "本周 · 全部模型", "本周 · Fable"])
        XCTAssertEqual(usage.rows.map { Int($0.window.remainingPct) }, [77, 29, 55])
        XCTAssertEqual(usage.rows.first?.descriptor.vendor, "Claude")
    }
}
