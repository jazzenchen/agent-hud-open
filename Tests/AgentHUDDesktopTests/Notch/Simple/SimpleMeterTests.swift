import AppKit
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

final class SimpleMeterTests: XCTestCase {
    @MainActor
    func testWindowsThatShareAMarkReportTheMostUsedOne() {
        let now = Date()
        func agent(_ id: String, _ vendor: String) -> AgentDescriptor {
            AgentDescriptor(id: id, vendor: vendor, model: id, source: "test", enabled: true)
        }
        let agents = [agent("claude-5h", "Claude"), agent("codex-5h", "Codex"), agent("claude-week", "Claude"), agent("kimi", "Kimi")]
        let report = UsageReport(generatedAt: now, snapshots: [
            .init(agentId: "claude-5h", remainingPct: 69, resetAt: now.addingTimeInterval(3600), windowDuration: 5.0 * 3600, updatedAt: now),
            .init(agentId: "claude-week", remainingPct: 38, resetAt: now.addingTimeInterval(86400), windowDuration: 7.0 * 86400, updatedAt: now),
            .init(agentId: "codex-5h", remainingPct: 82, resetAt: now.addingTimeInterval(3600), windowDuration: 5.0 * 3600, updatedAt: now),
        ], sessions: [])
        let view = ReportView(report: report, agents: agents, settings: Settings(), now: now)

        let items = SimpleMeterItem.items(rows: view.rows)

        XCTAssertEqual(items.map(\.vendor), ["Claude", "Codex", "Kimi"], "One item per mark, in row order")
        XCTAssertEqual(items.map(\.usedPct), [62, 18, nil], "Claude reports its weekly window, the more used of its two")
        XCTAssertEqual(items.map(\.label), ["62%", "18%", "–"])
        XCTAssertEqual(items[0].level, view.rows.first { $0.id == "claude-week" }?.level)
        XCTAssertNil(items[2].level, "A window with no reading has no level to colour it")
        XCTAssertEqual(items.map(\.isWorking), [false, false, false])
        let busy = SimpleMeterItem.items(rows: view.rows, working: ["ChatGPT"])
        XCTAssertEqual(busy.map(\.isWorking), [false, true, false], "Work under a shared mark is that mark's work")
    }

    @MainActor
    func testAnAgentWithOnlyABalanceFollowsTheWindowsWithItsBalanceForAFigure() {
        let now = Date()
        let agents = [AgentDescriptor(id: "claude-5h", vendor: "Claude", model: "claude-5h", source: "test", enabled: true)]
        let report = UsageReport(generatedAt: now, snapshots: [
            .init(agentId: "claude-5h", remainingPct: 69, resetAt: now.addingTimeInterval(3600), windowDuration: 5.0 * 3600, updatedAt: now),
        ], sessions: [])
        let view = ReportView(report: report, agents: agents, settings: Settings(), now: now)
        let balances = [SimpleMeterItem.Balance(vendor: "DeepSeek", text: "¥8.85", level: .ok),
                        // Claude has a window, so a balance of its own does not give it a second place.
                        SimpleMeterItem.Balance(vendor: "Claude", text: "$3.00", level: .ok),
                        SimpleMeterItem.Balance(vendor: "Kimi", text: nil, level: nil)]

        let items = SimpleMeterItem.items(rows: view.rows, balances: balances, working: ["DeepSeek"])

        XCTAssertEqual(items.map(\.vendor), ["Claude", "DeepSeek", "Kimi"])
        XCTAssertEqual(items.map(\.label), ["31%", "¥8.85", "–"], "A balance stands where the percentage would; none read yet is a dash")
        XCTAssertNil(items[1].usedPct, "A balance is a share of nothing, so its ring stays empty")
        XCTAssertEqual(items[1].level, .ok)
        XCTAssertEqual(items.map(\.isWorking), [false, true, false])
    }

    @MainActor
    func testTheDemosDeepSeekBalanceIsOnThePanelAndInItsPeek() throws {
        L10n.setLanguage(.en)
        defer { L10n.setLanguage(.system) }
        let domain = "app.agenthud.tests.simple-balance.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, accessAllowed: { false })
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))

        let items = SimpleMeterItem.items(rows: store.rows, balances: SimpleMeterItem.balances(store))
        let deepSeek = try XCTUnwrap(items.last)
        XCTAssertEqual(deepSeek.vendor, "DeepSeek")
        XCTAssertEqual(deepSeek.label, "¥8.85")

        let peek = AgentPeek.make(vendor: "DeepSeek", store: store)
        XCTAssertEqual(peek.lines.map(\.label), ["Balance"])
        XCTAssertEqual(peek.lines.map(\.value), ["¥8.85"])
        XCTAssertEqual(AgentPeek.make(vendor: "Claude", store: store).lines.map(\.label).contains("Balance"), false)
    }

    func testPanelIsSizedFromItsOneLengthAndDropsWhatIsSwitchedOff() {
        let items = [SimpleMeterItem(vendor: "Claude", usedPct: 62, level: .ok),
                     SimpleMeterItem(vendor: "Codex", usedPct: 100, level: .critical)]
        func config(_ edge: HUDEdge, size: Double = 22, rings: Bool = true, numbers: Bool = true) -> SimpleMeterConfig {
            SimpleMeterConfig(items: items, placement: ScreenPlacement(mode: .simple, edge: edge, simpleSize: size,
                                                                      showsRings: rings, showsNumbers: numbers))
        }
        let both = config(.top), ringsOnly = config(.top, numbers: false), numbersOnly = config(.top, rings: false)
        XCTAssertEqual(both.panelSize.height, 22 + both.insets.top + both.insets.bottom, "A ring sets the panel's thickness")
        XCTAssertEqual(ringsOnly.panelSize.width,
                       ringsOnly.insets.left + 2 * 22 + ringsOnly.itemGap + ringsOnly.insets.right,
                       "Without numbers the panel is its rings and the gaps between them")
        XCTAssertLessThan(ringsOnly.panelSize.width, both.panelSize.width)
        XCTAssertLessThan(numbersOnly.panelSize.height, both.panelSize.height, "A mark alone is shorter than its ring")
        XCTAssertEqual(both.labelWidth(items[1]), both.labelColumn, "The widest number sets a side panel's column")

        let side = config(.right)
        XCTAssertEqual(side.panelSize.width, side.insets.left + 22 + side.labelSpacing + side.labelColumn + side.insets.right)
        XCTAssertEqual(side.panelSize.height, side.insets.top + 2 * 22 + side.itemGap + side.insets.bottom)
        XCTAssertEqual(side.stripSize, CGSize(width: side.panelSize.width + SimpleMeterConfig.edgeMargin, height: side.panelSize.height),
                       "The margin is on the axis that meets the screen edge")
        XCTAssertEqual(both.stripSize, CGSize(width: both.panelSize.width, height: both.panelSize.height + SimpleMeterConfig.edgeMargin))

        let large = config(.top, size: 40)
        XCTAssertEqual(large.panelSize.height, 40 + large.insets.top + large.insets.bottom)
        XCTAssertGreaterThan(large.fontSize, both.fontSize, "The numbers scale with the rings")

        let empty = SimpleMeterConfig(items: [], placement: ScreenPlacement(mode: .simple, edge: .left))
        XCTAssertEqual(empty.panelSize.height, SimpleMeterConfig.emptyRun, "A panel with no quota row keeps something to grab")
    }

    func testTheGapBetweenAgentsIsItsOwnShareOfARing() {
        let items = [SimpleMeterItem(vendor: "Claude", usedPct: 62, level: .ok),
                     SimpleMeterItem(vendor: "Codex", usedPct: 100, level: .critical)]
        func config(_ edge: HUDEdge, size: Double = 40, gap: Double? = nil, numbers: Bool = true) -> SimpleMeterConfig {
            var placement = ScreenPlacement(mode: .simple, edge: edge, simpleSize: size, showsNumbers: numbers)
            if let gap { placement.simpleGapScale = gap }
            return SimpleMeterConfig(items: items, placement: placement)
        }
        XCTAssertEqual(config(.top).itemGap, 18, "0.45 of a ring at first, the gap the panel always had")
        XCTAssertEqual(config(.right).itemGap, 18)
        XCTAssertEqual(config(.top, numbers: false).itemGap, 13, "Marks alone in a row sit at 70 % of it")
        XCTAssertEqual(config(.top, gap: 0.1).itemGap, 4)
        XCTAssertEqual(config(.right, gap: 0.6).itemGap, 24)
        XCTAssertEqual(config(.top, gap: 0.6, numbers: false).itemGap, 17)
        let tight = config(.right, gap: 0.1), loose = config(.right, gap: 0.6)
        XCTAssertEqual(loose.panelSize.height - tight.panelSize.height, 24 - 4, "One gap between two agents")
        XCTAssertEqual(loose.panelSize.width, tight.panelSize.width)
        XCTAssertEqual(config(.right, size: 60).panelSize.height, config(.right, size: 60).insets.top + 2 * 60 + 27
                       + config(.right, size: 60).insets.bottom, "A ring reaches 60 pt")
    }

    func testDraggingASimplePanelKeepsItSimpleAndOnlyTheNotchBecomesADock() {
        let screen = CGRect(x: 0, y: 0, width: 1440, height: 900)
        let point = CGPoint(x: screen.maxX - 4, y: 420)
        let simple = NotchGeometry.dragPlacement(at: point, screenFrame: screen, queue: CGSize(width: 120, height: 33),
                                                 placement: ScreenPlacement(mode: .simple, edge: .top, ringWidth: 2.5, showsNumbers: false))
        XCTAssertEqual(simple.mode, .simple)
        XCTAssertEqual(simple.edge, .right)
        XCTAssertEqual(simple.ringWidth, 2.5)
        XCTAssertFalse(simple.showsNumbers)
        let notch = NotchGeometry.dragPlacement(at: point, screenFrame: screen, queue: CGSize(width: 120, height: 33),
                                                placement: ScreenPlacement(mode: .notch, edge: .top))
        XCTAssertEqual(notch.mode, .logos)
    }

    @MainActor
    func testSimpleScreenParksItsPanelOnTheEdgeAndDrawsNoGlow() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        let domain = "app.agenthud.tests.simple.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        let placement = ScreenPlacement(mode: .simple, edge: .right, offset: 0.3, simpleSize: 26)
        settings.update { $0.screens[key] = placement }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, accessAllowed: { false })
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
        defer { hud.close() }

        let working = store.queueVendors.filter(\.isWorking).map(\.vendor)
        let config = SimpleMeterConfig(items: SimpleMeterItem.items(rows: store.rows, balances: SimpleMeterItem.balances(store), working: working),
                                       placement: placement, bots: settings.settings.botLogos)
        XCTAssertFalse(config.items.isEmpty)
        XCTAssertTrue(config.items.contains(where: \.isWorking), "The demo has work in flight for the panel's bots to show")
        XCTAssertEqual(hud.geometry.mode, .simple)
        XCTAssertEqual(hud.geometry.edge, .right)
        XCTAssertEqual(hud.geometry.rect.maxX, screen.frame.maxX, accuracy: 0.001)
        XCTAssertEqual(hud.geometry.rect.size.width, config.stripSize.width, accuracy: 0.001, "The strip is the panel and its margin")
        XCTAssertEqual(hud.geometry.rect.size.height, config.stripSize.height.rounded(), accuracy: 0.001)
        XCTAssertEqual(hud.island.rootView.simpleMeter, config)
        XCTAssertNil(hud.island.rootView.logoQueue, "The panel takes the logo queue's place")
        XCTAssertTrue(hud.island.rootView.hidesSilhouette)
        XCTAssertNil(hud.glow.glowLayer.contents, "Simple mode has no glow behind it")

        settings.update { $0.screens[key]?.showsNumbers = false }
        hud.apply(animated: false)
        XCTAssertLessThan(hud.geometry.rect.width, config.stripSize.width, "Switching the numbers off narrows the strip at once")
    }

    @MainActor
    func testEveryEdgeKeepsItsFiguresInPlaceWhileThePanelOpensInward() async throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        for edge in HUDEdge.allCases {
            let domain = "app.agenthud.tests.simple-open.\(UUID().uuidString)"
            let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
            defer { defaults.removePersistentDomain(forName: domain) }
            let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
            settings.update {
                $0.screens[key] = ScreenPlacement(mode: .simple, edge: edge, offset: 0.4)
                $0.collapseDelayMs = 5000
            }
            let store = UsageStore(provider: DemoUsageProvider(), settings: settings, accessAllowed: { false })
            store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
            let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
            defer { hud.close() }
            let strip = hud.geometry.rect

            hud.forceOpen()
            let root = hud.island.rootView, canvas = hud.island.panel.frame
            func global(_ local: CGRect) -> CGRect {
                CGRect(x: canvas.minX + local.minX, y: canvas.maxY - local.maxY, width: local.width, height: local.height)
            }
            XCTAssertTrue(root.isOpen, "\(edge)")
            XCTAssertNotNil(root.simpleMeter, "\(edge): the figures stay on the open panel")
            let figures = global(try XCTUnwrap(root.logoQueueFrame))
            XCTAssertEqual(figures.minX, strip.minX, accuracy: 0.5, "\(edge): the figures keep their place on screen")
            XCTAssertEqual(figures.minY, strip.minY, accuracy: 0.5, "\(edge)")
            XCTAssertEqual(figures.size.width, strip.width, accuracy: 0.5, "\(edge)")
            let card = global(try XCTUnwrap(root.presentationFrame))
            XCTAssertGreaterThan(card.width * card.height, strip.width * strip.height, "\(edge): the panel has opened")
            switch edge {
            case .top: XCTAssertEqual(card.maxY, screen.frame.maxY, accuracy: 1)
            case .bottom: XCTAssertEqual(card.minY, screen.frame.minY, accuracy: 1)
            case .left: XCTAssertEqual(card.minX, screen.frame.minX, accuracy: 1)
            case .right: XCTAssertEqual(card.maxX, screen.frame.maxX, accuracy: 1)
            }
            XCTAssertNil(hud.glow.glowLayer.contents, "\(edge): an open Simple HUD draws no glow either")
        }
    }

    @MainActor
    func testFloatingSilhouetteIsThePanelCollapsedAndTheCardsCoreOpenOnEveryEdge() async {
        let margin = SimpleMeterConfig.edgeMargin, flare = IslandRootView.expandedTopRadius
        for edge in HUDEdge.allCases {
            // SwiftUI coordinates, y down: what a floating silhouette leaves clear on its edge side and at its ends.
            func inset(_ rect: CGRect, ends: CGFloat) -> CGRect {
                switch edge {
                case .top: return CGRect(x: rect.minX + ends, y: rect.minY + margin, width: rect.width - 2 * ends, height: rect.height - margin)
                case .bottom: return CGRect(x: rect.minX + ends, y: rect.minY, width: rect.width - 2 * ends, height: rect.height - margin)
                case .left: return CGRect(x: rect.minX + margin, y: rect.minY + ends, width: rect.width - margin, height: rect.height - 2 * ends)
                case .right: return CGRect(x: rect.minX, y: rect.minY + ends, width: rect.width - margin, height: rect.height - 2 * ends)
                }
            }
            func assertBounds(_ shape: IslandShape, in rect: CGRect, equal expected: CGRect, _ message: String) {
                let bounds = shape.path(in: rect).boundingRect
                for (got, want) in [(bounds.minX, expected.minX), (bounds.minY, expected.minY), (bounds.maxX, expected.maxX), (bounds.maxY, expected.maxY)] {
                    XCTAssertEqual(got, want, accuracy: 0.01, "\(edge): \(message)")
                }
            }
            let config = SimpleMeterConfig(items: SimpleMeterItem.samples, placement: ScreenPlacement(mode: .simple, edge: edge))
            let strip = CGRect(origin: .zero, size: config.stripSize)
            assertBounds(IslandShape(topRadius: 0, bottomRadius: config.cornerRadius, edge: edge, lift: margin), in: strip,
                         equal: inset(strip, ends: 0), "collapsed, the silhouette is the panel inside its strip")
            XCTAssertEqual(inset(strip, ends: 0).size, config.panelSize, "\(edge)")

            let card = edge.isHorizontal ? CGRect(x: 0, y: 0, width: 572, height: 400) : CGRect(x: 0, y: 0, width: 540, height: 432)
            assertBounds(IslandShape(topRadius: flare, bottomRadius: IslandRootView.expandedBottomRadius, edge: edge, lift: margin), in: card,
                         equal: inset(card, ends: flare), "open, it fills the card's core and still floats")
            assertBounds(IslandShape(topRadius: flare, bottomRadius: IslandRootView.expandedBottomRadius, edge: edge), in: card,
                         equal: card, "a flared silhouette reaches its edge and both ends of the card")
        }
    }

    @MainActor
    func testAMarkWithNothingUnderItDrawsItsOwnLogoWhileItRests() throws {
        let form = try XCTUnwrap(BotForm.form(for: "Claude", ground: .dark))
        let queued = BotActor(form: form, staysBot: false, period: 3)
        let alone = BotActor(form: form, staysBot: false, period: 3)
        alone.restsHidden = false
        for actor in [queued, alone] {
            actor.resize(side: 12, scale: 2, outline: 0)
            actor.set(.idle, at: 10)
            _ = actor.step(to: 10, yRunsDown: true) { .zero }
        }
        XCTAssertTrue(queued.layer.isHidden, "a queue draws the resting logo itself")
        XCTAssertFalse(alone.layer.isHidden)
        XCTAssertNotNil(alone.layer.contents, "the Simple panel's resting mark is its logo, drawn by the actor")
        let wait = try XCTUnwrap(alone.step(to: 10.05, yRunsDown: true) { .zero })
        XCTAssertGreaterThan(wait, 15, "a visible resting mark still asks for no frame before its visit")
    }

    @MainActor
    func testThePanelsBotsFollowBotLogos() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        for on in [true, false] {
            let domain = "app.agenthud.tests.simple-bots.\(UUID().uuidString)"
            let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
            defer { defaults.removePersistentDomain(forName: domain) }
            let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
            settings.update {
                $0.screens[key] = ScreenPlacement(mode: .simple)
                $0.botLogos = on
            }
            let store = UsageStore(provider: DemoUsageProvider(), settings: settings, accessAllowed: { false })
            let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings)
            defer { hud.close() }
            XCTAssertEqual(hud.island.rootView.simpleMeter?.bots, on, "the panel follows the one switch for every mark")
        }
        XCTAssertFalse(SimpleMeterConfig(items: [], placement: ScreenPlacement(mode: .simple)).bots,
                       "a panel only measured or drawn in a snapshot keeps its logos")
    }
}
