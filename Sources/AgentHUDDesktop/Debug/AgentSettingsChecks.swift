import AppKit
import SwiftUI
import AgentHUDCore

/// Exercises native pointer interactions in the snapshot's isolated settings, without polling or notifying.
@MainActor
enum AgentSettingsChecks {
    static func run(settings: SettingsStore, store: UsageStore, sources: [SourceStatus]) async {
        let hosting = NSHostingView(rootView: AnyView(SettingsView(settings: settings, store: store, initialTab: .sources,
                                                          sourceStatuses: sources).frame(width: 760, height: 800)))
        hosting.sizingOptions = []
        hosting.frame = NSRect(x: 0, y: 0, width: 760, height: 800)
        let window = OffscreenWindow(contentRect: NSRect(x: -20000, y: -20000, width: 760, height: 800))
        window.contentView = hosting
        window.acceptsMouseMovedEvents = true
        window.orderFrontRegardless()
        window.makeKey()
        defer { window.orderOut(nil) }
        func settle() async {
            try? await Task.sleep(for: .milliseconds(250))
            hosting.layoutSubtreeIfNeeded()
        }
        var eventNumber = 0
        func click(x: CGFloat, yFromTop: CGFloat) {
            func event(_ type: NSEvent.EventType) -> NSEvent? {
                eventNumber += 1
                return NSEvent.mouseEvent(with: type, location: CGPoint(x: x, y: 800 - yFromTop),
                    modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                    context: nil, eventNumber: eventNumber, clickCount: 1, pressure: type == .leftMouseDown ? 1 : 0)
            }
            guard let moved = event(.mouseMoved), let down = event(.leftMouseDown) else { return }
            window.sendEvent(moved)
            // AppKit tracks a press synchronously and consumes its release from the event queue.
            guard let up = event(.leftMouseUp) else { return }
            NSApp.postEvent(up, atStart: false)
            window.sendEvent(down)
        }
        func scrollView(_ view: NSView) -> NSScrollView? {
            if let scroll = view as? NSScrollView { return scroll }
            return view.subviews.compactMap(scrollView).first
        }
        func documentHeight() -> CGFloat { scrollView(hosting)?.documentView?.bounds.height ?? 0 }
        func check(_ passed: Bool, _ message: String) {
            print("settings interaction \(passed ? "PASS" : "FAILED"): \(message)")
        }
        await settle()
        let collapsedHeight = documentHeight()
        check(collapsedHeight > 0, "settings content renders")
        // Positions correspond to the adjacent 760 x 800 reference snapshots.
        click(x: 450, yFromTop: 130)
        await settle()
        check(documentHeight() > collapsedHeight + 100, "group expands to show windows")
        click(x: 700, yFromTop: 195)
        await settle()
        check(settings.agents.first { $0.id == "settings-claude-5h" }?.enabled == false,
              "display switch changes the stored window selection")
        let group = AgentSettingsGroup.make(sources: sources, agents: settings.agents).first { $0.id == "Claude" }
        check(group?.displayedCount == 1 && group?.agents.count == 3, "display count changes to 1/3")
        if let scroll = scrollView(hosting) {
            scroll.contentView.scroll(to: .zero)
            scroll.reflectScrolledClipView(scroll.contentView)
        }
        await settle()
        click(x: 450, yFromTop: 130)
        await settle()
        let finalHeight = documentHeight()
        check(abs(finalHeight - collapsedHeight) < 2, "group collapses again (\(collapsedHeight) → \(finalHeight))")

    }
}
