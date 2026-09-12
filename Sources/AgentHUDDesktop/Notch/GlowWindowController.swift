import AppKit
import QuartzCore
import AgentHUDCore

/// Click-through window that hosts the glow bitmap and the expanded panel's drop shadow.
/// The canvas spans the display height and stays fixed during expansion; only the layers inside move.
@MainActor
final class GlowWindowController {
    let panel: NotchPanel
    private let host = NSView()
    private let glowLayer = CALayer()
    private let shadowLayer = CALayer()
    private let alertLayer = CALayer()
    private var lastAlertID: String?
    private var glowKey = ""
    private var glowPadding: CGFloat = 0
    private var shadowKey = ""
    private var shadowPadding: CGFloat = 0
    private var breathKey = ""

    static let panelWidth: CGFloat = 1000

    init(geometry: NotchGeometry) {
        panel = NotchPanel(frame: Self.panelFrame(for: geometry), level: .statusBar, acceptsMouse: false)
        panel.setAccessibilityElement(false)
        panel.setAccessibilityHidden(true)
        host.wantsLayer = true
        host.layer?.masksToBounds = true
        panel.contentView = host
        for layer in [shadowLayer, glowLayer, alertLayer] {
            layer.contentsGravity = .resize
            layer.contentsScale = geometry.backingScale
            layer.anchorPoint = .zero
            host.layer?.addSublayer(layer)
        }
        shadowLayer.opacity = 0
        alertLayer.opacity = 0
    }

    static func panelFrame(for geometry: NotchGeometry) -> CGRect {
        CGRect(
            x: geometry.centerX - panelWidth / 2,
            y: geometry.screenFrame.minY,
            width: panelWidth,
            height: geometry.screenFrame.height
        )
    }

    /// - island: the island's current frame in screen coordinates.
    func update(
        geometry: NotchGeometry,
        island: CGRect,
        islandRadius: CGFloat,
        glow: GlowGeometry,
        outwardOnly: Bool,
        appearance: GlowAppearance,
        animated: Bool,
        alert: IslandAlert? = nil,
        quotaVendors: [String] = []
    ) {
        let frame = Self.panelFrame(for: geometry)
        if panel.frame != frame { panel.setFrame(frame, display: false) }
        glowLayer.contentsScale = geometry.backingScale
        shadowLayer.contentsScale = geometry.backingScale

        if appearance.hidden && alert?.isPreview != true {
            if panel.isVisible { panel.orderOut(nil) }
            return
        }
        if !panel.isVisible { panel.orderFrontRegardless() }

        // Island rect in the host's (bottom-left origin) coordinates.
        let local = CGRect(x: island.minX - frame.minX, y: island.minY - frame.minY, width: island.width, height: island.height)
        let glowTop = local.maxY - glow.topOffset
        let glowRect = CGRect(x: local.minX - glow.sideInset, y: glowTop - glow.height, width: glow.width, height: glow.height)

        updateGlowImage(glow: glow, islandSize: island.size, islandRadius: islandRadius, outwardOnly: outwardOnly, stops: appearance.stops, scale: geometry.backingScale)
        updateShadowImage(island: local, radius: islandRadius, scale: geometry.backingScale)

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
        shadowLayer.opacity = 1
        CATransaction.commit()

        applyBreathing(appearance)
        applyAlert(alert, vendors: quotaVendors, glow: glow, islandSize: island.size, radius: islandRadius,
                   outwardOnly: outwardOnly, scale: geometry.backingScale)
    }

    private func applyAlert(_ alert: IslandAlert?, vendors: [String], glow: GlowGeometry, islandSize: CGSize,
                            radius: CGFloat, outwardOnly: Bool, scale: CGFloat) {
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
        guard let rendered = GlowRenderer.render(glow: glow, islandSize: islandSize, islandRadius: radius,
                                                outwardOnly: outwardOnly, stops: stops, scale: scale) else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        alertLayer.contents = rendered.image
        alertLayer.contentsScale = scale
        alertLayer.contentsCenter = contentsCenter(for: rendered, side: glow.sideInset + radius, bottom: glow.sideInset + radius)
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

    private func updateGlowImage(glow: GlowGeometry, islandSize: CGSize, islandRadius: CGFloat, outwardOnly: Bool, stops: [GradientStop], scale: CGFloat) {
        let key = "\(glow)|\(islandSize)|r\(islandRadius)s\(scale)|outward:\(outwardOnly)|\(GlowGradient.css(stops))"
        guard key != glowKey else { return }
        guard let rendered = GlowRenderer.render(
            glow: glow, islandSize: islandSize, islandRadius: islandRadius, outwardOnly: outwardOnly, stops: stops, scale: scale
        ) else { return }
        glowKey = key
        glowPadding = rendered.padding
        CATransaction.begin()
        // A contents crossfade has its own timing and stretches the old contour into the new one.
        // Only animate the frame; keep the feather and rounded edge at their native point size.
        CATransaction.setDisableActions(true)
        glowLayer.contents = rendered.image
        glowLayer.contentsCenter = contentsCenter(for: rendered, side: glow.sideInset + islandRadius, bottom: glow.sideInset + islandRadius)
        CATransaction.commit()
    }

    private func updateShadowImage(island: CGRect, radius: CGFloat, scale: CGFloat) {
        let key = "\(island.width)x\(island.height)r\(radius)s\(scale)"
        guard key != shadowKey else { return }
        guard let rendered = GlowRenderer.renderShadow(width: island.width, height: island.height, cornerRadius: radius, scale: scale) else { return }
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

    private func applyBreathing(_ appearance: GlowAppearance) {
        let key = "\(appearance.breathing)|\(appearance.peakOpacity)|\(appearance.troughOpacity)|\(appearance.breathSeconds)"
        guard key != breathKey else { return }
        breathKey = key
        glowLayer.removeAnimation(forKey: "breathe")
        CATransaction.begin()
        CATransaction.setAnimationDuration(0.3)
        glowLayer.opacity = Float(appearance.peakOpacity)
        CATransaction.commit()
        guard appearance.breathing else { return }
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
