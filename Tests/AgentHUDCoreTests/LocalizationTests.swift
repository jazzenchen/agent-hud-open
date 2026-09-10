import XCTest
import Observation
@testable import AgentHUDCore

final class LocalizationTests: XCTestCase {
    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    func testSystemLanguageFollowsTheFirstPreferredLanguage() {
        XCTAssertEqual(L10n.systemLanguage(preferred: ["zh-Hans-CN", "en-US"]), .zhHans)
        XCTAssertEqual(L10n.systemLanguage(preferred: ["zh-Hant-TW"]), .zhHans, "traditional Chinese users get Chinese rather than English")
        XCTAssertEqual(L10n.systemLanguage(preferred: ["en-US", "zh-Hans-CN"]), .en)
        XCTAssertEqual(L10n.systemLanguage(preferred: ["ja-JP"]), .en, "unsupported languages fall back to English")
        XCTAssertEqual(L10n.systemLanguage(preferred: []), .en)
    }

    func testExplicitLanguageOverridesTheSystem() {
        L10n.setLanguage(.en)
        XCTAssertEqual(L10n.resolved, .en)
        XCTAssertEqual(L10n.text("中文", "English"), "English")
        L10n.setLanguage(.zhHans)
        XCTAssertEqual(L10n.resolved, .zhHans)
        XCTAssertEqual(L10n.text("中文", "English"), "中文")
        L10n.setLanguage(.system)
        XCTAssertEqual(L10n.language, .system)
        XCTAssertEqual(L10n.resolved, L10n.systemLanguage())
    }

    func testQuotaWindowKeysRenderInBothLanguages() {
        L10n.setLanguage(.zhHans)
        XCTAssertEqual(L10n.modelLabel(L10n.windowSession), "当前会话 · 5h")
        XCTAssertEqual(L10n.modelLabel(L10n.windowWeekly), "本周 · 全部模型")
        XCTAssertEqual(L10n.modelLabel(L10n.windowWeeklyPrefix + "Fable"), "本周 · Fable")
        XCTAssertEqual(L10n.shortModelLabel(L10n.windowSession), "当前会话")
        XCTAssertEqual(L10n.shortModelLabel(L10n.windowWeeklyPrefix + "Fable"), "本周 Fable")
        L10n.setLanguage(.en)
        XCTAssertEqual(L10n.modelLabel(L10n.windowSession), "Session · 5h")
        XCTAssertEqual(L10n.modelLabel(L10n.windowWeekly), "Weekly · all models")
        XCTAssertEqual(L10n.modelLabel(L10n.windowWeeklyPrefix + "Fable"), "Weekly · Fable")
        XCTAssertEqual(L10n.shortModelLabel(L10n.windowWeekly), "Weekly")
    }

    func testRealModelNamesPassThrough() {
        L10n.setLanguage(.en)
        XCTAssertEqual(L10n.modelLabel("Opus 4.5"), "Opus 4.5")
        XCTAssertEqual(L10n.shortModelLabel("Opus 4.5"), "Opus")
        XCTAssertEqual(L10n.shortModelLabel("GPT-5"), "GPT-5")
    }

    func testSourceKeysRenderAsText() {
        L10n.setLanguage(.zhHans)
        XCTAssertEqual(L10n.sourceLabel(L10n.sourceClaudeCode), "Claude Code")
        XCTAssertEqual(L10n.sourceLabel(L10n.sourceNotConnected), "未连接")
        XCTAssertEqual(L10n.sourceLabel(L10n.sourceBrowserAuth), "需浏览器授权")
        L10n.setLanguage(.en)
        XCTAssertEqual(L10n.sourceLabel(L10n.sourceNotConnected), "Not connected")
        XCTAssertEqual(L10n.sourceLabel("custom-source"), "custom-source", "unknown keys are shown verbatim")
    }

    func testWeekdayNamesMatchCalendarOrder() {
        L10n.setLanguage(.zhHans)
        XCTAssertEqual(L10n.weekdayNames.first, "周日")
        XCTAssertEqual(L10n.weekdayNamesMondayFirst, ["周一", "周二", "周三", "周四", "周五", "周六", "周日"])
        L10n.setLanguage(.en)
        XCTAssertEqual(L10n.weekdayNames, ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"])
        XCTAssertEqual(L10n.weekdayNamesMondayFirst.last, "Sun")
    }

    func testLocalizedFormattersSwitchLanguage() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        L10n.setLanguage(.en)
        XCTAssertEqual(Countdown.updatedLabel(since: nil, now: now), "Not updated yet")
        XCTAssertEqual(StatsRange.allCases.map(\.label), ["5 h", "24 h", "7 days"])
        L10n.setLanguage(.zhHans)
        XCTAssertEqual(Countdown.updatedLabel(since: nil, now: now), "尚未更新")
        XCTAssertEqual(StatsRange.allCases.map(\.label), ["5 小时", "24 小时", "7 天"])
    }

    func testLanguageSettingRoundTripsThroughJSON() throws {
        var settings = Settings()
        settings.language = .en
        let data = try JSONEncoder().encode(settings)
        let decoded = try JSONDecoder().decode(Settings.self, from: data)
        XCTAssertEqual(decoded.language, .en)
        let legacy = try JSONDecoder().decode(Settings.self, from: Data("{}".utf8))
        XCTAssertEqual(legacy.language, .system, "settings saved before the language option default to following the system")
    }

    @MainActor
    func testLanguageIsAppliedBeforeSettingsObserversRender() {
        let suite = "AgentHUDLanguageTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = SettingsStore(defaults: defaults)
        store.update { $0.language = .en }
        L10n.setLanguage(.en)

        let languages: [(AppLanguage, String)] = [
            (.zhHans, "中文"),
            (.en, "English"),
            (.system, L10n.systemLanguage() == .zhHans ? "中文" : "English"),
        ]
        for (language, text) in languages {
            let changed = expectation(description: "settings notified for \(language)")
            withObservationTracking {
                _ = store.settings.language
            } onChange: {
                XCTAssertEqual(L10n.text("中文", "English"), text,
                               "The first render after a language change must use the new language")
                changed.fulfill()
            }
            store.update { $0.language = language }
            wait(for: [changed], timeout: 0.1)
            XCTAssertEqual(store.settings.language, language)
        }
    }
}
