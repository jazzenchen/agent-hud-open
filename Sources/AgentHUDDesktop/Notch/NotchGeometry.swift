import AppKit

/// Where the notch (or its stand-in on notch-less displays) sits, in global screen coordinates.
struct NotchGeometry: Equatable {
    let screenFrame: CGRect
    let hasNotch: Bool
    /// The physical notch rect; on displays without a notch a floating bar of `fallbackWidth` centred on the menu bar.
    let rect: CGRect
    /// Convex radius of the bottom corners.
    let cornerRadius: CGFloat
    let backingScale: CGFloat

    static let fallbackWidth: CGFloat = 200
    static let notchCornerRadius: CGFloat = 12
    static let fallbackCornerRadius: CGFloat = 10
    /// Concave flare where the island meets the screen edge, collapsed / expanded.
    static let collapsedTopRadius: CGFloat = 8
    static let expandedTopRadius: CGFloat = 16

    static func detect(screens: [NSScreen] = NSScreen.screens, main: NSScreen? = NSScreen.main) -> NotchGeometry {
        if let screen = screens.first(where: { $0.safeAreaInsets.top > 0 }) {
            let frame = screen.frame
            let height = screen.safeAreaInsets.top
            let left = screen.auxiliaryTopLeftArea?.maxX ?? (frame.midX - fallbackWidth / 2)
            let right = screen.auxiliaryTopRightArea?.minX ?? (frame.midX + fallbackWidth / 2)
            let rect = CGRect(x: left, y: frame.maxY - height, width: max(1, right - left), height: height)
            return NotchGeometry(screenFrame: frame, hasNotch: true, rect: rect, cornerRadius: notchCornerRadius, backingScale: screen.backingScaleFactor)
        }
        let screen = main ?? screens.first
        let frame = screen?.frame ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let menuBarHeight = screen.map { max(22, $0.frame.maxY - $0.visibleFrame.maxY) } ?? 24
        let rect = CGRect(x: frame.midX - fallbackWidth / 2, y: frame.maxY - menuBarHeight, width: fallbackWidth, height: menuBarHeight)
        return NotchGeometry(screenFrame: frame, hasNotch: false, rect: rect, cornerRadius: fallbackCornerRadius, backingScale: screen?.backingScaleFactor ?? 2)
    }

    var centerX: CGFloat { rect.midX }
    var top: CGFloat { screenFrame.maxY }

    /// Collapsed island window: the notch plus the top flares on both sides.
    var islandFrame: CGRect {
        rect.insetBy(dx: -Self.collapsedTopRadius, dy: 0)
    }

    /// Core of the expanded panel (content area): same top edge, centred on the notch.
    func expandedFrame(size: CGSize) -> CGRect {
        CGRect(x: centerX - size.width / 2, y: top - size.height, width: size.width, height: size.height)
    }

    /// Expanded island window: content area plus the top flares.
    func expandedWindowFrame(size: CGSize) -> CGRect {
        expandedFrame(size: size).insetBy(dx: -Self.expandedTopRadius, dy: 0)
    }
}
