import AppKit
import QuartzCore
import AgentHUDCore

/// Click-through window that hosts the glow bitmap and the expanded panel's drop shadow, and under a light or glass
/// look the shadows of the HUD's shells.
/// The canvas spans the display height and stays fixed during expansion; only the layers inside move.
@MainActor
final class GlowWindowController {
    let panel: OverlayPanel
    private let host = NSView()
    /// Holds the materials the edge fade keeps to a logo queue's column while they are its backdrop.
    private let fadedCanvas = CALayer()
    /// Render every material in the existing top-edge coordinate space, then turn the canvas toward the
    /// chosen edge. This keeps the soft bitmap, grid shaders, alerts and shadow on one orientation.
    private let materialCanvas = CALayer()
    /// The shells' shadows, turned like the materials and beneath them, but outside the edge fade, which would clip
    /// a card's shadow to the queue's column.
    private let shellCanvas = CALayer()
    let glowLayer = CALayer()
    let shadowLayer = CALayer()
    /// Under a light or glass look, the shadow of the island, of the panel or reminder it has grown into, or of a
    /// detached card, in place of `shadowLayer`'s.
    let shellShadowLayer = CALayer()
    /// Under such a look, the shadow of the Simple panel on a dock that separates its agents.
    let dockShadowLayer = CALayer()
    private let alertLayer = CALayer()
    /// Fades the backdrop out at the ends. Clipping it to the queue's width leaves a cut edge; a queue has
    /// no silhouette to justify one, so the field dies away instead.
    private let edgeFade = CAGradientLayer()
    private var lastAlertID: String?
    private var softKey: SoftKey?
    private var glowPadding: CGFloat = 0
    private var shadowKey: ShadowKey?
    private var shadowPadding: CGFloat = 0
    private var shellShadowKey: ShadowKey?
    private var dockShadowKey: ShadowKey?
    /// The shells the shadows were last placed under: a shell that was not there has no place to move its shadow from.
    private var placedShells: HUDShells?
    /// A detached card went back into its dock while the materials were pending, before the shells were given their
    /// shadows again: the card's is dropped once they are.
    private var cardLeft = false
    /// A card whose shadow shows moved meanwhile: the dock's shadow fades to its new cut once they are.
    private var cardSlid = false
    /// The look the installed materials were drawn for.
    private var installedLook: HUDLook?
    /// The canvas they were laid out on: the panel's frame, and the edge the canvas was turned toward.
    private var installedCanvas: (frame: CGRect, edge: HUDEdge)?
    /// Set while an update installs a change of look, whose materials then fade over the old ones as the island
    /// cross-fades to it, rather than switching at once. Reduce Motion leaves it unset.
    private var crossFades = false
    private var breathKey: BreathKey?
    private var restingKey: GlowFrameRenderer.Key?
    /// The soft glow's resting bitmap, restored when a frame-by-frame effect stops.
    private var softStill: CGImage?
    private var frameRenderer: GlowFrameRenderer?
    private lazy var animator = GlowAnimator(host: host, layer: glowLayer)
    var isAnimating: Bool { animator.isRunning }

    /// The window's width beside the notch, where the glow rims the open panel too: the widest panel a layout sets
    /// (`PanelStyle.widthRange`) with the farthest field any setting draws on both sides of it, a grid's deepest or the
    /// soft glow's widest range and feather, and never less than the 1000 points it had. The host clips to it.
    static let panelWidth: CGFloat = {
        let grid = GlowMatrix.reach(pitch: GlowSettings.gridPitchRange.upperBound, core: GlowSettings.gridCoreRange.upperBound,
                                    fade: GlowSettings.gridFadeRange.upperBound)
        let soft = GlowSettings.sizeRange.upperBound + GlowSettings.sizeRange.upperBound * 3
        return max(1000, CGFloat(PanelStyle.widthRange.upperBound) + 2 * CGFloat(max(grid, soft)))
    }()

    /// Inputs of the soft glow bitmap.
    fileprivate struct SoftKey: Hashable, Sendable {
        let glow: GlowGeometry
        let islandSize: CGSize
        let islandRadius: CGFloat
        let outwardOnly: Bool
        let stops: [GradientStop]
        let scale: CGFloat
        /// The island cut out of it where the look is glass, flares included.
        let cutout: GlowCutout?
    }

    /// Inputs of a shadow bitmap.
    fileprivate struct ShadowKey: Equatable, Sendable {
        /// What casts it: the glow's island under the standard look, a shell under a light or glass one.
        let caster: ShellShape
        let scale: CGFloat
        /// The look it is cast under, which decides whose shadow it is and how it is drawn.
        let look: HUDLook
        /// What glass would show it through: the shells over it, placed from the caster. Empty for a solid look.
        let cutouts: [ShellCutout]
    }

    private struct BreathKey: Hashable {
        let pulses: Bool
        let peakOpacity: Double
        let troughOpacity: Double
        let breathSeconds: Double
    }

    /// Materials for one pending opening. The hover owner retains these until the panel opens or the pointer leaves.
    struct PreparedImages: Sendable {
        fileprivate let softKey: SoftKey
        let soft: GlowImage?
        /// The open surface's shadow: the panel's under the standard look, its shell's under a light or glass one, or
        /// none where such a look shows no shell.
        fileprivate let shadowKey: ShadowKey?
        let shadow: GlowImage?
        let renderer: GlowFrameRenderer?
        let resting: GlowImage?
    }

    /// Screen, accessibility and font queries happen before this value is handed to the raster worker.
    struct PreparationInputs: Sendable {
        fileprivate let softKey: SoftKey
        fileprivate let shadowKey: ShadowKey?
        fileprivate let rendersSoft: Bool
        fileprivate let rendersShadow: Bool
        fileprivate let rendererKey: GlowFrameRenderer.Key?
        fileprivate let glyphs: GlyphSet?
        fileprivate let rendersResting: Bool
    }

    init(geometry: NotchGeometry) {
        panel = OverlayPanel(frame: Self.panelFrame(for: geometry), level: .statusBar, acceptsMouse: false)
        panel.setAccessibilityElement(false)
        panel.setAccessibilityHidden(true)
        host.wantsLayer = true
        host.layer?.masksToBounds = true
        panel.contentView = host
        host.layer?.addSublayer(shellCanvas)
        host.layer?.addSublayer(fadedCanvas)
        fadedCanvas.addSublayer(materialCanvas)
        for layer in [shadowLayer, glowLayer, alertLayer] {
            layer.contentsGravity = .resize
            layer.contentsScale = geometry.backingScale
            layer.anchorPoint = .zero
            materialCanvas.addSublayer(layer)
        }
        for layer in [dockShadowLayer, shellShadowLayer] {
            layer.contentsGravity = .resize
            layer.contentsScale = geometry.backingScale
            layer.anchorPoint = .zero
            layer.opacity = 0
            shellCanvas.addSublayer(layer)
        }
        shadowLayer.opacity = 0
        alertLayer.opacity = 0
    }

    /// How far the backdrop reaches past the last mark, and the distance it fades over. Taken by thickness
    /// rather than geometry so the settings preview sizes its own backdrop from the same rule.
    static func logoEdgeMargin(stripThickness: CGFloat) -> CGFloat {
        max(12, stripThickness * 0.6)
    }

    static func logoEdgeMargin(for geometry: NotchGeometry) -> CGFloat {
        logoEdgeMargin(stripThickness: geometry.edge.isHorizontal ? geometry.rect.height : geometry.rect.width)
    }

    /// How far the backdrop's lip is run past the queue on each side, so that every cell's nearest point on
    /// it is straight above and the field falls vertically instead of curling in at the ends. The settings
    /// preview builds its own lip from this rule rather than a number of its own.
    static func backdropOverhang(_ glow: GlowSettings) -> CGFloat {
        glow.range + glow.blur * 3 + NotchGeometry.fallbackWidth
    }

    static func panelFrame(for geometry: NotchGeometry) -> CGRect {
        // A logo queue's panel spans the screen, because the island it holds does not stay the width of the
        // marks: an event widens it by a wing on each side, and the open panel is wider still. What keeps
        // the backdrop to the marks' own column is the fade below, which applies only while it is a backdrop.
        guard !geometry.mode.isDocked else { return geometry.screenFrame }
        return CGRect(
            x: geometry.centerX - panelWidth / 2,
            y: geometry.screenFrame.minY,
            width: panelWidth,
            height: geometry.screenFrame.height
        )
    }

    /// Keeps a backdrop to `column`, dying away over `margin` on each side. Passing no column removes the
    /// mask: an island that has grown — for an event, or because the panel opened — wants its whole rim,
    /// not the slice of it that fits the marks' column.
    private func updateEdgeFade(column: CGRect?, margin: CGFloat, panel: CGRect, edge: HUDEdge) {
        let horizontal = edge.isHorizontal
        let length = horizontal ? panel.width : panel.height
        guard let column, length > 0 else {
            if fadedCanvas.mask != nil { fadedCanvas.mask = nil }
            return
        }
        let bounds = CGRect(origin: .zero, size: panel.size)
        let start = horizontal ? panel.minX : panel.minY
        let lower = horizontal ? column.minX : column.minY
        let upper = horizontal ? column.maxX : column.maxY
        func stop(_ coordinate: CGFloat) -> NSNumber {
            NSNumber(value: Double(min(1, max(0, (coordinate - start) / length))))
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        edgeFade.frame = bounds
        edgeFade.startPoint = horizontal ? CGPoint(x: 0, y: 0.5) : CGPoint(x: 0.5, y: 0)
        edgeFade.endPoint = horizontal ? CGPoint(x: 1, y: 0.5) : CGPoint(x: 0.5, y: 1)
        edgeFade.colors = [
            CGColor(gray: 0, alpha: 0), CGColor(gray: 0, alpha: 1),
            CGColor(gray: 0, alpha: 1), CGColor(gray: 0, alpha: 0),
        ]
        edgeFade.locations = [stop(lower - margin), stop(lower), stop(upper), stop(upper + margin)]
        if fadedCanvas.mask !== edgeFade { fadedCanvas.mask = edgeFade }
        CATransaction.commit()
    }

    /// The island expressed with its parked edge at the top. For vertical queues, increasing x follows
    /// the marks from top to bottom, preserving agent colours in the same order as their logos.
    nonisolated static func canonicalIslandFrame(_ island: CGRect, panel: CGRect, edge: HUDEdge) -> CGRect {
        let local = island.offsetBy(dx: -panel.minX, dy: -panel.minY)
        switch edge {
        case .top: return local
        case .bottom:
            return CGRect(x: local.minX, y: panel.height - local.maxY, width: local.width, height: local.height)
        case .left:
            return CGRect(x: panel.height - local.maxY, y: panel.width - local.maxX,
                          width: local.height, height: local.width)
        case .right:
            return CGRect(x: panel.height - local.maxY, y: local.minX,
                          width: local.height, height: local.width)
        }
    }

    private func orientCanvas(in frame: CGRect, edge: HUDEdge) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fadedCanvas.frame = CGRect(origin: .zero, size: frame.size)
        let transform: CGAffineTransform
        switch edge {
        case .top: transform = .identity
        case .bottom: transform = CGAffineTransform(scaleX: 1, y: -1)
        case .left: transform = CGAffineTransform(a: 0, b: -1, c: -1, d: 0, tx: 0, ty: 0)
        case .right: transform = CGAffineTransform(a: 0, b: -1, c: 1, d: 0, tx: 0, ty: 0)
        }
        for canvas in [materialCanvas, shellCanvas] {
            canvas.bounds = CGRect(origin: .zero, size: edge.isHorizontal
                ? frame.size : CGSize(width: frame.height, height: frame.width))
            canvas.position = CGPoint(x: frame.width / 2, y: frame.height / 2)
            canvas.setAffineTransform(transform)
        }
        CATransaction.commit()
    }

    /// Which way a shell's shadow falls in the top-edge space: down the screen, as a window's does, whichever edge the
    /// HUD is parked on. That is away from a top edge, toward a bottom one, and along a side one.
    nonisolated static func shellShadowDrop(on edge: HUDEdge) -> CGSize {
        let drop = GlowRenderer.shellShadowDrop
        switch edge {
        case .top: return CGSize(width: 0, height: -drop)
        case .bottom: return CGSize(width: 0, height: drop)
        case .left, .right: return CGSize(width: drop, height: 0)
        }
    }

    /// Prepares the open panel while its hover delay runs, leaving the collapsed HUD and its animation untouched.
    func prepare(
        geometry: NotchGeometry,
        island: CGRect,
        islandRadius: CGFloat,
        islandFlare: CGFloat,
        glow: GlowGeometry,
        outwardOnly: Bool,
        appearance: GlowAppearance,
        look: HUDLook,
        shells: HUDShells,
        pattern: GlowPattern = GlowPattern(),
        drawsGlow: Bool = true
    ) -> PreparedImages {
        Self.render(capturePreparation(geometry: geometry, island: island, islandRadius: islandRadius, islandFlare: islandFlare,
            glow: glow, outwardOnly: outwardOnly, appearance: appearance, look: look, shells: shells, pattern: pattern,
            drawsGlow: drawsGlow))
    }

    func capturePreparation(
        geometry: NotchGeometry, island: CGRect, islandRadius: CGFloat, islandFlare: CGFloat, glow: GlowGeometry,
        outwardOnly: Bool, appearance: GlowAppearance, look: HUDLook, shells: HUDShells,
        pattern: GlowPattern = GlowPattern(), drawsGlow: Bool = true
    ) -> PreparationInputs {
        let frame = Self.panelFrame(for: geometry)
        let local = Self.canonicalIslandFrame(island, panel: frame, edge: geometry.edge)
        let scale = geometry.backingScale
        let cutout = GlowCutout(look: look, flare: islandFlare)
        let softKey = SoftKey(glow: glow, islandSize: local.size, islandRadius: islandRadius,
                              outwardOnly: outwardOnly, stops: appearance.stops, scale: scale, cutout: cutout)
        let visible = !appearance.hidden
        // The standard look's shadow lies under the glow's island and shows only with the glow; a light or glass
        // look's falls from the shell, glow or none, and there is none while the HUD shows no shell.
        let shadowKey: ShadowKey?
        let rendersShadow: Bool
        if look.castsShellShadows {
            shadowKey = shells.surface.flatMap { $0.bare ? nil : shellShadowKey(for: $0, among: shells, look: look,
                                                                                    geometry: geometry, frame: frame) }
            rendersShadow = shadowKey != nil
        } else {
            shadowKey = ShadowKey(caster: ShellShape(size: local.size, radius: islandRadius, flare: 0, floats: false),
                                  scale: scale, look: look, cutouts: [])
            rendersShadow = visible
        }
        let rendererKey: GlowFrameRenderer.Key?
        if visible && drawsGlow && (pattern.usesGrid || pattern.effect != .breathe) {
            rendererKey = .init(glow: glow, islandRadius: islandRadius, stops: appearance.stops,
                scale: scale, pattern: pattern, colorSpace: panel.screen?.colorSpace?.cgColorSpace,
                islandSize: pattern.usesGrid ? .zero : local.size, outwardOnly: pattern.usesGrid ? true : outwardOnly,
                cutout: cutout)
        } else {
            rendererKey = nil
        }
        let motion = Self.playsMotion(pattern: pattern, appearance: appearance,
                                      reduceMotion: NSWorkspace.shared.accessibilityDisplayShouldReduceMotion)
        return PreparationInputs(softKey: softKey, shadowKey: shadowKey,
            rendersSoft: visible && drawsGlow && !pattern.usesGrid, rendersShadow: rendersShadow,
            rendererKey: rendererKey, glyphs: rendererKey.flatMap(GlowFrameRenderer.glyphs),
            rendersResting: pattern.usesGrid && !motion)
    }

    /// Only immutable Core Graphics/Core Text values reach this function; no window or AppKit queries occur here.
    nonisolated static func render(_ inputs: PreparationInputs) -> PreparedImages {
        let key = inputs.softKey, shadowKey = inputs.shadowKey
        let soft = inputs.rendersSoft
            ? GlowRenderer.render(glow: key.glow, islandSize: key.islandSize, islandRadius: key.islandRadius,
                                  outwardOnly: key.outwardOnly, cutout: key.cutout, stops: key.stops, scale: key.scale)
            : nil
        let shadow = inputs.rendersShadow ? shadowKey.flatMap(renderShadow) : nil
        let renderer = Task.isCancelled ? nil : inputs.rendererKey.map { GlowFrameRenderer($0, glyphs: inputs.glyphs) }
        let resting = inputs.rendersResting
            ? renderer?.render(time: 0, blend: 0, breathSeconds: 0, breathAmplitude: 0) : nil
        return PreparedImages(softKey: key, soft: soft, shadowKey: shadowKey, shadow: shadow,
                              renderer: renderer, resting: resting)
    }

    /// Draws the shadow `key` describes: the standard look's panel shadow, or a light or glass shell's.
    nonisolated private static func renderShadow(_ key: ShadowKey) -> GlowImage? {
        guard key.look.castsShellShadows else {
            return GlowRenderer.renderShadow(width: key.caster.size.width, height: key.caster.size.height,
                                             cornerRadius: key.caster.radius, scale: key.scale)
        }
        return GlowRenderer.renderShellShadow(key.caster, cutouts: key.cutouts, scale: key.scale)
    }

    /// A clock or data refresh can change panel inputs without changing any of the installed materials.
    /// Reuse the controller's existing keys so those refreshes neither draw again nor briefly hide the glow.
    func installedPreparation(matching inputs: PreparationInputs) -> PreparedImages? {
        guard !inputs.rendersSoft || softKey == inputs.softKey,
              !inputs.rendersShadow || installedShadowKey(like: inputs.shadowKey) == inputs.shadowKey,
              inputs.rendererKey == nil || frameRenderer?.key == inputs.rendererKey,
              !inputs.rendersResting || restingKey == inputs.rendererKey else { return nil }
        return PreparedImages(softKey: inputs.softKey, soft: nil, shadowKey: inputs.shadowKey, shadow: nil,
                              renderer: inputs.rendererKey == nil ? nil : frameRenderer, resting: nil)
    }

    /// The key of the shadow installed where `key`'s would go: the panel shadow's under the standard look, the
    /// surface shell's under a light or glass one.
    private func installedShadowKey(like key: ShadowKey?) -> ShadowKey? {
        key?.look.castsShellShadows == true ? shellShadowKey : shadowKey
    }

    /// - island: the island's current frame in screen coordinates.
    func update(
        geometry: NotchGeometry,
        island: CGRect,
        islandRadius: CGFloat,
        /// How far the island's flares reach past its sides, which the glow's cutout under a glass island takes in.
        islandFlare: CGFloat,
        glow: GlowGeometry,
        outwardOnly: Bool,
        appearance: GlowAppearance,
        /// The HUD's look. A glass island has itself cut out of the glow, and a light or glass HUD's shells cast
        /// shadows of their own; the glow's own colours follow the desktop behind it, never the look.
        look: HUDLook,
        /// The pieces of the HUD a light or glass look gives shadows, where they are now.
        shells: HUDShells,
        animated: Bool,
        /// Whether the shells' shadows follow them in motion: whenever the island animates, and when a card moves
        /// along a dock that separates its agents, which animates while the island does not.
        animatesShells: Bool,
        /// Whether a detached card has gone back into its dock since the last update: its shadow then goes at once
        /// rather than fade (`updateShells`).
        cardLeaves: Bool = false,
        /// Whether a detached card whose shadow shows is moving to another place: the dock's shadow, cut by the card,
        /// then fades to its new cut as the card goes there, where it would otherwise switch ahead of the card.
        fadesDock: Bool = false,
        alert: IslandAlert? = nil,
        quotaVendors: [String] = [],
        pattern: GlowPattern = GlowPattern(),
        /// The marks' own column, when the glow is a logo queue's backdrop rather than a rim around a shape.
        backdrop: CGRect? = nil,
        /// A logo queue's glow is a backdrop behind its marks, never a rim: once the panel has grown there
        /// is no backdrop left to draw, so the field stops rather than following the new shape around. The
        /// panel keeps its shadow, which lives here too.
        drawsGlow: Bool = true,
        prepared: PreparedImages? = nil,
        materialsPending: Bool = false
    ) {
        let frame = Self.panelFrame(for: geometry)
        if panel.frame != frame { panel.setFrame(frame, display: false) }
        // The host has not been resized to the new panel frame yet; the fade is measured against it directly.
        updateEdgeFade(column: backdrop, margin: Self.logoEdgeMargin(for: geometry), panel: frame, edge: geometry.edge)
        orientCanvas(in: frame, edge: geometry.edge)
        glowLayer.contentsScale = geometry.backingScale
        shadowLayer.contentsScale = geometry.backingScale
        shellShadowLayer.contentsScale = geometry.backingScale
        dockShadowLayer.contentsScale = geometry.backingScale
        let restyled = installedLook.map { $0 != look } ?? false
        let unmoved = installedCanvas.map { $0.frame == frame && $0.edge == geometry.edge } ?? false
        crossFades = restyled && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        defer { crossFades = false }

        if appearance.hidden {
            animator.stop()
            softKey = nil
            restingKey = nil
            // Without its glow the standard HUD has nothing here; a light or glass one keeps its shells' shadows.
            guard look.castsShellShadows else {
                installedLook = look
                installedCanvas = (frame, geometry.edge)
                if panel.isVisible { panel.orderOut(nil) }
                return
            }
        }
        if !panel.isVisible { panel.orderFrontRegardless() }

        // Opening and collapse commit their window geometry immediately; the worker supplies correctly shaped materials.
        // A previous surface's contour must not be stretched over a different panel while that result is pending.
        // A change of look keeps the old look's materials up instead, until the new ones fade in over them, as long as
        // the canvas they were laid out on has not moved or turned under them, as it does when a drop changes the mode.
        let stale = materialsPending && !(restyled && unmoved)
        glowLayer.isHidden = stale || appearance.hidden
        shadowLayer.isHidden = stale
        alertLayer.isHidden = stale || appearance.hidden
        shellShadowLayer.isHidden = stale
        if materialsPending {
            // The shells are given their shadows once the materials have come, the card's dropped then.
            cardLeft = cardLeft || cardLeaves
            cardSlid = cardSlid || fadesDock
            return
        }
        installedLook = look
        installedCanvas = (frame, geometry.edge)

        // Bitmaps and shader geometry retain their top-edge convention, including on a vertical dock.
        let local = Self.canonicalIslandFrame(island, panel: frame, edge: geometry.edge)
        let glowTop = local.maxY - glow.topOffset
        let glowRect = CGRect(x: local.minX - glow.sideInset, y: glowTop - glow.height, width: glow.width, height: glow.height)
        let cutout = GlowCutout(look: look, flare: islandFlare)

        if drawsGlow && !appearance.hidden {
            let motion = Self.playsMotion(pattern: pattern, appearance: appearance,
                                          reduceMotion: NSWorkspace.shared.accessibilityDisplayShouldReduceMotion)
            if pattern.usesGrid {
                updateGridImage(glow: glow, islandRadius: islandRadius, cutout: cutout, stops: appearance.stops,
                                scale: geometry.backingScale, pattern: pattern, appearance: appearance, motion: motion,
                                prepared: prepared)
            } else {
                restingKey = nil
                updateSoftImage(glow: glow, islandSize: local.size, islandRadius: islandRadius, outwardOnly: outwardOnly,
                                cutout: cutout, stops: appearance.stops, scale: geometry.backingScale, pattern: pattern,
                                appearance: appearance, motion: motion, prepared: prepared)
            }
        } else {
            animator.stop()
            softKey = nil
            restingKey = nil
            glowLayer.contents = nil
        }
        // A light or glass look's shells cast shadows of their own, in place of the panel's.
        if !look.castsShellShadows {
            updateShadowImage(island: local, radius: islandRadius, look: look, scale: geometry.backingScale, prepared: prepared)
        }
        let shadowOpacity: Float = look.castsShellShadows ? 0 : 1
        let previousShadowOpacity = (shadowLayer.presentation() ?? shadowLayer).opacity

        CATransaction.begin()
        if animated {
            CATransaction.setAnimationDuration(IslandAnimation.duration)
            CATransaction.setAnimationTimingFunction(IslandAnimation.mediaCurve)
        } else {
            CATransaction.setDisableActions(true)
        }
        glowLayer.frame = glowRect.insetBy(dx: -glowPadding, dy: -glowPadding)
        alertLayer.frame = glowLayer.frame
        shadowLayer.frame = local.offsetBy(dx: 0, dy: -8).insetBy(dx: -shadowPadding, dy: -shadowPadding)
        shadowLayer.opacity = shadowOpacity
        CATransaction.commit()
        crossFadeOpacity(shadowLayer, from: previousShadowOpacity)
        updateShells(look.castsShellShadows ? shells : HUDShells(surface: nil, dock: nil), look: look,
                     geometry: geometry, frame: frame, prepared: prepared, animates: animatesShells,
                     cardLeaves: cardLeaves || cardLeft, fadesDock: fadesDock || cardSlid)
        cardLeft = false
        cardSlid = false

        // A running grid effect carries the breathing itself; otherwise the whole layer pulses.
        applyBreathing(appearance, pulsesOpacity: !animator.isRunning)
        // The alert's colour is carried by the glow, so it goes wherever the glow does.
        guard drawsGlow, !appearance.hidden else {
            lastAlertID = nil
            alertLayer.opacity = 0
            return
        }
        applyAlert(alert, vendors: quotaVendors, glow: glow, islandSize: local.size, radius: islandRadius,
                   outwardOnly: outwardOnly, cutout: cutout, scale: geometry.backingScale, pattern: pattern)
    }

    // MARK: Shells

    /// Gives the shells their shadows, leaving everything else here as the last update left it: a detached card's, which
    /// `ScreenHUD` holds back until the card has grown out of its dock (`CardShadowHold`). It fades in when `animates`,
    /// and the dock's, cut by the card from now on under a see-through look, fades to its new cut with it.
    func updateShells(_ shells: HUDShells, look: HUDLook, geometry: NotchGeometry, animates: Bool) {
        guard look.castsShellShadows else { return }
        updateShells(shells, look: look, geometry: geometry, frame: Self.panelFrame(for: geometry), prepared: nil,
                     animates: animates, fadesDock: animates)
    }

    /// Gives the shells their shadows, or takes them away. A shadow that appears fades in where its shell is, one that
    /// goes fades out where it was, and one already showing follows its shell when `animates`. A detached card's goes
    /// at once as the card goes back into its dock (`cardLeaves`): the card's shadow is cut out where the card was, and
    /// fading there it would show that place round the card as it shrinks. A shell that moves between the surface and
    /// the dock, as a Simple panel that does not separate its agents does when a card comes out of it and goes back,
    /// keeps its shadow where it stands: it passes from one layer to the other at once, so the panel never has two
    /// shadows nor one that slides in from the card. A change of look still cross-fades it. The dock's new shadow fades
    /// in over its old one when `fadesDock`.
    private func updateShells(_ shells: HUDShells, look: HUDLook, geometry: NotchGeometry, frame: CGRect,
                              prepared: PreparedImages?, animates: Bool, cardLeaves: Bool = false,
                              fadesDock: Bool = false) {
        if let surface = shells.surface, !surface.bare {
            let key = shellShadowKey(for: surface, among: shells, look: look, geometry: geometry, frame: frame)
            shellShadowKey = installShellShadow(key, in: shellShadowLayer, holding: shellShadowKey, prepared: prepared)
        }
        if let dock = shells.dock {
            let key = shellShadowKey(for: dock, among: shells, look: look, geometry: geometry, frame: frame)
            dockShadowKey = installShellShadow(key, in: dockShadowLayer, holding: dockShadowKey, prepared: nil,
                                               fades: fadesDock)
        }
        let passes = shells.dock != nil && shells.dock == placedShells?.surface
            || shells.surface != nil && shells.surface == placedShells?.dock
        place(shellShadowLayer, under: shells.surface, was: placedShells?.surface, geometry: geometry, frame: frame,
              animates: animates && !cardLeaves && !passes)
        place(dockShadowLayer, under: shells.dock, was: placedShells?.dock, geometry: geometry, frame: frame,
              animates: animates && !passes)
        placedShells = shells
    }

    /// What a shell's shadow is drawn from: the shell's shape where the canvas has it, and under glass every shell
    /// the shadow lies behind, itself included, placed from it, so that the glass shows none of it.
    private func shellShadowKey(for shell: HUDShell, among shells: HUDShells, look: HUDLook, geometry: NotchGeometry,
                                frame: CGRect) -> ShadowKey {
        let local = Self.canonicalIslandFrame(shell.frame, panel: frame, edge: geometry.edge)
        let drop = Self.shellShadowDrop(on: geometry.edge)
        let cutouts: [ShellCutout] = look.seeThrough
            ? [shells.surface, shells.dock].compactMap { $0 }.filter { !$0.bare }.map { other in
                let place = Self.canonicalIslandFrame(other.frame, panel: frame, edge: geometry.edge)
                return ShellCutout(shape: Self.shape(of: other, at: place),
                                   offset: CGSize(width: place.minX - local.minX - drop.width,
                                                  height: place.minY - local.minY - drop.height))
            }
            : []
        return ShadowKey(caster: Self.shape(of: shell, at: local), scale: geometry.backingScale, look: look, cutouts: cutouts)
    }

    private static func shape(of shell: HUDShell, at local: CGRect) -> ShellShape {
        ShellShape(size: local.size, radius: shell.radius, flare: shell.flare, floats: shell.floats)
    }

    /// Draws `key`'s shadow into `layer` unless it holds it already, from a prepared bitmap where one matches, and
    /// returns the key the layer holds now. The new shadow fades in over the old one on a change of look, or when `fades`.
    private func installShellShadow(_ key: ShadowKey, in layer: CALayer, holding installed: ShadowKey?,
                                    prepared: PreparedImages?, fades: Bool = false) -> ShadowKey? {
        guard key != installed else { return installed }
        guard let rendered = (prepared?.shadowKey == key ? prepared?.shadow : nil) ?? Self.renderShadow(key) else {
            return installed
        }
        // The corners of the shell and of everything cut out of it stay at their size while the shadow stretches.
        let drop = key.cutouts.isEmpty ? 0 : GlowRenderer.shellShadowDrop
        let corner = rendered.padding + max(key.caster.radius, key.caster.flare) + drop
        let horizontal = min(corner, (rendered.size.width - 1) / 2), vertical = min(corner, (rendered.size.height - 1) / 2)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer.contents = rendered.image
        layer.contentsCenter = CGRect(x: horizontal / rendered.size.width, y: vertical / rendered.size.height,
                                      width: (rendered.size.width - horizontal * 2) / rendered.size.width,
                                      height: (rendered.size.height - vertical * 2) / rendered.size.height)
        crossFade(layer, fades: fades)
        CATransaction.commit()
        return key
    }

    /// Puts `layer` under `shell`, or fades it out where it is when there is none or the shell is bare.
    private func place(_ layer: CALayer, under shell: HUDShell?, was previous: HUDShell?, geometry: NotchGeometry,
                       frame: CGRect, animates: Bool) {
        let opacity: Float = shell.map { $0.bare ? 0 : 1 } ?? 0
        let previousOpacity = (layer.presentation() ?? layer).opacity
        if let shell {
            let padding = GlowRenderer.shellShadowPadding
            let drop = Self.shellShadowDrop(on: geometry.edge)
            CATransaction.begin()
            // A shell that was not there has nowhere to move from: its shadow fades in where it is.
            if animates && previous != nil {
                CATransaction.setAnimationDuration(IslandAnimation.duration)
                CATransaction.setAnimationTimingFunction(IslandAnimation.mediaCurve)
            } else {
                CATransaction.setDisableActions(true)
            }
            layer.frame = Self.canonicalIslandFrame(shell.frame, panel: frame, edge: geometry.edge)
                .offsetBy(dx: drop.width, dy: drop.height).insetBy(dx: -padding, dy: -padding)
            CATransaction.commit()
        }
        CATransaction.begin()
        if animates {
            CATransaction.setAnimationDuration(IslandAnimation.duration)
            CATransaction.setAnimationTimingFunction(IslandAnimation.mediaCurve)
        } else {
            CATransaction.setDisableActions(true)
        }
        layer.opacity = opacity
        CATransaction.commit()
        if !animates { crossFadeOpacity(layer, from: previousOpacity) }
    }

    /// Fades a layer's new contents in over its old ones while a change of look is being installed, or when `fades`.
    /// Called inside the transaction that sets them.
    private func crossFade(_ layer: CALayer, fades: Bool = false) {
        guard crossFades || fades else { return }
        let fade = CATransition()
        fade.type = .fade
        fade.duration = IslandAnimation.duration
        fade.timingFunction = IslandAnimation.mediaCurve
        layer.add(fade, forKey: "look")
    }

    /// Fades a layer from `previous` to its opacity now while a change of look is being installed, where the change
    /// would otherwise switch at once.
    private func crossFadeOpacity(_ layer: CALayer, from previous: Float) {
        guard crossFades, previous != layer.opacity else { return }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = previous
        fade.toValue = layer.opacity
        fade.duration = IslandAnimation.duration
        fade.timingFunction = IslandAnimation.mediaCurve
        layer.add(fade, forKey: "look-opacity")
    }

    /// Effects play frame by frame while an agent is running, collapsed or expanded alike; Reduce Motion keeps the
    /// resting frame. The soft glow's breathing stays a Core Animation opacity pulse.
    nonisolated static func playsMotion(pattern: GlowPattern, appearance: GlowAppearance, reduceMotion: Bool) -> Bool {
        (pattern.usesGrid || pattern.effect != .breathe) && appearance.breathing && !appearance.hidden && !reduceMotion
    }

    /// How deep the appearance breathes, as the fraction of brightness it loses at the trough.
    private static func breathDepth(_ appearance: GlowAppearance) -> Double {
        appearance.peakOpacity > 0 ? 1 - appearance.troughOpacity / appearance.peakOpacity : 0
    }

    /// The blurred bitmap stretches through its nine-slice centre while the island animates; a grid of dots
    /// would distort, so those bitmaps stay at native size, pinned to the screen edge, and the animating frame
    /// reveals or hides rows instead.
    private func configure(_ layer: CALayer, for style: GlowStyle, image: GlowImage, side: CGFloat, bottom: CGFloat) {
        if style == .blur {
            layer.contentsGravity = .resize
            layer.masksToBounds = false
            layer.contentsCenter = contentsCenter(for: image, side: side, bottom: bottom)
        } else {
            layer.contentsGravity = .top
            layer.masksToBounds = true
            layer.contentsCenter = CGRect(x: 0, y: 0, width: 1, height: 1)
        }
    }

    private func applyAlert(_ alert: IslandAlert?, vendors: [String], glow: GlowGeometry, islandSize: CGSize,
                            radius: CGFloat, outwardOnly: Bool, cutout: GlowCutout?, scale: CGFloat, pattern: GlowPattern) {
        guard lastAlertID != alert?.id else { return }
        lastAlertID = alert?.id
        alertLayer.removeAllAnimations()
        guard let alert else { return }
        let color = alert.accent
        let clear = color.withAlpha(0)
        let matches = vendors.indices.filter { vendors[$0] == alert.vendor }
        let stops: [GradientStop]
        if let first = matches.first, let last = matches.last {
            let count = Double(vendors.count)
            let left = Double(first) / count, right = Double(last + 1) / count
            let feather = 0.15 / count
            stops = [GradientStop(color: clear, location: max(0, left - feather)),
                     GradientStop(color: color, location: left + feather),
                     GradientStop(color: color, location: right - feather),
                     GradientStop(color: clear, location: min(1, right + feather))]
        } else {
            stops = [GradientStop(color: color, location: 0), GradientStop(color: color, location: 1)]
        }
        guard let rendered = GlowFrameRenderer.resting(.init(glow: glow, islandRadius: radius, stops: stops, scale: scale,
                                                             pattern: pattern, islandSize: islandSize,
                                                             outwardOnly: outwardOnly, cutout: cutout)) else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        alertLayer.contents = rendered.image
        alertLayer.contentsScale = scale
        configure(alertLayer, for: pattern.style, image: rendered, side: glow.sideInset + radius, bottom: glow.sideInset + radius)
        CATransaction.commit()
        let pulse = CAKeyframeAnimation(keyPath: "opacity")
        if NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            pulse.values = [0, 0.12, 0]
            pulse.keyTimes = [0, 0.3, 1]
        } else if alert.isWarning {
            pulse.values = [0, 0.22, 0.05, 0.22, 0]
            pulse.keyTimes = [0, 0.2, 0.45, 0.65, 1]
        } else {
            pulse.values = [0, 0.22, 0.15, 0]
            pulse.keyTimes = [0, 0.25, 0.65, 1]
        }
        pulse.duration = 1.8
        pulse.calculationMode = .linear
        alertLayer.add(pulse, forKey: "quota-event")
    }

    private func renderer(for key: GlowFrameRenderer.Key, prepared: PreparedImages?) -> GlowFrameRenderer {
        if let frameRenderer, frameRenderer.key == key { return frameRenderer }
        let renderer = prepared?.renderer.flatMap { $0.key == key ? $0 : nil } ?? GlowFrameRenderer(key)
        frameRenderer = renderer
        return renderer
    }

    private func updateGridImage(glow: GlowGeometry, islandRadius: CGFloat, cutout: GlowCutout?, stops: [GradientStop],
                                 scale: CGFloat, pattern: GlowPattern, appearance: GlowAppearance, motion: Bool,
                                 prepared: PreparedImages?) {
        let renderer = renderer(for: .init(glow: glow, islandRadius: islandRadius, stops: stops, scale: scale, pattern: pattern,
                                           colorSpace: panel.screen?.colorSpace?.cgColorSpace, cutout: cutout),
                                prepared: prepared)
        let resting = prepared?.renderer?.key == renderer.key ? prepared?.resting : nil
        softKey = nil
        glowPadding = 0
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        glowLayer.contentsGravity = .top
        glowLayer.masksToBounds = true
        glowLayer.contentsCenter = CGRect(x: 0, y: 0, width: 1, height: 1)
        CATransaction.commit()
        if motion {
            restingKey = nil
            animator.play(renderer, breathSeconds: appearance.breathSeconds, breathAmplitude: Self.breathDepth(appearance))
        } else if animator.isRunning && !appearance.hidden {
            // The last agent went idle: ease back into the resting frame.
            animator.settle(renderer) { [weak self] in self?.showResting(renderer, image: resting) }
        } else {
            animator.stop()
            showResting(renderer, image: resting)
        }
    }

    /// The soft glow keeps its nine-slice bitmap. Effects other than breathing swap in frames of the same size and
    /// layout, and hand the resting bitmap back when they stop.
    private func updateSoftImage(glow: GlowGeometry, islandSize: CGSize, islandRadius: CGFloat, outwardOnly: Bool,
                                 cutout: GlowCutout?, stops: [GradientStop], scale: CGFloat, pattern: GlowPattern,
                                 appearance: GlowAppearance, motion: Bool, prepared: PreparedImages?) {
        updateGlowImage(glow: glow, islandSize: islandSize, islandRadius: islandRadius, outwardOnly: outwardOnly,
                        cutout: cutout, stops: stops, scale: scale, prepared: prepared)
        guard pattern.effect != .breathe else { return stopSoftMotion() }
        let renderer = renderer(for: .init(glow: glow, islandRadius: islandRadius, stops: stops, scale: scale, pattern: pattern,
                                           colorSpace: panel.screen?.colorSpace?.cgColorSpace,
                                           islandSize: islandSize, outwardOnly: outwardOnly, cutout: cutout), prepared: prepared)
        if motion {
            animator.play(renderer, breathSeconds: appearance.breathSeconds, breathAmplitude: Self.breathDepth(appearance))
        } else if animator.isRunning && !appearance.hidden {
            animator.settle(renderer) { [weak self] in self?.restoreSoftStill() }
        } else {
            stopSoftMotion()
        }
    }

    /// Takes the glow off screen for good, with its display link: the display went away with the HUD.
    func close() {
        animator.stop()
        panel.orderOut(nil)
    }

    private func stopSoftMotion() {
        guard animator.isRunning else { return }
        animator.stop()
        restoreSoftStill()
    }

    private func restoreSoftStill() {
        guard let softStill else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        glowLayer.contents = softStill
        CATransaction.commit()
    }

    private func showResting(_ renderer: GlowFrameRenderer, image: GlowImage? = nil) {
        guard restingKey != renderer.key,
              let rendered = image ?? renderer.render(time: 0, blend: 0, breathSeconds: 0, breathAmplitude: 0) else { return }
        restingKey = renderer.key
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        glowLayer.contents = rendered.image
        crossFade(glowLayer)
        CATransaction.commit()
    }

    private func updateGlowImage(glow: GlowGeometry, islandSize: CGSize, islandRadius: CGFloat, outwardOnly: Bool,
                                 cutout: GlowCutout?, stops: [GradientStop], scale: CGFloat, prepared: PreparedImages?) {
        let key = SoftKey(glow: glow, islandSize: islandSize, islandRadius: islandRadius, outwardOnly: outwardOnly, stops: stops,
                          scale: scale, cutout: cutout)
        guard key != softKey else { return }
        guard let rendered = (prepared?.softKey == key ? prepared?.soft : nil) ?? GlowRenderer.render(
            glow: glow, islandSize: islandSize, islandRadius: islandRadius, outwardOnly: outwardOnly, cutout: cutout,
            stops: stops, scale: scale
        ) else { return }
        softKey = key
        glowPadding = rendered.padding
        softStill = rendered.image
        CATransaction.begin()
        // A contents crossfade has its own timing and stretches the old contour into the new one.
        // Only animate the frame; keep the feather and rounded edge at their native point size. A change of look
        // keeps the contour, so that alone fades across.
        CATransaction.setDisableActions(true)
        glowLayer.contents = rendered.image
        configure(glowLayer, for: .blur, image: rendered, side: glow.sideInset + islandRadius, bottom: glow.sideInset + islandRadius)
        crossFade(glowLayer)
        CATransaction.commit()
    }

    /// The standard look's shadow, under the glow's island.
    private func updateShadowImage(island: CGRect, radius: CGFloat, look: HUDLook, scale: CGFloat, prepared: PreparedImages?) {
        let key = ShadowKey(caster: ShellShape(size: island.size, radius: radius, flare: 0, floats: false), scale: scale,
                            look: look, cutouts: [])
        guard key != shadowKey else { return }
        guard let rendered = (prepared?.shadowKey == key ? prepared?.shadow : nil)
            ?? GlowRenderer.renderShadow(width: island.width, height: island.height, cornerRadius: radius, scale: scale) else { return }
        shadowKey = key
        shadowPadding = rendered.padding
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        shadowLayer.contents = rendered.image
        shadowLayer.contentsCenter = contentsCenter(for: rendered, side: radius, bottom: radius)
        CATransaction.commit()
    }

    private func contentsCenter(for image: GlowImage, side: CGFloat, bottom: CGFloat) -> CGRect {
        let left = min(image.padding + side, (image.size.width - 1) / 2)
        let top = image.padding
        let lower = min(image.padding + bottom, image.size.height - top - 1)
        return CGRect(x: left / image.size.width, y: top / image.size.height,
                      width: (image.size.width - left * 2) / image.size.width,
                      height: (image.size.height - top - lower) / image.size.height)
    }

    private func applyBreathing(_ appearance: GlowAppearance, pulsesOpacity: Bool) {
        let pulses = appearance.breathing && pulsesOpacity
        let key = BreathKey(pulses: pulses, peakOpacity: appearance.peakOpacity, troughOpacity: appearance.troughOpacity,
                            breathSeconds: appearance.breathSeconds)
        guard key != breathKey else { return }
        breathKey = key
        glowLayer.removeAnimation(forKey: "breathe")
        CATransaction.begin()
        CATransaction.setAnimationDuration(0.3)
        glowLayer.opacity = Float(appearance.peakOpacity)
        CATransaction.commit()
        guard pulses else { return }
        let animation = CABasicAnimation(keyPath: "opacity")
        animation.fromValue = appearance.peakOpacity
        animation.toValue = appearance.troughOpacity
        animation.duration = appearance.breathSeconds / 2
        animation.autoreverses = true
        animation.repeatCount = .infinity
        animation.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        glowLayer.add(animation, forKey: "breathe")
    }
}
