import AppKit
import AgentHUDCore
import SwiftUI

extension EnvironmentValues {
    /// Whether the hover panel is drawn as a picture for an agent: account names masked and session titles left out,
    /// as `IdentityMask` writes them, and text a provider or a client wrote drawn as placeholder bars. Off everywhere
    /// else, so the panel a person sees names them and says it.
    @Entry var masksIdentity = false
}

extension View {
    /// Text a provider or a client wrote, such as a reading's reason or a client's notice, which can quote a path, an
    /// account or whatever a process printed: on the panel's picture for an agent it is drawn as placeholder bars, in
    /// its own place and over its own lines, so the panel keeps its height; everywhere else it is drawn as it is.
    func redactedOnPicture() -> some View { modifier(PictureRedaction()) }
}

private struct PictureRedaction: ViewModifier {
    @Environment(\.masksIdentity) private var masksIdentity

    func body(content: Content) -> some View {
        content.redacted(reason: masksIdentity ? .placeholder : [])
    }
}

/// The hover panel drawn off screen for an agent that checks how a layout looks, and measured the same way for
/// `get_hud`: one screen's whole panel as the layout in effect there draws it, a draft included, at the width the
/// island gives it and with that display's insets and look, on the island's card with its rounded corners. The shared
/// layout is drawn with the main display's insets and look. Given an agent, it draws that agent's own panel instead,
/// the card a click on the agent's mark opens beside it on a dock that separates its agents: at the layout's
/// `agentPanelWidth`, with the insets of a card that keeps no strip clear and its corners, in the display's look.
///
/// It is the panel's own view in a hosting view that is never put in a window, measured the way the island measures
/// it before opening: nothing opens, nothing shows on screen and no read starts. It draws the panel's content alone,
/// with no glow, no notch silhouette, no alert card and no refresh error, and the picture masks account names, leaves
/// out session titles and draws the readings' reasons and the clients' notices as placeholder bars.
@MainActor
struct PanelPictureRenderer {
    /// How a display draws the whole panel: the room its content keeps from the card's edges, which the notch or a
    /// dock sets, the tallest the panel opens there, the screen's height less 80 points, and the look of its HUD.
    struct Display: Equatable {
        var insets: EdgeInsets
        var maximumHeight: CGFloat
        var look: HUDLook

        /// Beside the notch of the main screen, in the standard look, for when no island is up to ask.
        @MainActor static var main: Display {
            Display(insets: HoverPanelView.notchInsets, maximumHeight: (NSScreen.main?.frame.height ?? 900) - 80,
                    look: .standard)
        }
    }

    let store: UsageStore
    let display: Display
    /// The key of the screen whose layout is drawn; nil draws the shared layout.
    var screen: String? = nil
    /// The vendor whose own panel is drawn, as a dock that separates its agents opens it beside the agent's mark; nil
    /// draws the whole panel.
    var agent: String? = nil
    /// The tallest picture drawn, in pixels.
    var pixelLimit = PanelPicture.pixelLimit

    /// The panel's width, which the island gives it: the width of the layout drawn (`PanelStyle.panelWidth`), or on a
    /// side dock the width that keeps the content at its narrowest (`HoverPanelView.width(_:insets:)`); an agent's own
    /// panel's, 100 points narrower and never below 440 (`PanelStyle.agentPanelWidth`), as the card beside a mark is.
    var width: CGFloat {
        let style = store.settings.panelLayout.effective(for: screen).style
        guard agent == nil else { return CGFloat(style.agentPanelWidth) }
        return HoverPanelView.width(CGFloat(style.panelWidth), insets: insets)
    }

    /// The room the content keeps from the card's edges: the display's, or on an agent's own card, which floats beside
    /// its mark and covers no strip, a detached card's (`IslandRootView.detachedInsets`).
    var insets: EdgeInsets { agent == nil ? display.insets : IslandRootView.detachedInsets }

    /// The panel's height as the island opens it, at least 80 points, the tallest the display lets it open, and the
    /// width its content is laid out in there.
    func fit() -> PanelFit {
        let natural = IslandWindowController.naturalHeight(of: panel(masked: false))
        return PanelFit(height: Double(max(80, natural.rounded())), maximumHeight: Double(display.maximumHeight),
                        contentWidth: Double(width - insets.leading - insets.trailing))
    }

    /// The whole panel as a PNG, at `PanelPicture.pointScale` pixels a point or at the scale that keeps it within
    /// `pixelLimit` pixels tall, with account names masked, session titles left out and what providers and clients
    /// wrote as placeholder bars; nil when no bitmap can be made of it.
    func picture() -> PanelPicture? { draw(masked: true) }

    /// The picture, masked or as a person sees the panel.
    func draw(masked: Bool) -> PanelPicture? {
        let fit = fit()
        let size = CGSize(width: width, height: CGFloat(fit.height))
        let scale = PanelPicture.scale(forHeight: fit.height, limit: pixelLimit)
        let wide = Int((Double(size.width) * scale).rounded())
        let high = min(pixelLimit, Int((fit.height * scale).rounded()))
        guard wide > 0, high > 0,
              let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide, pixelsHigh: high, bitsPerSample: 8,
                                            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                            bytesPerRow: 0, bitsPerPixel: 0) else { return nil }
        bitmap.size = size
        let hosting = NSHostingView(rootView: card(masked: masked, height: size.height))
        hosting.appearance = NSAppearance(named: display.look.appearanceName)
        hosting.frame = CGRect(origin: .zero, size: size)
        hosting.layoutSubtreeIfNeeded()
        // The software renderer draws a capsule's continuous outline with a line past each tip; see `HUDRecorder`.
        if let layer = hosting.layer {
            HUDRecorder.withDrawableBorders(under: layer) { hosting.cacheDisplay(in: hosting.bounds, to: bitmap) }
        } else {
            hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
        }
        guard let png = bitmap.representation(using: .png, properties: [:]) else { return nil }
        return PanelPicture(png: png, width: Double(size.width), fit: fit, scale: scale)
    }

    /// The panel's own view as the island draws the whole of it, or the agent's own, in the display's look, without an
    /// alert or a refresh error.
    func panel(masked: Bool) -> some View {
        HoverPanelView(store: store, onOpenStats: {}, insets: insets, focusVendor: agent, showsRefreshError: false,
                       screen: screen)
            .frame(width: width, alignment: .top)
            .environment(\.masksIdentity, masked)
            .hudLook(display.look)
    }

    /// The panel on the island's card: the look's surface, rounded on every corner as the open island is at its foot,
    /// or as an agent's whole card is beside its mark (`ScreenHUD.detachedRadius(height:full:bubble:)`). Nothing is
    /// behind a picture to show through glass, so glass is drawn as its stand-in, the look's tint, over the backdrop the
    /// HUD's own pictures have (`HUDPictureRenderer.backdrop`).
    func card(masked: Bool, height: CGFloat) -> some View {
        let radius = agent == nil ? IslandController.expandedRadius
            : ScreenHUD.detachedRadius(height: height, full: true, bubble: false)
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        let look = display.look
        return panel(masked: masked)
            .frame(height: height, alignment: .top)
            .background {
                HUDSurface(shape, look: look, fill: look.theme.islandFill, contact: nil)
                    .background { if look.seeThrough { shape.fill(HUDPictureRenderer.backdrop) } }
                    .environment(\.drawsGlassStandIn, true)
            }
    }
}
