import AppKit

/// Borderless, non-activating overlay used for both the glow and the island.
/// Lives above the menu bar on every Space, including full-screen apps, and never takes focus.
final class NotchPanel: NSPanel {
    init(frame: CGRect, level: NSWindow.Level, acceptsMouse: Bool) {
        super.init(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        self.level = level
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = !acceptsMouse
        acceptsMouseMovedEvents = acceptsMouse
        collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary, .ignoresCycle]
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        isMovable = false
        isMovableByWindowBackground = false
        animationBehavior = .none
        isExcludedFromWindowsMenu = true
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    /// Allow the frame to touch (or exceed) the screen's top edge; AppKit would otherwise push it below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}
