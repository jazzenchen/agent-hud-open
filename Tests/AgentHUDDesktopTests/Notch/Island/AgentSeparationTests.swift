import AppKit
import XCTest
import AgentHUDCore
@testable import AgentHUDDesktop

final class AgentSeparationTests: XCTestCase {
    @MainActor
    func testMarksAreFoundAlongEveryEdgeAndOnlyOnTheStrip() {
        let centres: [(vendor: String, along: CGFloat)] = [("Claude", 10), ("Codex", 40), ("Grok", 70)]
        let top = CGRect(x: 500, y: 868, width: 80, height: 32)
        let marks = ScreenHUD.agentMarks(centres: centres, strip: top, edge: .top)
        XCTAssertEqual(marks.map(\.anchor), [510, 540, 570])
        XCTAssertEqual(ScreenHUD.mark(at: CGPoint(x: 548, y: 880), among: marks, strip: top, edge: .top)?.vendor, "Codex")
        XCTAssertEqual(ScreenHUD.mark(at: CGPoint(x: 579, y: 880), among: marks, strip: top, edge: .top)?.vendor, "Grok")
        XCTAssertNil(ScreenHUD.mark(at: CGPoint(x: 548, y: 800), among: marks, strip: top, edge: .top),
                     "A pointer in the open panel keeps the agent it came from")

        // A side strip is read from the top down, against the screen's upward coordinates.
        let right = CGRect(x: 1568, y: 400, width: 32, height: 80)
        let side = ScreenHUD.agentMarks(centres: centres, strip: right, edge: .right)
        XCTAssertEqual(side.map(\.anchor), [470, 440, 410])
        XCTAssertEqual(ScreenHUD.mark(at: CGPoint(x: 1580, y: 468), among: side, strip: right, edge: .right)?.vendor, "Claude")
    }

    @MainActor
    func testMarkCentresFollowTheLayoutThatDrawsThem() {
        let items = [SimpleMeterItem(vendor: "Claude", usedPct: 62, level: .ok),
                     SimpleMeterItem(vendor: "Codex", usedPct: 18, level: .ok)]
        for edge in HUDEdge.allCases {
            let meter = SimpleMeterConfig(items: items, placement: ScreenPlacement(mode: .simple, edge: edge))
            let centres = meter.markCentres
            let run = edge.isHorizontal ? meter.panelSize.width : meter.panelSize.height
            XCTAssertEqual(centres.map(\.vendor), ["Claude", "Codex"])
            XCTAssertGreaterThan(centres[0].along, meter.mark / 2 - 0.5)
            XCTAssertGreaterThanOrEqual(centres[1].along - centres[0].along, meter.mark + meter.itemGap - 0.5)
            XCTAssertLessThan(centres[1].along + meter.mark / 2, run, "\(edge): the last mark is inside the panel")
        }
        let queue = LogoQueueConfig(items: [LogoQueueItem(vendor: "Claude", isWorking: false),
                                            LogoQueueItem(vendor: "Codex", isWorking: false)],
                                    placement: ScreenPlacement(logoSize: 20, gapScale: 0.5), settings: Settings())
        XCTAssertEqual(queue.markCentres.map(\.along), [10, 40])
    }

    @MainActor
    func testADetachedCardFloatsBesideItsMarkOnTheInnerSideAndStaysOnScreen() {
        let screen = CGRect(x: 0, y: 0, width: 1600, height: 900)
        let size = CGSize(width: 200, height: 80)
        let top = CGRect(x: 600, y: 868, width: 400, height: 32)
        let below = ScreenHUD.detachedFrame(size: size, anchor: 650, strip: top, edge: .top, gap: 6, screen: screen)
        XCTAssertEqual(below.midX, 650)
        XCTAssertEqual(below.maxY, top.minY - 6, "Clear of the strip, not welded to the screen edge")
        XCTAssertEqual(ScreenHUD.detachedFrame(size: size, anchor: 40, strip: top, edge: .top, gap: 6, screen: screen).minX, 0)
        let above = ScreenHUD.detachedFrame(size: size, anchor: 650, strip: CGRect(x: 600, y: 0, width: 400, height: 32),
                                            edge: .bottom, gap: 6, screen: screen)
        XCTAssertEqual(above.minY, 38)
        let right = CGRect(x: 1568, y: 300, width: 32, height: 300)
        let beside = ScreenHUD.detachedFrame(size: size, anchor: 560, strip: right, edge: .right, gap: 6, screen: screen)
        XCTAssertEqual(beside.midY, 560)
        XCTAssertEqual(beside.maxX, right.minX - 6)
        // The Simple line starts behind its panel.
        let tucked = ScreenHUD.detachedFrame(size: size, anchor: 560, strip: right, edge: .right, gap: -16, screen: screen)
        XCTAssertEqual(tucked.maxX, right.minX + 16)
        let left = ScreenHUD.detachedFrame(size: size, anchor: 890, strip: CGRect(x: 0, y: 300, width: 32, height: 300),
                                           edge: .left, gap: 6, screen: screen)
        XCTAssertEqual(left.minX, 38)
        XCTAssertEqual(left.maxY, 900)
    }

    @MainActor
    func testThePeekIsSizedFromItsTextInBothStyles() {
        let lines = [AgentPeek.Line(label: "5h", value: "62%", level: .ok), AgentPeek.Line(label: "Weekly", value: "18%", level: .ok)]
        var peek = AgentPeek(vendor: "Claude", lines: lines, running: 2)
        XCTAssertEqual(peek.size, CGSize(width: AgentPeek.bubbleWidth, height: 20 + 18 * 4), "Name, two windows, running count")
        peek.style = .line
        let line = peek.size
        XCTAssertEqual(line.height, 26)
        peek.tuck = 16
        XCTAssertEqual(peek.size.width, line.width + 16, "The part behind the panel is extra, not taken from the text")
        XCTAssertGreaterThan(AgentPeek(vendor: "Claude", lines: [], running: 0, style: .line).size.width, 40)
    }

    @MainActor
    func testTheLineIsAsTallAsThePanelsRingAndNeverSmallerThanAtTheDefault() {
        let lines = [AgentPeek.Line(label: "5h", value: "62%", level: .ok), AgentPeek.Line(label: "Weekly", value: "18%", level: .ok)]
        func line(panel size: Double) -> AgentPeek {
            var peek = AgentPeek(vendor: "Claude", lines: lines, running: 0, style: .line)
            peek.scale = IslandRootView.dockScale(mode: .simple, placement: ScreenPlacement(mode: .simple, edge: .left, simpleSize: size))
            return peek
        }
        let usual = line(panel: 22), large = line(panel: 36), small = line(panel: 16)
        XCTAssertEqual(usual.lineFont, 9, "Every word a dock writes is 9 pt at the default")
        XCTAssertEqual(usual.size.height, 26, "The default panel's line is what it always was")
        XCTAssertEqual(line(panel: 26).size, usual.size, "A ring no taller than the line leaves it alone")
        XCTAssertEqual(line(panel: 30).size.height, 30, "Past that the line is as tall as the ring")
        XCTAssertEqual(line(panel: 30).lineFont, 10)
        XCTAssertEqual(large.size.height, 36)
        XCTAssertEqual(large.lineFont, 12, "Its text grows by as much as it does")
        XCTAssertGreaterThan(large.size.width, usual.size.width * 1.3)
        XCTAssertEqual(small.lineFont, 9, "Nothing is written smaller than at the default")
        XCTAssertEqual(small.size, usual.size)

        // The Dynamic dock's bubble keeps pace with its marks the same way.
        var bubble = AgentPeek(vendor: "Claude", lines: lines, running: 0)
        XCTAssertEqual(bubble.size, CGSize(width: 196, height: 20 + 18 * 3))
        bubble.scale = 1.6
        XCTAssertEqual(bubble.size, CGSize(width: 314, height: 2 * 16 + 29 * 3))
        bubble.scale = 0.6
        XCTAssertEqual(bubble.size, CGSize(width: 196, height: 20 + 18 * 3))
    }

    @MainActor
    func testTheHoveredMarkWakesFromThePeekOnAndIsLeftAloneAfterwards() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        for mode in [HUDMode.simple, .logos] {
            let domain = "app.agenthud.tests.attention.\(UUID().uuidString)"
            let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
            defer { defaults.removePersistentDomain(forName: domain) }
            let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
            settings.update { $0.screens[key] = ScreenPlacement(mode: mode, edge: .right, separatesAgents: true) }
            let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
            store.replace(report: DemoUsageProvider.report(agents: settings.agents,
                historyHours: UsageStore.historyHours, now: Date()))
            var pointer = CGPoint(x: -10_000, y: -10_000)
            let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings, mouseLocation: { pointer })
            defer { hud.close() }
            func attention() -> [MarkAttention] {
                let root = hud.island.rootView
                return mode == .simple ? (root.simpleMeter?.items.map(\.attention) ?? []) : (root.logoQueue?.items.map(\.attention) ?? [])
            }
            XCTAssertFalse(attention().isEmpty, "\(mode)")
            XCTAssertTrue(attention().allSatisfy { $0 == .none }, "\(mode): nothing is looked at while the dock is closed")

            let strip = hud.geometry.rect
            pointer = CGPoint(x: strip.midX, y: strip.maxY - 6)
            hud.samplePointer()
            hud.forceOpen()
            XCTAssertEqual(attention().first, .opened, "\(mode): the hovered mark is its bot from the peek on")
            XCTAssertTrue(attention().dropFirst().allSatisfy { $0 == .none }, "\(mode): the others follow their agents")

            hud.pinAgent()
            XCTAssertEqual(attention().first, .opened, "\(mode): and stays it through the click")

            hud.forceCollapse()
            XCTAssertTrue(attention().allSatisfy { $0 == .none }, "\(mode): closing gives every mark back to its agent")
        }
    }

    @MainActor
    func testHoverPeeksAClickOpensTheWholeCardAndEventsKeepTheMarks() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        for mode in [HUDMode.simple, .logos] {
            for edge in HUDEdge.allCases {
                let domain = "app.agenthud.tests.separate.\(UUID().uuidString)"
                let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
                defer { defaults.removePersistentDomain(forName: domain) }
                let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
                settings.update { $0.screens[key] = ScreenPlacement(mode: mode, edge: edge, separatesAgents: true) }
                let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
                store.replace(report: DemoUsageProvider.report(agents: settings.agents,
                    historyHours: UsageStore.historyHours, now: Date()))
                var pointer = CGPoint(x: -10_000, y: -10_000)
                let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings,
                                    mouseLocation: { pointer })
                defer { hud.close() }
                let strip = hud.geometry.rect
                let name = "\(mode) \(edge)"
                func card() throws -> CGRect {
                    let local = try XCTUnwrap(hud.island.rootView.presentationFrame)
                    let canvas = hud.island.panel.frame
                    return CGRect(x: canvas.minX + local.minX, y: canvas.maxY - local.maxY, width: local.width, height: local.height)
                }

                // The first mark in reading order: left on a horizontal edge, top on a side.
                pointer = edge.isHorizontal ? CGPoint(x: strip.minX + 6, y: strip.midY)
                                            : CGPoint(x: strip.midX, y: strip.maxY - 6)
                hud.samplePointer()
                hud.forceOpen()
                let first = try XCTUnwrap(hud.island.rootView.focusVendor, name)
                let peek = try XCTUnwrap(hud.island.rootView.peek, name)
                XCTAssertEqual(peek.style, mode == .simple ? .line : .bubble, name)
                XCTAssertFalse(hud.island.rootView.pinned)
                XCTAssertEqual(try card().size, peek.size, name)
                XCTAssertTrue(hud.island.panel.frame.insetBy(dx: -1, dy: -1).contains(strip), "\(name): the marks stay on the canvas")
                if mode == .logos {
                    XCTAssertFalse(try card().intersects(strip), "\(name): a bubble floats clear of the marks")
                }

                // Sliding to the far end of the strip changes agent without closing.
                pointer = edge.isHorizontal ? CGPoint(x: strip.maxX - 6, y: strip.midY)
                                            : CGPoint(x: strip.midX, y: strip.minY + 6)
                hud.samplePointer()
                XCTAssertNotEqual(hud.island.rootView.focusVendor, first, name)
                XCTAssertTrue(hud.island.rootView.isOpen)

                // A click asks for the whole panel, which is bigger than any peek and still clear of the marks.
                hud.pinAgent()
                XCTAssertTrue(hud.island.rootView.pinned, name)
                XCTAssertEqual(try card().width, IslandController.agentPanelWidth, name)
                XCTAssertFalse(try card().intersects(strip), name)
                XCTAssertTrue(screen.frame.insetBy(dx: -1, dy: -1).contains(try card()), name)
                hud.forceCollapse()
                XCTAssertFalse(hud.island.rootView.pinned, "\(name): closing forgets the click")

                // A reminder from the first agent opens beside its mark and replaces nothing.
                pointer = CGPoint(x: -10_000, y: -10_000)
                hud.samplePointer()
                hud.present(.completion(SessionCompletion(
                    sessionID: "separate", vendor: first, turnID: "reply", task: "New reply",
                    model: "test", startedAt: nil, completedAt: Date())))
                XCTAssertEqual(hud.island.rootView.focusVendor.map(AgentArtwork.markKey), AgentArtwork.markKey(first))
                XCTAssertNotNil(hud.island.rootView.alert)
                XCTAssertFalse(hud.island.rootView.isOpen, "\(name): a reply stays compact until it is clicked")
                XCTAssertFalse(try card().intersects(strip), name)
                XCTAssertTrue(hud.island.panel.frame.insetBy(dx: -1, dy: -1).contains(strip), "\(name): a reminder leaves the marks standing")
                XCTAssertTrue(screen.frame.insetBy(dx: -1, dy: -1).contains(hud.island.panel.frame))
            }
        }
    }

    /// Simple and Dynamic never hide their marks, separated or not: an event comes out of its sender's mark, or the
    /// middle of the dock for an agent with none, and only the open usage panel takes an event in as one of its rows.
    @MainActor
    func testEveryDockKeepsItsMarksAndAnEventComesOutOfItsSendersMark() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        for mode in [HUDMode.simple, .logos] {
            for edge in HUDEdge.allCases {
                let name = "\(mode) \(edge)"
                func dock() throws -> ScreenHUD {
                    let domain = "app.agenthud.tests.dock-events.\(UUID().uuidString)"
                    let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
                    addTeardownBlock { defaults.removePersistentDomain(forName: domain) }
                    let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
                    settings.update { $0.screens[key] = ScreenPlacement(mode: mode, edge: edge, offset: 0.4) }
                    let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
                    store.replace(report: DemoUsageProvider.report(agents: settings.agents,
                        historyHours: UsageStore.historyHours, now: Date()))
                    let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings,
                                        mouseLocation: { CGPoint(x: -10_000, y: -10_000) })
                    addTeardownBlock { hud.close() }
                    return hud
                }
                func card(_ hud: ScreenHUD) throws -> CGRect {
                    let local = try XCTUnwrap(hud.island.rootView.presentationFrame)
                    let canvas = hud.island.panel.frame
                    return CGRect(x: canvas.minX + local.minX, y: canvas.maxY - local.maxY, width: local.width, height: local.height)
                }
                /// The card stands clear of the strip, centred on `anchor` along the edge unless the screen's end holds
                /// it back.
                func besides(_ hud: ScreenHUD, _ anchor: CGFloat, _ what: String) throws {
                    let strip = hud.geometry.rect, frame = try card(hud)
                    XCTAssertFalse(frame.intersects(strip.insetBy(dx: 1, dy: 1)), "\(name) \(what): clear of the marks")
                    XCTAssertTrue(hud.island.panel.frame.insetBy(dx: -1, dy: -1).contains(strip), "\(name) \(what): the marks stay on the canvas")
                    let along = edge.isHorizontal ? frame.midX : frame.midY
                    let held = edge.isHorizontal ? frame.minX <= screen.frame.minX || frame.maxX >= screen.frame.maxX
                                                 : frame.minY <= screen.frame.minY || frame.maxY >= screen.frame.maxY
                    XCTAssertTrue(abs(along - anchor) <= 1 || held, "\(name) \(what): centred on \(anchor), at \(along)")
                }

                // A reply from the first agent: a pill beside its mark, then its card beside it.
                let replying = try dock()
                XCTAssertTrue(replying.agentMarks.isEmpty, "\(name): hovering opens the whole panel")
                let first = try XCTUnwrap(replying.dockMarks.first, name)
                replying.present(.completion(SessionCompletion(
                    sessionID: "dock", vendor: first.vendor, turnID: "reply", task: "New reply",
                    model: "test", startedAt: nil, completedAt: Date())))
                XCTAssertEqual(replying.island.rootView.focusVendor, first.vendor, name)
                XCTAssertNotNil(replying.island.rootView.alert)
                XCTAssertFalse(replying.island.rootView.isOpen, "\(name): a reply stays compact until it is hovered")
                try besides(replying, first.anchor, "pill")
                replying.forceOpen()
                XCTAssertTrue(replying.island.rootView.showsAlertDetails, name)
                XCTAssertEqual(replying.island.rootView.focusVendor, first.vendor, name)
                try besides(replying, first.anchor, "card")

                // A request from a client with no mark here comes out of the middle of the dock.
                let asking = try dock()
                let marked = Set(asking.dockMarks.map(\.key))
                if let source = PermissionHooks.Source.allCases.first(where: { !marked.contains(AgentArtwork.markKey($0.vendor)) }) {
                    let request = PermissionRequest(id: "dock-\(name)", source: source, sessionID: "dock", toolName: "Bash",
                                                    summary: "ls", detail: "ls", cwd: "/tmp", at: Date())
                    asking.present(.permission(request))
                    XCTAssertEqual(asking.island.rootView.focusVendor, source.vendor, name)
                    let strip = asking.geometry.rect
                    try besides(asking, edge.isHorizontal ? strip.midX : strip.midY, "markless request")
                    asking.withdraw(requestID: request.id)
                    XCTAssertNil(asking.island.rootView.focusVendor, name)
                }

                // News that arrives while the whole panel is open is one of its rows.
                let reading = try dock()
                reading.forceOpen()
                let agent = AgentDescriptor(id: "dock-quota", vendor: first.vendor, model: "test", source: "", enabled: true)
                reading.present(.quota(QuotaAlert.preview(.exhaustion, agent: agent, now: Date())))
                XCTAssertNotNil(reading.island.rootView.alert, name)
                XCTAssertNil(reading.island.rootView.focusVendor, "\(name): the open panel holds it")
                XCTAssertFalse(reading.island.rootView.showsAlertDetails, name)
            }
        }
    }

    @MainActor
    func testASettledCardAndItsWindowAreLeftAloneByLaterPasses() throws {
        _ = NSApplication.shared
        let screen = try XCTUnwrap(NSScreen.main)
        let key = ScreenIdentity.key(for: screen)
        for (mode, edge, size) in [(HUDMode.simple, HUDEdge.right, 23.0), (.logos, .bottom, 21.0)] {
            let domain = "app.agenthud.tests.settle.\(UUID().uuidString)"
            let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
            defer { defaults.removePersistentDomain(forName: domain) }
            let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.everyAgent)
            settings.update { $0.screens[key] = ScreenPlacement(mode: mode, edge: edge, offset: 0.37, logoSize: size, simpleSize: size, showsNumbers: false, separatesAgents: true) }
            let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
            store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
            var pointer = CGPoint(x: -10_000, y: -10_000)
            let hud = ScreenHUD(key: key, screen: screen, store: store, settings: settings, mouseLocation: { pointer })
            defer { hud.close() }
            let strip = hud.geometry.rect
            // Marks sit on half points at these sizes; a card placed on one would be nudged by every pass.
            for f in [0.13, 0.91] {
                pointer = edge.isHorizontal ? CGPoint(x: strip.minX + strip.width * f, y: strip.midY) : CGPoint(x: strip.midX, y: strip.maxY - strip.height * f)
                hud.samplePointer(); hud.forceOpen()
                RunLoop.main.run(until: Date().addingTimeInterval(0.6))
                let settled = hud.island.panel.frame
                let card = hud.island.rootView.presentationFrame
                var frames: Set<String> = []
                for _ in 0..<2 {
                    hud.apply(animated: true)
                    RunLoop.main.run(until: Date().addingTimeInterval(0.25))
                    frames.insert("\(hud.island.panel.frame)")
                    XCTAssertEqual(hud.island.rootView.presentationFrame, card, "\(mode) \(edge) \(f)")
                }
                XCTAssertEqual(frames, ["\(settled)"], "\(mode) \(edge) \(f): later passes must leave the window alone")
            }
        }
    }

    func testSeparationIsOffUnlessStoredAndSurvivesARoundTrip() throws {
        XCTAssertFalse(ScreenPlacement().separatesAgents)
        let old = try JSONDecoder().decode(ScreenPlacement.self, from: Data(#"{"mode":"simple","edge":"right"}"#.utf8))
        XCTAssertFalse(old.separatesAgents)
        let on = ScreenPlacement(mode: .simple, separatesAgents: true)
        XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: JSONEncoder().encode(on)), on)
    }
}
