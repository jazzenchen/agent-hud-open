import SwiftUI

/// A host-provided page presented alongside the built-in desktop settings.
@MainActor
public struct DesktopSettingsPage: Identifiable {
    public let id: String
    public var title: () -> String
    public var heading: (() -> String)?
    public var subtitle: () -> String
    public var symbol: String
    public var color: Color
    public var preferredContentWidth: CGFloat?
    let content: () -> AnyView

    public init<Content: View>(id: String, title: @escaping () -> String,
                               heading: (() -> String)? = nil,
                               subtitle: @escaping () -> String, symbol: String, color: Color,
                               preferredContentWidth: CGFloat? = nil,
                               @ViewBuilder content: @escaping () -> Content) {
        self.id = id
        self.title = title
        self.heading = heading
        self.subtitle = subtitle
        self.symbol = symbol
        self.color = color
        self.preferredContentWidth = preferredContentWidth
        self.content = { AnyView(content()) }
    }
}
