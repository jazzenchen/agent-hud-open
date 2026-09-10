import Foundation

/// Geometry of the glow layer relative to the island it wraps.
///
/// From the design: width = island + 2·range, height = island + range + 3·blur,
/// top = −3·blur (the part above the screen edge is clipped, so the top edge reads as solid), radius = island + range.
public struct GlowGeometry: Hashable, Sendable {
    public let width: Double
    public let height: Double
    /// Negative offset of the glow's top edge relative to the island's top edge.
    public let topOffset: Double
    public let cornerRadius: Double
    public let blur: Double
    /// Inset of the island inside the glow on the left, right and bottom (== range).
    public let sideInset: Double

    public init(width: Double, height: Double, topOffset: Double, cornerRadius: Double, blur: Double, sideInset: Double) {
        self.width = width
        self.height = height
        self.topOffset = topOffset
        self.cornerRadius = cornerRadius
        self.blur = blur
        self.sideInset = sideInset
    }

    public static func compute(
        islandWidth: Double,
        islandHeight: Double,
        islandRadius: Double,
        range: Double,
        blur: Double
    ) -> GlowGeometry {
        GlowGeometry(
            width: islandWidth + range * 2,
            height: islandHeight + range + blur * 3,
            topOffset: -blur * 3,
            cornerRadius: islandRadius + range,
            blur: blur,
            sideInset: range
        )
    }

    /// Height of the part that is actually on screen (below the island's top edge).
    public var visibleHeight: Double { height + topOffset }
}
