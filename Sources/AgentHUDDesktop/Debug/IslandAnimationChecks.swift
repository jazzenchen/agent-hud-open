import AppKit
import SwiftUI
import AgentHUDCore

/// Opt-in real-window regression check: AGENTHUD_SNAPSHOT_PREFIX=island-animation.
@MainActor
enum IslandAnimationChecks {
    static func run(store: UsageStore, settings: SettingsStore, folder: URL) async {
        guard ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"] == "island-animation" else { return }
        settings.update { $0.collapseDelayMs = 5000; $0.showIslandQuota = true; $0.showIslandTokens = false }
        settings.updateAgents { _ in DemoData.agents }
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))
        let controller = NotchController(store: store, settings: settings)
        let window = NSApp.windows.first { $0.title == "Agent HUD Island" }!
        let glowWindow = NSApp.windows.first { $0 !== window && $0 is NotchPanel && $0.ignoresMouseEvents }!
        defer { window.orderOut(nil); glowWindow.orderOut(nil) }
        await settle(0.1)

        func captureEndpoint(_ label: String) {
            let view = window.contentView!
            view.layoutSubtreeIfNeeded()
            guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { preconditionFailure("Missing island bitmap") }
            view.cacheDisplay(in: view.bounds, to: rep)
            let x = rep.pixelsWide / 2
            let occupied = (0..<rep.pixelsHigh).filter { (rep.colorAt(x: x, y: $0)?.alphaComponent ?? 0) > 0.9 }
            let height = CGFloat(occupied.count) * view.bounds.height / CGFloat(rep.pixelsHigh)
            let glowLayer = glowWindow.contentView!.layer!.sublayers![1]
            let visible = (glowLayer.presentation() ?? glowLayer).frame
            let glowHeight = visible.height - ceil(settings.settings.glowBlur * 3) * 2
                - settings.settings.glowRange - settings.settings.glowBlur * 3
            precondition(abs(height - glowHeight) < 1, "The island and glow must settle to the same contour")
            SnapshotRunner.capture("island-animation-\(label)", view: view, folder: folder)
        }

        func measure() -> CGFloat {
            NSHostingView(rootView: HoverPanelView(store: store, onOpenStats: {})
                .frame(width: NotchController.expandedWidth).fixedSize(horizontal: false, vertical: true)).fittingSize.height.rounded()
        }

        let expectedHeight = measure()
        controller.forceOpen()
        precondition(window.frame.height == expectedHeight, "Opening must use the measured height immediately")
        for _ in 0..<8 {
            await settle(0.05)
            precondition(window.frame.height == expectedHeight, "Content measurement must not resize the canvas mid-animation")
        }
        await settle(0.1)
        captureEndpoint("open")

        controller.forceCollapse()
        await settle(0.1)
        precondition(window.frame.height == expectedHeight, "Closing must retain its canvas while the silhouette shrinks")
        controller.forceOpen()
        await settle(0.5)
        precondition(window.frame.height == expectedHeight, "Reopening must cancel the pending canvas shrink")

        settings.update { $0.showIslandQuota = false }
        await settle(0.1)
        let shorterHeight = max(80, measure())
        precondition(window.frame.height == expectedHeight, "Live content changes must retain the transition canvas")
        await settle(0.5)
        precondition(window.frame.height == shorterHeight, "Live content height must settle to its measured target: \(window.frame.height) vs \(shorterHeight)")
        controller.forceCollapse()
        await settle(0.5)
        precondition(window.frame == controller.geometry.islandFrame, "Closing must finish at the notch frame")
        captureEndpoint("closed")
        await checkTallGlowCanvas()
        print("animation PASS: measured expansion, stable canvas, interrupted collapse, live resize, final collapse and unclipped tall-panel glow")
    }

    private static func checkTallGlowCanvas() async {
        for screenHeight: CGFloat in [900, 1117, 1440] {
            let screen = CGRect(x: -20000, y: -20000, width: 1728, height: screenHeight)
            let geometry = NotchGeometry(screenFrame: screen, hasNotch: true,
                rect: CGRect(x: screen.midX - 108, y: screen.maxY - 32, width: 216, height: 32),
                cornerRadius: 12, backingScale: 2)
            let controller = GlowWindowController(geometry: geometry)
            defer { controller.panel.orderOut(nil) }
            let canvas = controller.panel.frame
            let maximum = Settings.glowSizeRange.upperBound

            func update(height: CGFloat, animated: Bool) {
                let island = geometry.expandedFrame(size: CGSize(width: NotchController.expandedWidth, height: height))
                let glow = GlowGeometry.compute(islandWidth: island.width, islandHeight: island.height,
                    islandRadius: NotchController.expandedRadius, range: maximum, blur: maximum)
                controller.update(geometry: geometry, island: island, islandRadius: NotchController.expandedRadius,
                    glow: glow, outwardOnly: true, appearance: .idle(), animated: animated)
            }

            func checkClipping() {
                precondition(controller.panel.frame == canvas, "Glow canvas must stay fixed during expansion and collapse")
                let host = controller.panel.contentView!
                for layer in host.layer!.sublayers!.prefix(2) {
                    let frame = (layer.presentation() ?? layer).frame
                    // Only the portion above the display's top edge may be clipped.
                    precondition(frame.minY >= host.bounds.minY - 0.01 && frame.minX >= host.bounds.minX
                        && frame.maxX <= host.bounds.maxX, "Tall panels must retain the full bottom glow and shadow")
                }
            }

            update(height: screenHeight - 80, animated: false)
            checkClipping()
            update(height: NotchController.defaultPanelHeight, animated: true)
            await settle(0.1)
            checkClipping()
            update(height: screenHeight - 80, animated: true)
            await settle(IslandAnimation.duration + 0.1)
            checkClipping()
        }
    }

    private static func settle(_ seconds: TimeInterval) async {
        try? await Task.sleep(for: .seconds(seconds))
    }
}
