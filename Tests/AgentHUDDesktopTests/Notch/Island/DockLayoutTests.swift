import AppKit
import SwiftUI
import XCTest
import AgentHUDCore
@testable import AgentHUDDesktop

final class DockLayoutTests: XCTestCase {
    @MainActor
    func testADockReminderIsAsLongAsWhatItSays() {
        L10n.setLanguage(.en)
        defer { L10n.setLanguage(.system) }
        func reply(_ vendor: String) -> IslandAlert {
            .completion(SessionCompletion(sessionID: "s", vendor: vendor, turnID: "t", task: "Task", model: "m",
                                          startedAt: nil, completedAt: Date()))
        }
        let short = CompactWings.dock(for: reply("Pi"), scale: 1), claude = CompactWings.dock(for: reply("Claude"), scale: 1)
        XCTAssertLessThan(short.leading, claude.leading, "Each half is as wide as its words")
        XCTAssertEqual(short.trailing, claude.trailing)
        XCTAssertLessThan(claude.width, 215, "Where two fixed wings made it 256")
        XCTAssertEqual(CompactWings.dock(for: reply("An agent with a very long name indeed"), scale: 1).leading, 112,
                       "No wider than one of the notch's wings; the name is cut short there as it is beside the notch")
        XCTAssertGreaterThan(CompactWings.dock(for: reply("Claude"), scale: 1.6).width, claude.width * 1.5, "And to the dock's scale")
        XCTAssertEqual(CompactWings.dock(for: reply("Claude"), scale: 0.5), claude, "Never smaller than at the default")
        let request = PermissionRequest(id: "r", source: .claude, sessionID: "s", toolName: "Bash", summary: "Run the tests",
                                        detail: "swift test", cwd: "~/work/agent-hud", at: Date())
        XCTAssertGreaterThan(CompactWings.dock(for: .permission(request), waiting: 3, scale: 1).trailing,
                             CompactWings.dock(for: .permission(request), waiting: 1, scale: 1).trailing,
                             "A count of the requests waiting has its own room")
    }

    @MainActor
    func testADockReminderIsAsTallAsOneOfTheDocksMarksAndTheNotchHasNoScale() {
        func pill(_ placement: ScreenPlacement) -> CGFloat {
            IslandRootView.dockCompactSize(edge: placement.edge, collapsedSize: CGSize(width: 216, height: 32),
                                           scale: IslandRootView.dockScale(mode: placement.mode, placement: placement)).height
        }
        // A Simple panel's mark is its ring, drawn or not. A ring shorter than the default pill leaves the pill alone.
        for (ring, height): (Double, CGFloat) in [(20, 26), (22, 26), (26, 26), (27, 27), (30, 30), (33, 33), (40, 40), (50, 50)] {
            XCTAssertEqual(pill(ScreenPlacement(mode: .simple, simpleSize: ring)), height, "A \(ring) pt ring")
            XCTAssertEqual(pill(ScreenPlacement(mode: .simple, simpleSize: ring, showsRings: false)), height)
        }
        // A Dynamic dock's is its logo, whatever the Simple panel on the same screen was set to.
        for (logo, height): (Double, CGFloat) in [(12, 26), (20, 26), (27, 27), (36, 36)] {
            XCTAssertEqual(pill(ScreenPlacement(mode: .logos, logoSize: logo, simpleSize: 40)), height, "A \(logo) pt logo")
        }
        // The notch has no size of its own to follow, so neither dock's setting reaches it.
        XCTAssertEqual(IslandRootView.dockScale(mode: .notch, placement: ScreenPlacement(mode: .notch, logoSize: 36, simpleSize: 40)), 1)
        // The row and its two margins are the pill, to the point, at every size.
        let sizes = ScreenPlacement.simpleSizeRange
        for size in stride(from: sizes.lowerBound, through: sizes.upperBound, by: 1) {
            let row = IslandRootView.dockCompactRow(scale: IslandRootView.dockScale(mode: .simple, placement: ScreenPlacement(mode: .simple, simpleSize: size)))
            XCTAssertEqual(row.height + 2 * row.inset, max(26, size), "A \(size) pt ring")
        }
    }

    @MainActor
    func testADockWritesAReminderTheNumbersAndTheHoverLineAtOneSize() {
        let sizes = ScreenPlacement.simpleSizeRange
        for ring in stride(from: sizes.lowerBound, through: sizes.upperBound, by: 1) {
            let placement = ScreenPlacement(mode: .simple, edge: .right, simpleSize: ring)
            let scale = IslandRootView.dockScale(mode: .simple, placement: placement)
            let text = IslandRootView.dockText(scale: scale)
            XCTAssertEqual(SimpleMeterConfig(items: [], placement: placement).fontSize, text, "A \(ring) pt ring's numbers")
            var line = AgentPeek(vendor: "Claude", lines: [], running: 0, style: .line)
            line.scale = scale
            XCTAssertEqual(line.lineFont, text, "A \(ring) pt ring's hover line")
            // A reminder's views write its name and status at the dock's scale, as `s(_:)` rounds it.
            for beside: CGFloat in [13, 12] {
                XCTAssertEqual((CompactWings.words(beside, onDock: true) * scale).rounded(), text, "A \(ring) pt ring's reminder")
            }
        }
        XCTAssertEqual(IslandRootView.dockText(scale: 1), 9)
        XCTAssertEqual(IslandRootView.dockText(scale: IslandRootView.dockScale(mode: .simple, placement: ScreenPlacement(mode: .simple, simpleSize: 50))), 17)
        // Beside the notch a reminder keeps the cards' sizes.
        XCTAssertEqual(CompactWings.words(13, onDock: false), 13)
        XCTAssertEqual(CompactWings.words(12, onDock: false), 12)
    }

    @MainActor
    func testCompactDockEventsUseOneThinRowInsideTheirContactShoulders() async throws {
        _ = NSApplication.shared
        let alert = IslandAlert.completion(SessionCompletion(
            sessionID: "compact-dock", vendor: "Codex", turnID: "reply", task: "New reply",
            model: "test", startedAt: nil, completedAt: Date()))
        for edge in HUDEdge.allCases {
            let strip = edge.isHorizontal ? CGSize(width: 216, height: 32) : CGSize(width: 32, height: 400)
            let wings = CompactWings.dock(for: alert, scale: 1)
            let core = IslandRootView.dockCompactSize(edge: edge, collapsedSize: strip, wings: wings)
            XCTAssertEqual(core, CGSize(width: wings.width, height: 26), "\(edge): events do not reserve the hidden logo strip")
            XCTAssertLessThan(core.width, 256, "As long as what it says, not two full wings")
            // Without its own halves a reminder is two full wings long, the most it can be. A dock set larger than its
            // default grows its reminder with it, and one set smaller does not shrink it.
            let widest = IslandRootView.dockCompactSize(edge: edge, collapsedSize: strip)
            XCTAssertEqual(widest, CGSize(width: 256, height: 26))
            XCTAssertEqual(IslandRootView.dockCompactSize(edge: edge, collapsedSize: strip, scale: 1.5), CGSize(width: 384, height: 39))
            XCTAssertEqual(IslandRootView.dockCompactSize(edge: edge, collapsedSize: strip, scale: 0.6), widest)
            XCTAssertEqual(IslandRootView.dockCompactSize(edge: edge, collapsedSize: CGSize(width: 80, height: 500), wings: wings), core,
                           "Making the logo queue thicker or longer must not add empty space to a compact event")
            let size = CGSize(width: core.width + (edge.isHorizontal ? 32 : 0),
                              height: core.height + (edge.isHorizontal ? 0 : 32))
            var root = IslandRootView(store: nil, isOpen: false, collapsedSize: strip,
                                      collapsedTopRadius: IslandRootView.expandedTopRadius,
                                      collapsedBottomRadius: 10, look: .standard, lightBorder: false, onOpenStats: {},
                                      alert: alert)
            root.hidesSilhouette = true
            root.edge = edge
            root.presentationFrame = CGRect(origin: .zero, size: size)
            root.animatesGeometry = false
            let controller = IslandWindowController(
                frame: CGRect(x: -20000, y: -20000, width: size.width, height: size.height), rootView: root)
            XCTAssertEqual(controller.contentHeight(for: root), core.height, accuracy: 1,
                           "The actual compact view must fit a 20 pt row and two 3 pt margins")
            controller.show()
            defer { controller.panel.orderOut(nil) }
            try await Task.sleep(for: .milliseconds(80))
            let hosting = try XCTUnwrap(controller.panel.contentView)
            hosting.layoutSubtreeIfNeeded()
            let bitmap = try XCTUnwrap(hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds))
            hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
            let scale = size.height / CGFloat(bitmap.pixelsHigh)
            let painted = (0..<bitmap.pixelsHigh).filter {
                (bitmap.colorAt(x: bitmap.pixelsWide / 2, y: $0)?.alphaComponent ?? 0) > 0.9
            }
            let shoulder = edge.isHorizontal ? CGFloat.zero : IslandRootView.expandedTopRadius
            XCTAssertEqual((CGFloat(try XCTUnwrap(painted.first)) + 0.5) * scale, shoulder, accuracy: scale,
                           "\(edge): the event's black core starts immediately after its shoulder")
            XCTAssertEqual((CGFloat(try XCTUnwrap(painted.last)) + 0.5) * scale,
                           size.height - shoulder, accuracy: scale)
            let readableRows = (0..<bitmap.pixelsHigh).filter { row in
                (0..<bitmap.pixelsWide).contains { column in
                    guard let color = bitmap.colorAt(x: column, y: row)?.usingColorSpace(.deviceRGB) else { return false }
                    return color.alphaComponent > 0.8 && max(color.redComponent, color.greenComponent, color.blueComponent) > 0.3
                }
            }
            let firstText = try XCTUnwrap(readableRows.first)
            let lastText = try XCTUnwrap(readableRows.last)
            // A 17 pt mark in a 26 pt pill: the row's 3 pt margin and a point and a half of the row itself.
            XCTAssertGreaterThanOrEqual(CGFloat(firstText) * scale, shoulder + 4,
                                        "\(edge): the logo and text must fit within the compact row")
            XCTAssertLessThanOrEqual(CGFloat(lastText + 1) * scale, size.height - shoulder - 4)
        }
    }

    @MainActor
    func testEveryDockEdgeKeepsTheContactFlaresAndInsetInwardSides() {
        for edge in HUDEdge.allCases {
            let rect = CGRect(x: 37, y: 53, width: edge.isHorizontal ? 240 : 120,
                              height: edge.isHorizontal ? 120 : 240)
            let path = IslandShape(topRadius: 16, bottomRadius: 26, edge: edge).path(in: rect)
            XCTAssertEqual(path.boundingRect, rect, "\(edge) must touch the full contact edge and reach into the screen")

            func point(along: CGFloat, inward: CGFloat) -> CGPoint {
                switch edge {
                case .top: return CGPoint(x: rect.minX + along, y: rect.minY + inward)
                case .bottom: return CGPoint(x: rect.minX + along, y: rect.maxY - inward)
                case .left: return CGPoint(x: rect.minX + inward, y: rect.minY + along)
                case .right: return CGPoint(x: rect.maxX - inward, y: rect.minY + along)
                }
            }
            for along in [CGFloat(4), 236] {
                XCTAssertTrue(path.contains(point(along: along, inward: 0.1)),
                              "\(edge) must flare outward near both contact corners")
                XCTAssertFalse(path.contains(point(along: along, inward: 4)),
                               "\(edge) contact corners must curve inward from the edge")
            }
            XCTAssertFalse(path.contains(point(along: 12, inward: 30)),
                           "The side beyond a contact flare must sit 16pt inside the window")
            XCTAssertTrue(path.contains(point(along: 17, inward: 30)))
            XCTAssertFalse(path.contains(point(along: 17, inward: 119)), "The inward corner must remain convex")
            XCTAssertTrue(path.contains(point(along: 43, inward: 119)))
        }
    }

    @MainActor
    func testVerticalQueueLengthDoesNotBecomePanelTopClearance() throws {
        _ = NSApplication.shared
        let domain = "app.agenthud.tests.dock-layout.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        settings.panelLayout.setSwitches { $0.quotaDetails = false; $0.tokenUsage = false; $0.sessionList = false }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        for edge in [HUDEdge.left, .right] {
            var root = IslandRootView(store: store, isOpen: true, collapsedSize: CGSize(width: 32, height: 80),
                                      collapsedTopRadius: 8, collapsedBottomRadius: 10,
                                      look: .standard, lightBorder: false, onOpenStats: {})
            root.hidesSilhouette = true
            root.edge = edge
            let controller = IslandWindowController(
                frame: CGRect(x: -20000, y: -20000, width: 540, height: 400), rootView: root)
            let shortHeight = controller.contentHeight(for: root)
            root = IslandRootView(store: store, isOpen: true, collapsedSize: CGSize(width: 32, height: 400),
                                  collapsedTopRadius: 8, collapsedBottomRadius: 10,
                                  look: .standard, lightBorder: false, onOpenStats: {})
            root.hidesSilhouette = true
            root.edge = edge
            XCTAssertEqual(controller.contentHeight(for: root), shortHeight, accuracy: 1,
                           "Adding logos down a side must not push the content down or make the card taller")
            XCTAssertLessThan(shortHeight, 150, "A dock with no content should remain a compact footer")
        }
    }

    @MainActor
    func testOpenDockKeepsLogoQueueAtItsExplicitWindowRectOnEveryEdge() async throws {
        _ = NSApplication.shared
        let size = CGSize(width: 540, height: 400)
        let cases: [(HUDEdge, CGRect)] = [
            (.top, CGRect(x: 58, y: 0, width: 200, height: 32)),
            (.bottom, CGRect(x: 58, y: 368, width: 200, height: 32)),
            (.left, CGRect(x: 0, y: 58, width: 32, height: 200)),
            (.right, CGRect(x: 508, y: 58, width: 32, height: 200))
        ]
        for (edge, rect) in cases {
            let placement = ScreenPlacement(mode: .logos, edge: edge)
            var root = IslandRootView(store: nil, isOpen: true, collapsedSize: rect.size,
                                      collapsedTopRadius: 8, collapsedBottomRadius: 10,
                                      look: .standard, lightBorder: false, onOpenStats: {})
            root.hidesSilhouette = true
            root.edge = edge
            root.logoQueue = LogoQueueConfig(items: [LogoQueueItem(vendor: "Claude", isWorking: false)],
                                             placement: placement, settings: AgentHUDCore.Settings())
            root.logoQueueFrame = rect
            root.presentationSize = size
            root.animatesGeometry = false
            let controller = IslandWindowController(
                frame: CGRect(x: -20000, y: -20000, width: size.width, height: size.height), rootView: root)
            let hosting = try XCTUnwrap(controller.panel.contentView)
            controller.show()
            defer { controller.panel.orderOut(nil) }
            let deadline = Date().addingTimeInterval(3)
            while queue(in: hosting) == nil, Date() < deadline {
                hosting.layoutSubtreeIfNeeded()
                try await Task.sleep(for: .milliseconds(20))
            }
            hosting.layoutSubtreeIfNeeded()
            let queue = try XCTUnwrap(queue(in: hosting))
            let actual = queue.convert(queue.bounds, to: hosting)
            let expectedY = hosting.isFlipped ? rect.minY : size.height - rect.maxY
            XCTAssertEqual(actual.minX, rect.minX, accuracy: 1, "\(edge) must keep its along-edge offset")
            XCTAssertEqual(actual.minY, expectedY, accuracy: 1, "\(edge) must remain parked at the window edge")
            XCTAssertEqual(actual.width, rect.width, accuracy: 1)
            XCTAssertEqual(actual.height, rect.height, accuracy: 1)
        }
    }

    @MainActor
    func testShortSideCardUsesItsExplicitFrameInsideALongQueueCanvas() async throws {
        _ = NSApplication.shared
        let domain = "app.agenthud.tests.dock-canvas.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        settings.panelLayout.setSwitches { $0.quotaDetails = false; $0.tokenUsage = false; $0.sessionList = false }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        let canvas = CGSize(width: 540, height: 400)
        let card = CGRect(x: 0, y: 200, width: 540, height: 120)
        let strip = CGRect(x: 508, y: 0, width: 32, height: 400)
        var root = IslandRootView(store: store, isOpen: true, collapsedSize: strip.size,
                                  collapsedTopRadius: 8, collapsedBottomRadius: 10,
                                  look: .standard, lightBorder: false, onOpenStats: {},
                                  additionalHUDControls: { AnyView(DockFooterMarker().frame(width: 16, height: 28)) })
        root.hidesSilhouette = true
        root.edge = .right
        root.presentationSize = canvas
        root.presentationFrame = card
        root.logoQueueFrame = strip
        root.logoQueue = LogoQueueConfig(items: (0..<12).map { _ in
            LogoQueueItem(vendor: "Claude", isWorking: false)
        }, placement: ScreenPlacement(mode: .logos, edge: .right, logoSize: 24), settings: settings.settings)
        root.animatesGeometry = false
        let controller = IslandWindowController(
            frame: CGRect(x: -20000, y: -20000, width: canvas.width, height: canvas.height), rootView: root)
        let hosting = try XCTUnwrap(controller.panel.contentView)
        controller.show()
        defer { controller.panel.orderOut(nil) }
        let deadline = Date().addingTimeInterval(3)
        while (footerMarker(in: hosting) == nil || queue(in: hosting) == nil), Date() < deadline {
            hosting.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }
        hosting.layoutSubtreeIfNeeded()
        let marker = try XCTUnwrap(footerMarker(in: hosting))
        let queue = try XCTUnwrap(queue(in: hosting))
        let footer = topLeftRect(marker.convert(marker.bounds, to: hosting), in: hosting)
        let queueRect = topLeftRect(queue.convert(queue.bounds, to: hosting), in: hosting)
        XCTAssertTrue(card.contains(footer), "Upright content must stay inside the short card, at its explicit y=200")
        XCTAssertEqual(footer.maxY, card.maxY - IslandRootView.expandedTopRadius - 14, accuracy: 1,
                       "The footer must use card height, rather than the entire queue canvas")
        XCTAssertEqual(queueRect, strip, "The independently anchored logos must retain the full 400pt canvas")

        let bitmap = try XCTUnwrap(hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds))
        hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
        let centreX = bitmap.pixelsWide / 2
        let occupied = (0..<bitmap.pixelsHigh).filter { (bitmap.colorAt(x: centreX, y: $0)?.alphaComponent ?? 0) > 0.9 }
        let scale = canvas.height / CGFloat(bitmap.pixelsHigh)
        XCTAssertEqual(CGFloat(try XCTUnwrap(occupied.first)) * scale,
                       card.minY + IslandRootView.expandedTopRadius, accuracy: 1,
                       "The black card must begin at its own shoulder, rather than being centred in the canvas")
        XCTAssertEqual(CGFloat(try XCTUnwrap(occupied.last)) * scale,
                       card.maxY - IslandRootView.expandedTopRadius, accuracy: 1)
        let lastMark = try XCTUnwrap(queue.layer?.sublayers?.last)
        let lastCentre = topLeftRect(queue.convert(lastMark.frame, to: hosting), in: hosting).midY
        XCTAssertGreaterThan(lastCentre, card.maxY, "The last logo deliberately sits below the card")
        let pixelX = Int(strip.midX * CGFloat(bitmap.pixelsWide) / canvas.width)
        let pixelY = Int(lastCentre / scale)
        XCTAssertGreaterThan(bitmap.colorAt(x: pixelX, y: pixelY)?.alphaComponent ?? 0, 0.9,
                             "A logo outside the card must remain visible instead of being clipped to its mask")
    }

    @MainActor
    private func topLeftRect(_ rect: CGRect, in hosting: NSView) -> CGRect {
        hosting.isFlipped ? rect : CGRect(x: rect.minX, y: hosting.bounds.height - rect.maxY,
                                         width: rect.width, height: rect.height)
    }

    @MainActor
    private func footerMarker(in view: NSView) -> DockFooterMarkerView? {
        if let marker = view as? DockFooterMarkerView { return marker }
        return view.subviews.lazy.compactMap { self.footerMarker(in: $0) }.first
    }

    @MainActor
    private func queue(in view: NSView) -> LogoQueueLayerView? {
        if let queue = view as? LogoQueueLayerView { return queue }
        return view.subviews.lazy.compactMap { self.queue(in: $0) }.first
    }
}

private struct DockFooterMarker: NSViewRepresentable {
    func makeNSView(context: Context) -> DockFooterMarkerView { DockFooterMarkerView() }
    func updateNSView(_ view: DockFooterMarkerView, context: Context) {}
}

private final class DockFooterMarkerView: NSView {}
