import AppKit
import AgentHUDCore

/// 22×11 notch silhouette: 1.5 px outline in the label color, filled with the same gradient as the glow.
enum StatusIconRenderer {
    static let size = NSSize(width: 22, height: 11)

    static func image(stops: [GradientStop]) -> NSImage {
        let image = NSImage(size: size, flipped: false) { rect in
            let inset = rect.insetBy(dx: 0.75, dy: 0.75)
            let path = NSBezierPath(roundedRect: inset, xRadius: 4, yRadius: 4)
            NSGraphicsContext.saveGraphicsState()
            path.addClip()
            var resolved = stops
            if resolved.count == 1 {
                resolved = [GradientStop(color: stops[0].color, location: 0), GradientStop(color: stops[0].color, location: 1)]
            }
            if !resolved.isEmpty,
               let gradient = NSGradient(colors: resolved.map { NSColor($0.color) }, atLocations: resolved.map { CGFloat($0.location) }, colorSpace: .sRGB) {
                gradient.draw(in: inset, angle: 0)
            }
            NSGraphicsContext.restoreGraphicsState()
            // Resolved at draw time, so it follows the menu bar's light/dark appearance.
            NSColor.labelColor.setStroke()
            path.lineWidth = 1.5
            path.stroke()
            return true
        }
        image.isTemplate = false
        return image
    }

    /// Small filled circle used for menu rows.
    static func dot(color: NSColor, diameter: CGFloat = 7) -> NSImage {
        let image = NSImage(size: NSSize(width: diameter, height: diameter), flipped: false) { rect in
            color.setFill()
            NSBezierPath(ovalIn: rect).fill()
            return true
        }
        image.isTemplate = false
        return image
    }
}
