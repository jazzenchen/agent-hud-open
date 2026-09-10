import AppKit
import SwiftUI
import AgentHUDCore

/// Exercise actual island rows and their native tracking anchors in a non-key panel.
@MainActor
enum IslandHoverChecks {
    static func run(store: UsageStore, settings: SettingsStore, folder: URL) {
        let prefix = "island-forecast-hover-"
        if let filter = ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"], !prefix.hasPrefix(filter) { return }
        let now = Date()
        let agents = [
            AgentDescriptor(id: "session", vendor: "Claude", model: L10n.windowSession, source: "Snapshot", enabled: true),
            AgentDescriptor(id: "weekly", vendor: "Claude", model: L10n.windowWeekly, source: "Snapshot", enabled: true),
            AgentDescriptor(id: "fable", vendor: "Claude", model: L10n.windowWeeklyPrefix + "Fable", source: "Snapshot", enabled: true),
            AgentDescriptor(id: "codex", vendor: "Codex", model: L10n.windowWeekly, source: "Snapshot", enabled: true),
        ]
        settings.updateAgents { _ in agents }
        settings.update { $0.showIslandQuota = true; $0.showIslandTokens = false; $0.showIslandSessions = false; $0.showResetCountdown = true }
        let snapshots = agents.enumerated().map { index, agent in
            UsageSnapshot(agentId: agent.id, remainingPct: [67, 80, 60, 0][index],
                          resetAt: now.addingTimeInterval(index == 0 ? 5 * 3600 : 24 * 3600),
                          windowDuration: index == 0 ? 5 * 3600 : 7 * 86400, updatedAt: now)
        }
        func insights(rate: Double, remaining: Double) -> UsageInsights {
            UsageInsights(burnRatePctPerHour: rate, timeToExhaust: BurnRate(pctPerHour: rate).timeToExhaust(remainingPct: remaining),
                          weeklyCapHits: 0, weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                          weeklyShare: [:], windowSessionCount: 0, windowUsedPct: 100 - remaining)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: snapshots, sessions: [], history: [], activity: .empty, insights: .empty,
            insightsByAgent: ["session": insights(rate: 30, remaining: 67), "weekly": insights(rate: 1, remaining: 80)],
            codexResetCredits: DemoData.codexResetCredits(now: now)))

        func root(open: Bool) -> IslandRootView {
            IslandRootView(store: store, isOpen: open, collapsedSize: CGSize(width: 216, height: 32), collapsedTopRadius: 16,
                           collapsedBottomRadius: 12, lightBorder: false, onOpenStats: {})
        }
        let measured = NSHostingView(rootView: HoverPanelView(store: store, onOpenStats: {})
            .frame(width: NotchController.expandedWidth).fixedSize(horizontal: false, vertical: true))
        let size = CGSize(width: NotchController.expandedWidth + 2 * NotchGeometry.expandedTopRadius, height: measured.fittingSize.height)
        let window = NotchPanel(frame: CGRect(origin: CGPoint(x: -20000, y: -20000), size: size), level: .statusBar, acceptsMouse: true)
        let hosting = NSHostingView(rootView: root(open: true))
        hosting.sizingOptions = []
        hosting.frame = CGRect(origin: .zero, size: size)
        window.contentView = hosting
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        settle(hosting)
        precondition(!window.isKeyWindow && !window.canBecomeKey, "Hover must work without keyboard focus")

        let anchors: [IslandHoverAnchorView<QuotaForecastDetails>] = descendants(in: hosting)
            .sorted { $0.convert($0.bounds, to: nil).midY > $1.convert($1.bounds, to: nil).midY }
        precondition(anchors.count == agents.count, "Every timed model row needs an island hover anchor")
        SnapshotRunner.capture(prefix + "island", view: hosting, folder: folder)
        for (agent, anchor) in zip(agents, anchors) {
            let popup = checkEnter(anchor, in: window)
            let details = popup.contentView as! NSHostingView<QuotaForecastDetails>
            precondition(details.rootView.agent.id == agent.id, "The hovered row must use its own forecast")
            SnapshotRunner.capture(prefix + agent.id, view: details, folder: folder)
            checkFollowsPointer(anchor, popup: popup, in: window)
            checkExit(anchor, popup: popup, in: window)
        }

        let resets: [IslandHoverAnchorView<ResetCreditsDetails>] = descendants(in: hosting)
        precondition(resets.count == 1, "Reset credits retain their shared hover behavior")
        let resetPopup = checkEnter(resets[0], in: window)
        SnapshotRunner.capture(prefix + "resets", view: resetPopup.contentView!, folder: folder)
        checkExit(resets[0], popup: resetPopup, in: window)

        let popup = checkEnter(anchors[0], in: window)
        hosting.rootView = root(open: false)
        settle(hosting)
        RunLoop.current.run(until: Date().addingTimeInterval(0.45))
        settle(hosting)
        precondition(!popup.isVisible && popup.parent == nil, "Collapsing the island must dismiss the forecast")
        print("hover PASS: 4 model forecasts follow the pointer; rows and reset credits enter/exit in a non-key island; collapse dismisses the popup")
    }

    private static func checkEnter<Content: View>(_ anchor: IslandHoverAnchorView<Content>, in window: NSWindow) -> NSWindow {
        anchor.updateTrackingAreas()
        precondition(anchor.trackingAreas.contains { $0.options.contains([.activeAlways, .mouseMoved]) }, "Pointer tracking must remain active outside the key window")
        let originalSize = window.frame.size
        let point = anchor.convert(CGPoint(x: anchor.bounds.minX + 12, y: anchor.bounds.midY), to: nil)
        anchor.mouseEntered(with: event(.mouseEntered, window: window, at: point))
        settle(window.contentView!)
        let popup = window.childWindows?.first
        precondition(popup?.isVisible == true, "Entering a model row must show an island popup")
        precondition(window.frame.size == originalSize && !window.isKeyWindow, "Hover must not resize or focus the island")
        return popup!
    }

    private static func checkFollowsPointer<Content: View>(_ anchor: IslandHoverAnchorView<Content>, popup: NSWindow, in window: NSWindow) {
        for x in [24.0, 120.0] {
            let point = anchor.convert(CGPoint(x: anchor.bounds.minX + x, y: anchor.bounds.midY), to: nil)
            let moved = NSEvent.mouseEvent(with: .mouseMoved, location: point, modifierFlags: [],
                                          timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                          context: nil, eventNumber: 0, clickCount: 0, pressure: 0)!
            anchor.mouseMoved(with: moved)
            let screenPoint = window.convertPoint(toScreen: point)
            precondition(abs(popup.frame.minX - screenPoint.x - 10) < 1 && abs(popup.frame.maxY - screenPoint.y + 12) < 1,
                         "The forecast must stay beside the pointer, not at the row's right edge")
        }
    }

    private static func checkExit<Content: View>(_ anchor: IslandHoverAnchorView<Content>, popup: NSWindow, in window: NSWindow) {
        anchor.mouseExited(with: event(.mouseExited, window: window))
        settle(window.contentView!)
        precondition(!popup.isVisible && popup.parent == nil, "Leaving a model row must dismiss its popup")
    }

    private static func event(_ type: NSEvent.EventType, window: NSWindow, at point: CGPoint = .zero) -> NSEvent {
        NSEvent.enterExitEvent(with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                              windowNumber: window.windowNumber, context: nil, eventNumber: 0, trackingNumber: 0, userData: nil)!
    }

    private static func settle(_ view: NSView) {
        view.layoutSubtreeIfNeeded()
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        view.window?.displayIfNeeded()
    }

    private static func descendants<T: NSView>(in view: NSView) -> [T] {
        (view as? T).map { [$0] } ?? view.subviews.flatMap { descendants(in: $0) }
    }
}
