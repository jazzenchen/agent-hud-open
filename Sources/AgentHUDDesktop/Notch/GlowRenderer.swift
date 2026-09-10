import AppKit
import CoreImage
import AgentHUDCore

/// A pre-rendered, blurred glow bitmap. Rendering once per parameter change keeps the always-on breathing
/// animation a pure opacity composite instead of a live blur filter.
struct GlowImage {
    let image: CGImage
    /// Extra points on every side so the blur is not clipped by the bitmap edge.
    let padding: CGFloat
    /// Bitmap size in points, padding included.
    let size: CGSize
}

enum GlowRenderer {
    private static let ciContext = CIContext(options: [.useSoftwareRenderer: false])

    /// Keeps colour dense at the island's contour, then fades it out across the glow's range.
    static func render(
        glow: GlowGeometry,
        islandSize: CGSize,
        islandRadius: CGFloat,
        outwardOnly: Bool,
        stops: [GradientStop],
        scale: CGFloat
    ) -> GlowImage? {
        let padding = ceil(max(0, glow.blur) * 3)
        return renderBitmap(width: glow.width, height: glow.height, blur: outwardOnly ? 0 : glow.blur, padding: padding, scale: scale) { rect, context, space in
            // A zero-width band has no outward falloff; avoid dividing by its range.
            if outwardOnly && glow.blur > 0 && glow.sideInset > 0 {
                drawOutwardFalloff(glow: glow, islandSize: islandSize, islandRadius: islandRadius, in: rect, context: context, scale: scale)
            } else {
                drawFalloff(glow: glow, islandSize: islandSize, islandRadius: islandRadius, in: rect, context: context, scale: scale)
            }
            context.setBlendMode(.sourceIn)
            drawGradient(stops, in: rect, context: context, space: space)
        }
    }

    /// Feather only the outer edge; zero blur keeps the full band solid.
    private static func drawFalloff(
        glow: GlowGeometry,
        islandSize: CGSize,
        islandRadius: CGFloat,
        in rect: CGRect,
        context: CGContext,
        scale: CGFloat
    ) {
        let featherWidth = min(glow.sideInset, glow.blur * 2)
        guard featherWidth > 0 else {
            context.setFillColor(gray: 1, alpha: 1)
            context.addPath(bottomRoundedPath(rect, radius: glow.cornerRadius))
            context.fillPath()
            return
        }
        let radius = min(islandRadius, min(islandSize.width, islandSize.height) / 2)
        // Sample the feathered edge at half a device pixel.
        let steps = max(1, Int(ceil(featherWidth * scale * 2)))
        context.setBlendMode(.copy)
        for step in stride(from: steps, through: 0, by: -1) {
            let t = CGFloat(step) / CGFloat(steps)
            let inset = featherWidth * (1 - t)
            let contour = CGRect(
                x: rect.minX + inset, y: rect.minY + inset,
                width: rect.width - inset * 2, height: rect.height - inset
            )
            let contourRadius = radius + (glow.cornerRadius - radius) * (1 - inset / glow.sideInset)
            context.setFillColor(gray: 1, alpha: (1 - t) * (1 - t))
            context.addPath(bottomRoundedPath(contour, radius: contourRadius))
            context.fillPath()
        }
    }

    /// Start at full opacity on the island, then immediately decay with distance.
    /// More feather trades the broad solid band for a longer, fainter tail without blurring the rim.
    private static func drawOutwardFalloff(
        glow: GlowGeometry,
        islandSize: CGSize,
        islandRadius: CGFloat,
        in rect: CGRect,
        context: CGContext,
        scale: CGFloat
    ) {
        let range = glow.sideInset
        let reach = range + glow.blur * 3
        let power = 8 * glow.blur / range
        let radius = min(islandRadius, min(islandSize.width, islandSize.height) / 2)
        let island = CGRect(x: rect.minX + range, y: rect.minY + range,
                            width: islandSize.width, height: rect.height - range)
        let steps = max(1, Int(ceil(reach * scale * 2)))
        context.setBlendMode(.copy)
        for step in stride(from: steps, through: 0, by: -1) {
            let t = CGFloat(step) / CGFloat(steps)
            let distance = reach * t
            let contourRadius = radius + distance
            let contour = CGRect(
                x: island.minX - distance, y: island.minY - distance,
                width: island.width + distance * 2,
                // Keep the corner centres fixed; any extra height is above the screen edge.
                height: max(island.height + distance, contourRadius * 2)
            )
            context.setFillColor(gray: 1, alpha: pow(1 - t, power))
            context.addPath(bottomRoundedPath(contour, radius: contourRadius))
            context.fillPath()
        }
    }

    private static func renderBitmap(
        width: CGFloat,
        height: CGFloat,
        blur: CGFloat,
        padding: CGFloat,
        scale: CGFloat,
        draw: (CGRect, CGContext, CGColorSpace) -> Void
    ) -> GlowImage? {
        let totalWidth = width + padding * 2
        let totalHeight = height + padding * 2
        let pixelWidth = Int((totalWidth * scale).rounded(.up))
        let pixelHeight = Int((totalHeight * scale).rounded(.up))
        guard pixelWidth > 0, pixelHeight > 0,
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                data: nil, width: pixelWidth, height: pixelHeight, bitsPerComponent: 8, bytesPerRow: 0,
                space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              )
        else { return nil }

        context.scaleBy(x: scale, y: scale)
        let rect = CGRect(x: padding, y: padding, width: width, height: height)
        draw(rect, context, space)

        guard let base = context.makeImage() else { return nil }
        let size = CGSize(width: totalWidth, height: totalHeight)
        guard blur > 0, let filter = CIFilter(name: "CIGaussianBlur") else {
            return GlowImage(image: base, padding: padding, size: size)
        }
        filter.setValue(CIImage(cgImage: base), forKey: kCIInputImageKey)
        filter.setValue(blur * scale, forKey: kCIInputRadiusKey)
        guard let output = filter.outputImage,
              let blurred = ciContext.createCGImage(output, from: CGRect(x: 0, y: 0, width: pixelWidth, height: pixelHeight))
        else { return GlowImage(image: base, padding: padding, size: size) }
        return GlowImage(image: blurred, padding: padding, size: size)
    }

    /// Soft drop shadow (`0 8px 30px rgba(0,0,0,.35)`) for the expanded panel.
    static func renderShadow(width: CGFloat, height: CGFloat, cornerRadius: CGFloat, scale: CGFloat) -> GlowImage? {
        let blur: CGFloat = 15
        return renderBitmap(width: width, height: height, blur: blur, padding: ceil(blur * 3), scale: scale) { rect, context, space in
            let black = RGBA(hex: 0x000000, alpha: 0.35)
            context.addPath(bottomRoundedPath(rect, radius: min(cornerRadius, min(width, height) / 2)))
            context.clip()
            drawGradient([GradientStop(color: black, location: 0), GradientStop(color: black, location: 1)], in: rect, context: context, space: space)
        }
    }

    private static func drawGradient(_ stops: [GradientStop], in rect: CGRect, context: CGContext, space: CGColorSpace) {
        var resolved = stops
        if resolved.count == 1 {
            resolved = [GradientStop(color: stops[0].color, location: 0), GradientStop(color: stops[0].color, location: 1)]
        }
        guard !resolved.isEmpty else { return }
        let colors = resolved.map { CGColor.rgba($0.color) } as CFArray
        let locations = resolved.map { CGFloat($0.location) }
        guard let gradient = CGGradient(colorsSpace: space, colors: colors, locations: locations) else { return }
        context.drawLinearGradient(
            gradient,
            start: CGPoint(x: rect.minX, y: rect.midY),
            end: CGPoint(x: rect.maxX, y: rect.midY),
            options: [.drawsBeforeStartLocation, .drawsAfterEndLocation]
        )
    }

    /// Rectangle whose bottom corners are rounded; the top edge is square (it meets the screen edge).
    static func bottomRoundedPath(_ r: CGRect, radius: CGFloat) -> CGPath {
        let path = CGMutablePath()
        let radius = max(0, min(radius, min(r.width, r.height) / 2))
        path.move(to: CGPoint(x: r.minX, y: r.maxY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.maxY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.minY + radius))
        path.addArc(tangent1End: CGPoint(x: r.maxX, y: r.minY), tangent2End: CGPoint(x: r.maxX - radius, y: r.minY), radius: radius)
        path.addLine(to: CGPoint(x: r.minX + radius, y: r.minY))
        path.addArc(tangent1End: CGPoint(x: r.minX, y: r.minY), tangent2End: CGPoint(x: r.minX, y: r.minY + radius), radius: radius)
        path.closeSubpath()
        return path
    }
}
