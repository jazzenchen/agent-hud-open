import AppKit
import SwiftUI

/// The black island itself. Collapsed, the panel is exactly the notch. Opening sets the panel to the expanded content size
/// immediately and lets SwiftUI grow the shape out of the notch; closing shrinks the shape first, then the panel.
@MainActor
final class IslandWindowController {
    let panel: NotchPanel
    private let hosting: TrackingHostingView
    private var measurement: NSHostingView<AnyView>?

    var onPointerChange: ((Bool) -> Void)? {
        didSet { hosting.onPointerChange = onPointerChange }
    }

    init(frame: CGRect, rootView: IslandRootView) {
        panel = NotchPanel(frame: frame, level: NSWindow.Level(rawValue: NSWindow.Level.statusBar.rawValue + 1), acceptsMouse: true)
        panel.title = "Agent HUD Island"
        hosting = TrackingHostingView(rootView: rootView)
        hosting.sizingOptions = []
        hosting.frame = CGRect(origin: .zero, size: frame.size)
        hosting.autoresizingMask = [.width, .height]
        panel.contentView = hosting
    }

    func setRootView(_ view: IslandRootView) {
        hosting.rootView = view
        // Commit SwiftUI's layout in the same run-loop turn as the glow layers.
        hosting.layoutSubtreeIfNeeded()
    }

    /// Resolve the natural height before expansion so the first frame has its final target.
    func contentHeight(for view: IslandRootView) -> CGFloat {
        let content = AnyView(view.content.fixedSize(horizontal: false, vertical: true))
        if let measurement {
            measurement.rootView = content
        } else {
            measurement = NSHostingView(rootView: content)
        }
        return measurement!.fittingSize.height
    }

    func setFrame(_ frame: CGRect) {
        guard panel.frame != frame else { return }
        panel.setFrame(frame, display: true)
    }

    /// Part of the window that counts as "the island" for hover purposes (nil = whole window).
    func setVisibleSize(_ size: CGSize?) {
        hosting.visibleSize = size
    }

    func show() {
        panel.orderFrontRegardless()
    }
}

/// Hosting view that reports pointer enter/exit for the visible island shape and accepts the first click
/// without activating the app.
final class TrackingHostingView: NSHostingView<IslandRootView> {
    var onPointerChange: ((Bool) -> Void)?
    /// When set, only a top-centred rect of this size counts as inside.
    var visibleSize: CGSize? {
        didSet { if oldValue != visibleSize { reevaluate(with: nil) } }
    }

    private var trackingArea: NSTrackingArea?
    private var inside = false

    required init(rootView: IslandRootView) {
        super.init(rootView: rootView)
    }

    @available(*, unavailable)
    @objc required dynamic init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea { removeTrackingArea(trackingArea) }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        super.mouseEntered(with: event)
        reevaluate(with: event)
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        reevaluate(with: event)
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        report(false)
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    private func reevaluate(with event: NSEvent?) {
        let point: CGPoint
        if let event {
            point = convert(event.locationInWindow, from: nil)
        } else if let window {
            point = convert(window.mouseLocationOutsideOfEventStream, from: nil)
        } else {
            return
        }
        report(islandHitRect.contains(point))
    }

    private var islandHitRect: CGRect {
        guard let visibleSize else { return bounds }
        let width = min(visibleSize.width, bounds.width)
        let height = min(visibleSize.height, bounds.height)
        let y = isFlipped ? 0 : bounds.height - height
        return CGRect(x: (bounds.width - width) / 2, y: y, width: width, height: height)
    }

    private func report(_ isInside: Bool) {
        guard isInside != inside else { return }
        inside = isInside
        onPointerChange?(isInside)
    }
}
