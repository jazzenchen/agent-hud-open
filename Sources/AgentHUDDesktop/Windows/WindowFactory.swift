import AppKit
import SwiftUI

/// Standard windows with a transparent title bar so the SwiftUI content owns the whole surface
/// (traffic lights stay native and sit over our sidebar/header, as in the design).
enum WindowFactory {
    static func make(size: CGSize, title: String, resizable: Bool = false) -> NSWindow {
        var mask: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .fullSizeContentView]
        if resizable { mask.insert(.resizable) }
        let window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: mask, backing: .buffered, defer: false)
        window.title = title
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        window.standardWindowButton(.zoomButton)?.isEnabled = resizable
        window.center()
        return window
    }
}

/// Hosts one SwiftUI root view in a factory window.
public class HostedWindowController: NSWindowController, NSWindowDelegate {
    private let hosting: NSHostingView<AnyView>
    private let baseSize: CGSize

    public init<Content: View>(size: CGSize, title: String, resizable: Bool = false, fitToContent: Bool = false, content: Content) {
        let window = WindowFactory.make(size: size, title: title, resizable: resizable)
        baseSize = size
        hosting = NSHostingView(rootView: AnyView(content.ignoresSafeArea()))
        hosting.sizingOptions = []
        hosting.frame = CGRect(origin: .zero, size: window.contentLayoutRect.size)
        hosting.autoresizingMask = [.width, .height]
        window.contentView = hosting
        super.init(window: window)
        window.delegate = self
        setContent(content, fitToContent: fitToContent)
        window.center()
    }

    @available(*, unavailable)
    public required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    /// Replaces the root view; with `fitToContent` the window grows to the view's ideal height.
    func setContent<Content: View>(_ content: Content, fitToContent: Bool = false) {
        hosting.rootView = AnyView(content.ignoresSafeArea())
        guard fitToContent, let window else { return }
        hosting.sizingOptions = [.intrinsicContentSize]
        let fitting = hosting.fittingSize
        hosting.sizingOptions = []
        if fitting.height > 0 {
            window.setContentSize(CGSize(width: baseSize.width, height: max(baseSize.height, fitting.height)))
        }
    }

    public func show() {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        if let window, !window.isVisible { window.center() }
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }

    public func windowWillClose(_ notification: Notification) {
        // The closing window is still visible during this callback. Minimized windows
        // remain open and must keep the app available in the Dock and app switcher.
        let hasOtherOpenWindow = NSApp.windows.contains {
            $0 !== window && $0.windowController is HostedWindowController
                && ($0.isVisible || $0.isMiniaturized)
        }
        if !hasOtherOpenWindow {
            NSApp.setActivationPolicy(.accessory)
        }
    }
}
