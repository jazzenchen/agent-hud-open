import AppKit
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

final class AccountMenuTests: XCTestCase {
    @MainActor
    func testHostActionsUseMenuRowsWithoutReplacingCustomViewsOrTargets() throws {
        let suite = "AccountMenuTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [])
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        let target = NSResponder()
        let update = NSMenuItem(title: "New Version…", action: #selector(NSResponder.selectAll(_:)), keyEquivalent: "u")
        update.target = target
        update.isEnabled = false
        let custom = NSMenuItem(title: "Custom", action: nil, keyEquivalent: "")
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 250, height: 24))
        custom.view = view
        let separator = NSMenuItem.separator()
        let controller = StatusItemController(store: store, settings: settings, additionalMenuItems: { [update, separator, custom] })
        let menu = NSMenu()
        controller.menuNeedsUpdate(menu)
        XCTAssertNotNil(update.view)
        XCTAssertTrue(update.target === target)
        XCTAssertEqual(update.action, #selector(NSResponder.selectAll(_:)))
        XCTAssertEqual(update.keyEquivalent, "u")
        XCTAssertFalse(update.isEnabled)
        XCTAssertTrue(custom.view === view)
        XCTAssertNil(separator.view)
    }

    @MainActor
    func testHostMenuItemsAreRebuiltWithTheirEnabledStateBetweenSettingsAndQuit() throws {
        let suite = "AccountMenuTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite); L10n.setLanguage(.system) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [])
        settings.update { $0.language = .en }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        var available = false
        let controller = StatusItemController(store: store, settings: settings, additionalMenuItems: {
            let item = NSMenuItem(title: "Check for Updates…", action: nil, keyEquivalent: "")
            item.isEnabled = available
            return [item]
        })
        let menu = NSMenu()
        controller.menuNeedsUpdate(menu)
        XCTAssertEqual(menu.items.suffix(3).map(\.title), ["Settings…", "Check for Updates…", "Quit"])
        XCTAssertFalse(menu.items[menu.items.count - 2].isEnabled)
        available = true
        controller.menuNeedsUpdate(menu)
        XCTAssertTrue(menu.items[menu.items.count - 2].isEnabled)
        XCTAssertEqual(menu.items.filter { $0.title == "Check for Updates…" }.count, 1)
    }

    @MainActor
    func testPiCodexAccountAppearsOnceUnderItsSubscriptionProvider() throws {
        let suite = "AccountMenuTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite); L10n.setLanguage(.system) }
        let now = Date()
        let account = try XCTUnwrap(ProviderAccount.identified(provider: "Codex", user: "shared@example.com", workspace: "workspace"))
        let agents = ["5h", "Weekly"].map {
            AgentDescriptor(id: account.windowID($0), vendor: "Codex", model: $0, source: "", enabled: true, account: account)
        }
        let settings = SettingsStore(defaults: defaults, defaultAgents: agents)
        settings.update { $0.language = .en }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: UsageReport(generatedAt: now, snapshots: agents.map {
            .init(agentId: $0.id, remainingPct: 60, updatedAt: now)
        }, sessions: [], discoveredAgents: agents, accounts: ["Codex": [
            .init(account: account, home: "pi:alternate", client: "Pi", label: "shared@example.com", observedAt: now),
            .init(account: account, home: "", client: "Codex", label: "shared@example.com", observedAt: now, isCurrent: false)
        ]]))
        let controller = StatusItemController(store: store, settings: settings)
        let menu = NSMenu()
        controller.menuNeedsUpdate(menu)
        XCTAssertEqual(Array(menu.items.prefix(4)).map(\.title), ["Codex", "shared@example.com", "5h", "Weekly"])
        XCTAssertEqual(menu.items.filter { $0.title == "shared@example.com" }.count, 1)
        XCTAssertEqual(menu.items.filter { $0.title == "Codex" }.count, 1)
        XCTAssertFalse(menu.items.contains { $0.title == "Pi" })
    }

    @MainActor
    func testMenuGroupsAccountsAndDistinguishesPendingFromHistoricalReadings() throws {
        _ = NSApplication.shared
        let suite = "AccountMenuTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite); L10n.setLanguage(.system) }
        let now = Date()
        let current = ProviderAccount.identified(provider: "Codex", user: "current@example.com", workspace: "a")!
        let old = ProviderAccount.identified(provider: "Codex", user: "old@example.com", workspace: "b")!
        let agents = [current, old].map {
            AgentDescriptor(id: $0.windowID("codex"), vendor: "Codex", model: "Weekly", source: "", enabled: true, account: $0)
        }
        let settings = SettingsStore(defaults: defaults, defaultAgents: agents)
        settings.update { $0.language = .en }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: UsageReport(generatedAt: now, snapshots: agents.map {
            .init(agentId: $0.id, remainingPct: 0, resetAt: now.addingTimeInterval(-1), updatedAt: now)
        }, sessions: [], discoveredAgents: agents, accounts: ["Codex": [
            .init(account: current, label: "current@example.com", plan: "pro", observedAt: now),
            .init(account: old, label: "old@example.com", plan: "plus", observedAt: now.addingTimeInterval(-3600), isCurrent: false)
        ]]))
        let controller = StatusItemController(store: store, settings: settings)
        let menu = NSMenu()
        controller.menuNeedsUpdate(menu)
        XCTAssertEqual(Array(menu.items.prefix(5)).map(\.title), ["Codex", "current@example.com", "Weekly", "old@example.com", "Weekly"])
        XCTAssertTrue(menu.items[1].view?.accessibilityLabel()?.contains("Current account") == true)
        XCTAssertTrue(menu.items[2].view?.accessibilityLabel()?.contains("Pending update") == true)
        XCTAssertTrue(menu.items[3].view?.accessibilityLabel()?.contains("Last read 1h ago") == true)
        XCTAssertTrue(menu.items[4].view?.accessibilityLabel()?.hasSuffix("100% · —") == true)
        XCTAssertNil(store.rows[0].level, "a past deadline cannot show a confirmed exhaustion status")
        let future = UsageReport(generatedAt: now, snapshots: [.init(agentId: agents[0].id, remainingPct: 50,
            resetAt: now.addingTimeInterval(20), updatedAt: now)], sessions: [], discoveredAgents: [agents[0]])
        store.replace(report: future)
        XCTAssertEqual(store.rows[0].resetLabel(now: now), "<1m")
    }

    /// The menu bar figure counts only windows whose readings show a level, and only such a row has the hover hint as its
    /// tooltip. A header says its account is current only while its readings are sound, however old: the account's own
    /// failed read and its vendor's both make it say when it was last read. Its tooltip is what the island shows above the
    /// account: the account's reason, then its client's notices.
    @MainActor
    func testMenuFigureHeadersAndTooltipsFollowEachAccountsOwnReading() throws {
        let suite = "AccountMenuTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite); L10n.setLanguage(.system) }
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let failed = try XCTUnwrap(ProviderAccount.identified(provider: "Codex", user: "failed@example.com", workspace: "a"))
        let stale = try XCTUnwrap(ProviderAccount.identified(provider: "Codex", user: "stale@example.com", workspace: "b"))
        let old = try XCTUnwrap(ProviderAccount.identified(provider: "Codex", user: "old@example.com", workspace: "c"))
        // Each window: its account and name, what is left, when it resets and was read, and when it runs out.
        let windows: [(account: ProviderAccount, model: String, remaining: Double, resetIn: TimeInterval?, readAgo: TimeInterval,
                       exhaustsIn: TimeInterval?)] = [
            (failed, "Weekly", 10, 20, 300, 3600),
            (failed, "Credits", 70, nil, 300, nil),
            (stale, "Weekly", 40, -60, 7200, nil),
            (stale, "5h", 20, 7530, 7200, nil),
            (old, "Weekly", 2, 3 * 3600, 3 * 3600, 7200),
        ]
        let agents = windows.map {
            AgentDescriptor(id: $0.account.windowID($0.model), vendor: "Codex", model: $0.model, source: "", enabled: true, account: $0.account)
        }
        let report = UsageReport(
            generatedAt: now,
            snapshots: zip(agents, windows).map { agent, window in
                .init(agentId: agent.id, remainingPct: window.remaining, resetAt: window.resetIn.map(now.addingTimeInterval),
                      windowDuration: window.model == "5h" ? 5 * 3600 : 7 * 86400, updatedAt: now.addingTimeInterval(-window.readAgo))
            },
            sessions: [], discoveredAgents: agents,
            insightsByAgent: Dictionary(uniqueKeysWithValues: zip(agents, windows).compactMap { agent, window in
                window.exhaustsIn.map { (agent.id, UsageInsights(burnRatePctPerHour: 10, timeToExhaust: $0, weeklyCapHits: 0,
                                                                 weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil)) }
            }),
            sourceNotices: ["Codex": "Codex login failed"], quotaNotices: ["Codex": "Codex login failed"],
            accounts: ["Codex": [
                .init(account: failed, label: "failed@example.com", observedAt: now.addingTimeInterval(-300), quotaNotice: "Quota read failed"),
                .init(account: stale, label: "stale@example.com", observedAt: now.addingTimeInterval(-7200)),
                .init(account: old, label: "old@example.com", observedAt: now.addingTimeInterval(-3 * 3600), isCurrent: false),
            ]])
        let (menu, store) = buildMenu(showing: report, agents: agents, now: now, defaults: defaults)

        XCTAssertNil(store.maxUsedPct, "no window counts: the account's or the vendor's failed read, a passed reset or a signed-out account holds each back")
        XCTAssertEqual(menu.items.prefix(9).map { [$0.view?.accessibilityLabel() ?? "", $0.toolTip ?? "—"] }, [
            ["Codex", "—"],
            ["failed@example.com, Last read 5m ago", "Quota read failed · Codex login failed"],
            ["Weekly, 90% · <1m", "—"],
            ["Credits, 30% · —", "—"],
            ["stale@example.com, Last read 2h ago", "Codex login failed"],
            ["Weekly, 60% · Pending update", "—"],
            ["5h, 80% · 2h05m", "—"],
            ["old@example.com, Last read 3h ago", "Codex login failed"],
            ["Weekly, 98% · —", "—"],
        ])
        XCTAssertEqual(store.accountSections(store.rows).map { store.accountNotice(for: $0) },
                       ["Quota read failed · Codex login failed", "Codex login failed", "Codex login failed"],
                       "the island shows above each account what its menu header's tooltip says")
    }

    /// A balance's figure follows its level, as a quota row's does: red when the account is unavailable or its balance is
    /// at or below zero, and without a colour when its read failed, which its tooltip explains before the cost.
    @MainActor
    func testABalanceIsRedWhenItsLevelIsCritical() throws {
        let suite = "AccountMenuTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite); L10n.setLanguage(.system) }
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        // Each balance: its provider and amount, whether the service allows use, and a failed read's notice and age.
        let balances: [(provider: String, total: Decimal, isAvailable: Bool?, notice: String?, readAgo: TimeInterval)] = [
            ("DeepSeek", 50, false, nil, 0),
            ("Kimi", 0, true, nil, 0),
            ("GLM", -1, nil, nil, 0),
            ("OpenAI", 5, nil, "Balance could not be read", 7200),
        ]
        let pools = balances.map {
            BillingPool(provider: $0.provider, realm: "International", product: .api, scope: $0.provider, evidence: .credential, entitlement: "api")
        }
        let agents = pools.map {
            AgentDescriptor(id: $0.windowID("api"), vendor: $0.provider, model: "API", source: "", enabled: true, billingPool: $0)
        }
        let report = UsageReport(generatedAt: now, snapshots: [], sessions: [], discoveredAgents: agents,
                                 billing: zip(balances, pools).map { balance, pool in
            APIBilling(vendor: balance.provider, balances: [.init(currency: "USD", total: balance.total, granted: 0, toppedUp: 0)],
                       isAvailable: balance.isAvailable, updatedAt: now.addingTimeInterval(-balance.readAgo), notice: balance.notice,
                       billingPool: pool)
        })
        let (menu, store) = buildMenu(showing: report, agents: agents, now: now, defaults: defaults)

        let items = menu.items.filter { $0.title.hasSuffix(" · Balance") }
        XCTAssertEqual(items.map { $0.view?.accessibilityLabel() }, [
            "DeepSeek · API · Balance, $50.00", "Kimi · API · Balance, $0.00", "GLM · API · Balance, -$1.00", "OpenAI · API · Balance, $5.00",
        ])
        XCTAssertEqual(store.enabledBilling.map { store.view.level(of: $0) }, [.critical, .critical, .critical, nil])
        XCTAssertEqual(items.map { $0.toolTip?.hasPrefix("Balance could not be read\nEst. cost") }, [false, false, false, true])
        XCTAssertEqual(StatusItemController.valueColor(.critical), NSColor(StatusPalette.textColor(for: .critical, light: SystemAppearance.isLight)))
        XCTAssertEqual([StatusLevel.warning, .ok, nil].map { StatusItemController.valueColor($0) }, [.secondaryLabelColor, .secondaryLabelColor, .secondaryLabelColor])
    }

    /// The menu `StatusItemController` builds over `report` at `now`, in English, with the rows of `agents` shown.
    @MainActor
    private func buildMenu(showing report: UsageReport, agents: [AgentDescriptor], now: Date,
                           defaults: UserDefaults) -> (menu: NSMenu, store: UsageStore) {
        _ = NSApplication.shared
        let settings = SettingsStore(defaults: defaults, defaultAgents: agents)
        settings.update { $0.language = .en }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: report)
        store.now = now
        let menu = NSMenu()
        StatusItemController(store: store, settings: settings).menuNeedsUpdate(menu)
        return (menu, store)
    }

}
