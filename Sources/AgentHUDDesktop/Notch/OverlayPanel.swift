import AppKit

/// Borderless, non-activating overlay used for the glow, the island and its hover popups.
/// Lives above the menu bar on every Space, including full-screen apps, and never takes focus — except while the user
/// is typing into a field on it, and then without activating the app, so the app underneath stays in front.
final class OverlayPanel: NSPanel {
    private var takesKeyboard = false

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

    override var canBecomeKey: Bool { takesKeyboard }
    override var canBecomeMain: Bool { false }

    /// Takes the keyboard for a field the user clicked into.
    func takeKeyboard() {
        takesKeyboard = true
        makeKey()
    }

    /// Hands the keyboard back to the app it was taken from. That app was never deactivated, so the panel only has to
    /// stop being key: ordering it out and straight back in lets the window server give the keys back to it.
    func releaseKeyboard() {
        guard takesKeyboard else { return }
        takesKeyboard = false
        guard isKeyWindow else { return }
        makeFirstResponder(nil)
        orderOut(nil)
        orderFrontRegardless()
    }

    /// Clicking into another app takes the keyboard back by itself, and ends the typing with it.
    override func resignKey() {
        super.resignKey()
        guard takesKeyboard else { return }
        takesKeyboard = false
        makeFirstResponder(nil)
    }

    /// Allow the frame to touch (or exceed) the screen's top edge; AppKit would otherwise push it below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}
