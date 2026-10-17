import AppKit
import AgentHUDCore
import SwiftUI

/// One screen's resting HUD drawn off screen for an agent that checks how a style looks, as `get_hud_screenshot`
/// returns it with show "hud": the island at the notch with its glow, a Dynamic dock's marks over their backdrop, or a
/// Simple panel, as that screen draws it now with any style draft an agent proposed, on a dark backdrop. It is the
/// Settings preview of that screen: a dock is laid along a top edge whichever edge it is parked on, and the island has
/// the screen's own notch, or the bar that stands in for one.
///
/// Like the panel's picture it is views in a hosting view that is never put in a window: nothing opens, nothing shows
/// on screen and no read starts. Nothing is behind it to show through glass, which is drawn as its stand-in, the look's
/// tint over the backdrop. The glow is drawn as a still frame, the one the snapshots draw, which every build draws the
/// same way. The marks of a dock and of a Simple panel follow Bot logos, as they do on screen. It shows marks and
/// figures, and no account or session.
@MainActor
struct HUDPictureRenderer {
    let store: UsageStore
    /// The key of the screen whose HUD is drawn.
    let screen: String
    /// Whether that screen has a notch, which decides the placement it draws without one of its own.
    let hasNotch: Bool
    /// The look of that screen's HUD, which the picture draws it in.
    let look: HUDLook
    /// The screen's measurements, for the island's size beside the notch.
    var metrics = ScreenMetrics.fallback

    /// Room around the HUD on the backdrop, in points.
    static let margin: CGFloat = 16
    /// The backdrop: dark enough for a glow to read, light enough for the black island to stand off it. Glass's stand-in
    /// is drawn over it, here and on the panel's picture (`PanelPictureRenderer.card(masked:height:)`).
    static let backdrop = Color(white: 0.2)
    /// The moment of a moving glow or bot the still is drawn at, the snapshots' own.
    static let frozenTime = 0.9

    private var settings: SettingsStore { store.settings }

    /// The placement the screen's HUD draws now, its style draft included.
    var placement: ScreenPlacement { settings.effectivePlacement(on: screen, hasNotch: hasNotch) }

    /// What the screen's resting HUD shows of the agents, under the layout it draws.
    private var agents: ScreenAgents {
        ScreenAgents(store: store, listed: settings.panelLayout.effective(for: screen).hud.agents, mode: placement.mode)
    }

    /// The placement as the picture lays it out: along a top edge, as Settings previews a dock.
    private var laidAlongTop: ScreenPlacement {
        var sample = placement
        sample.edge = .top
        return sample
    }

    /// Whether the picture holds the HUD's glow. A Simple panel draws none.
    var drawsGlow: Bool { placement.mode != .simple }

    /// The resting HUD as a PNG at `PanelPicture.pointScale` pixels a point; nil when no bitmap can be made of it.
    func picture() -> HUDPicture? {
        let hosting = NSHostingView(rootView: view())
        hosting.appearance = NSAppearance(named: look.appearanceName)
        let size = hosting.fittingSize
        guard size.width > 0, size.height > 0 else { return nil }
        let scale = CGFloat(PanelPicture.pointScale)
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int((size.width * scale).rounded()),
                                            pixelsHigh: Int((size.height * scale).rounded()), bitsPerSample: 8,
                                            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                            bytesPerRow: 0, bitsPerPixel: 0) else { return nil }
        bitmap.size = size
        hosting.frame = CGRect(origin: .zero, size: size)
        hosting.layoutSubtreeIfNeeded()
        // The software renderer draws a capsule's continuous outline with a line past each tip; see `HUDRecorder`.
        if let layer = hosting.layer {
            HUDRecorder.withDrawableBorders(under: layer) { hosting.cacheDisplay(in: hosting.bounds, to: bitmap) }
        } else {
            hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
        }
        guard let png = bitmap.representation(using: .png, properties: [:]) else { return nil }
        return HUDPicture(png: png, width: Double(size.width), height: Double(size.height), scale: Double(scale),
                          drawsGlow: drawsGlow)
    }

    /// The HUD on its backdrop, sized to what it holds. The island and a dock hang from the picture's top edge, which
    /// stands for the screen's; the Simple panel floats, with room all round.
    func view() -> some View {
        hud
            .padding(EdgeInsets(top: placement.mode == .simple ? Self.margin : 0, leading: Self.margin, bottom: Self.margin,
                                trailing: Self.margin))
            .background(Self.backdrop)
            .environment(\.glowFrozenTime, Self.frozenTime)
            .environment(\.masksIdentity, true)
            .environment(\.drawsGlassStandIn, true)
            .hudLook(look)
    }

    @ViewBuilder
    private var hud: some View {
        switch placement.mode {
        case .notch: notch
        case .logos: dock
        case .simple: simple
        }
    }

    /// The island at the top of the screen with its glow around it, the part above the screen's edge cut off.
    private var notch: some View {
        let glow = settings.effectiveGlow(on: screen)
        let island = metrics.islandSize, radius = metrics.islandRadius
        let reach = glow.geometry(islandWidth: island.width, islandHeight: island.height, islandRadius: radius)
        return GlowPreview(appearance: agents.glowAppearance(light: false, on: screen), settings: glow,
                           islandSize: island, islandRadius: radius, look: look)
            .frame(width: max(island.width, reach.width), height: max(island.height, reach.height + reach.topOffset),
                   alignment: .top)
            .clipped()
    }

    /// A Dynamic dock's marks over their backdrop, as Settings previews them.
    private var dock: some View {
        let config = LogoQueueConfig(items: agents.queueItems, placement: laidAlongTop, settings: settings.effectiveSettings)
        let margin = GlowWindowController.logoEdgeMargin(stripThickness: config.stripThickness)
        let glow = settings.effectiveGlow(on: screen)
        let reach = glow.geometry(islandWidth: 2, islandHeight: 2, islandRadius: 0)
        return LogoQueuePreview(settings: settings, store: store, placement: placement, metrics: metrics, screen: screen,
                                look: look)
            .frame(width: max(120, config.size.width + margin * 2),
                   height: max(config.stripThickness, reach.height + reach.topOffset))
    }

    /// The Simple panel as the picture lays it out, along a top edge, with its marks following Bot logos as the
    /// dock's do (`LogoQueueConfig.bots`).
    var simpleMeter: SimpleMeterConfig {
        SimpleMeterConfig(items: agents.meterItems, placement: laidAlongTop, bots: settings.effectiveSettings.botLogos)
    }

    /// The Simple panel, with its own fill and hairline, as Settings previews it.
    private var simple: some View {
        let config = simpleMeter
        return SimpleMeterView(config: config)
            .frame(width: config.stripSize.width, height: config.stripSize.height)
    }
}
