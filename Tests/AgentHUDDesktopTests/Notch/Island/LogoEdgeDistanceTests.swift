import AppKit
import AgentHUDCore
import XCTest
@testable import AgentHUDDesktop

@MainActor
final class LogoEdgeDistanceTests: XCTestCase {
    private let screen = CGRect(x: -1600, y: -200, width: 1600, height: 900)

    func testChangingLogoSizeKeepsItsVisibleEdgeAtTheConfiguredDistanceOnEveryEdgeAndScale() throws {
        let settings = Settings()
        for edge in HUDEdge.allCases {
            for scale in [CGFloat(1), 2] {
                let view = LogoQueueLayerView()
                for size in [20.0, 30, 36, 60] {
                    for distance in [0.0, 2, 20] {
                        let placement = ScreenPlacement(edge: edge, offset: 0.35, logoSize: size,
                                                        logoEdgeDistance: distance, gapScale: 0.35)
                        let config = LogoQueueConfig(items: [
                            LogoQueueItem(vendor: "Claude", isWorking: false),
                            LogoQueueItem(vendor: "DeepSeek", isWorking: false),
                            LogoQueueItem(vendor: "GitHub Copilot", isWorking: false),
                            LogoQueueItem(vendor: "OpenCode", isWorking: false),
                        ], placement: placement, settings: settings)
                        let strip = NotchGeometry.stripRect(queue: config.size, frame: screen, placement: placement)
                        view.setFrameSize(strip.size)
                        view.apply(config: config, light: false, scale: scale, animates: false)
                        for (index, mark) in view.marks.enumerated() {
                            let visible = try visibleRect(of: mark)
                            let global = CGRect(x: strip.minX + visible.minX, y: strip.maxY - visible.maxY,
                                                width: visible.width, height: visible.height)
                            XCTAssertEqual(edgeDistance(global, edge: edge, screen: screen), distance, accuracy: 0.000001,
                                           "\(config.items[index].vendor), \(edge), size \(size), distance \(distance), scale \(scale)")
                            XCTAssertEqual(mark.frame.width - 2 / scale, size, accuracy: 0.000001)
                            XCTAssertEqual(mark.frame.height - 2 / scale, size, accuracy: 0.000001)
                            if let actor = view.actors[index] {
                                XCTAssertEqual(actor.layer.position.x, mark.frame.midX, accuracy: 0.000001)
                                XCTAssertEqual(actor.layer.position.y, mark.frame.midY, accuracy: 0.000001)
                            }
                        }
                        let nominal = view.marks[0].frame.insetBy(dx: 1 / scale, dy: 1 / scale)
                        let next = view.marks[1].frame.insetBy(dx: 1 / scale, dy: 1 / scale)
                        let gap = edge.isHorizontal ? next.minX - nominal.maxX : next.minY - nominal.maxY
                        XCTAssertEqual(gap, size * placement.gapScale, accuracy: 0.000001,
                                       "Screen-edge distance must not change inter-logo spacing")
                    }
                }
            }
        }
    }

    func testLogoStripThicknessDoesNotFollowTheMenuBar() {
        for size in [20.0, 30, 36] {
            let placement = ScreenPlacement(logoSize: size, logoEdgeDistance: 0)
            let config = LogoQueueConfig(items: [LogoQueueItem(vendor: "Claude", isWorking: false)],
                                         placement: placement, settings: Settings())
            let detected = NotchGeometry.detect(screen: nil, placement: placement, queue: config.size)
            XCTAssertEqual(detected.menuBarHeight, 24)
            XCTAssertEqual(detected.rect.height, size,
                           "A 20pt logo must remain 20pt thick, and a 30pt logo must reach the screen edge")
        }
    }

    func testOpeningAndClosingCanvasKeepsThirtyPointLogosAtZeroDistanceOnEveryEdge() async throws {
        _ = NSApplication.shared
        let display = CGRect(x: -20000, y: -20000, width: 1200, height: 900)
        var settings = Settings()
        settings.botLogos = false
        for edge in HUDEdge.allCases {
            let placement = ScreenPlacement(edge: edge, logoSize: 30, logoEdgeDistance: 0)
            let config = LogoQueueConfig(items: [LogoQueueItem(vendor: "DeepSeek", isWorking: false)],
                                         placement: placement, settings: settings)
            let strip = NotchGeometry.stripRect(queue: config.size, frame: display, placement: placement)
            let geometry = NotchGeometry(screenFrame: display, mode: .logos, edge: edge, hasNotch: false,
                                         rect: strip, cornerRadius: 10, backingScale: 2, menuBarHeight: 38)
            let closed = geometry.islandFrame
            let expanded = geometry.expandedFrame(size: CGSize(width: 540, height: 300))
            var root = IslandRootView(store: nil, isOpen: false, collapsedSize: strip.size,
                                      collapsedTopRadius: 8, collapsedBottomRadius: 10,
                                      look: .standard, lightBorder: false, onOpenStats: {})
            root.hidesSilhouette = true
            root.edge = edge
            root.logoQueue = config
            root.logoQueueFrame = CGRect(x: strip.minX - closed.minX, y: closed.maxY - strip.maxY,
                                         width: strip.width, height: strip.height)
            root.animatesGeometry = false
            let controller = IslandWindowController(frame: closed, rootView: root)
            controller.show()
            defer { controller.panel.orderOut(nil) }
            let hosting = try XCTUnwrap(controller.panel.contentView)
            for canvas in [closed, closed.union(expanded), expanded, closed.union(expanded), closed] {
                controller.setFrame(canvas)
                let rebased = controller.rootView
                var current = IslandRootView(store: rebased.store, isOpen: canvas != closed,
                                             collapsedSize: rebased.collapsedSize,
                                             collapsedTopRadius: rebased.collapsedTopRadius,
                                             collapsedBottomRadius: rebased.collapsedBottomRadius,
                                             look: rebased.look, lightBorder: rebased.lightBorder,
                                             onOpenStats: rebased.onOpenStats)
                current.hidesSilhouette = rebased.hidesSilhouette
                current.edge = rebased.edge
                current.logoQueue = rebased.logoQueue
                current.logoQueueFrame = rebased.logoQueueFrame
                current.animatesGeometry = false
                controller.setRootView(current)
                let deadline = Date().addingTimeInterval(3)
                while queue(in: hosting) == nil, Date() < deadline {
                    hosting.layoutSubtreeIfNeeded()
                    try await Task.sleep(for: .milliseconds(20))
                }
                hosting.layoutSubtreeIfNeeded()
                let view = try XCTUnwrap(queue(in: hosting))
                let mark = try XCTUnwrap(view.marks.first)
                let local = view.convert(try visibleRect(of: mark), to: hosting)
                let global = CGRect(x: canvas.minX + local.minX,
                                    y: hosting.isFlipped ? canvas.maxY - local.maxY : canvas.minY + local.minY,
                                    width: local.width, height: local.height)
                XCTAssertEqual(edgeDistance(global, edge: edge, screen: display), 0, accuracy: 0.5,
                               "\(edge): canvas changes must not move the logo off its screen edge")
                XCTAssertEqual(mark.frame.width - 2 / mark.contentsScale, 30, accuracy: 0.000001)
                XCTAssertEqual(mark.frame.height - 2 / mark.contentsScale, 30, accuracy: 0.000001)
            }
        }
    }

    /// Read the actual baked pixels independently of the production bounds cache.
    private func visibleRect(of mark: CALayer) throws -> CGRect {
        let contents = try XCTUnwrap(mark.contents)
        let image = contents as! CGImage
        let bitmap = NSBitmapImageRep(cgImage: image)
        var minX = image.width, minY = image.height, maxX = -1, maxY = -1
        for y in 0..<image.height {
            for x in 0..<image.width where (bitmap.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0 {
                minX = min(minX, x); minY = min(minY, y)
                maxX = max(maxX, x); maxY = max(maxY, y)
            }
        }
        XCTAssertGreaterThanOrEqual(maxX, minX, "The fixture must contain visible pixels")
        let sx = mark.frame.width / CGFloat(image.width), sy = mark.frame.height / CGFloat(image.height)
        return CGRect(x: mark.frame.minX + CGFloat(minX) * sx,
                      y: mark.frame.minY + CGFloat(minY) * sy,
                      width: CGFloat(maxX - minX + 1) * sx, height: CGFloat(maxY - minY + 1) * sy)
    }

    private func edgeDistance(_ rect: CGRect, edge: HUDEdge, screen: CGRect) -> CGFloat {
        switch edge {
        case .top: screen.maxY - rect.maxY
        case .bottom: rect.minY - screen.minY
        case .left: rect.minX - screen.minX
        case .right: screen.maxX - rect.maxX
        }
    }

    private func queue(in view: NSView) -> LogoQueueLayerView? {
        if let queue = view as? LogoQueueLayerView { return queue }
        return view.subviews.lazy.compactMap { self.queue(in: $0) }.first
    }
}
