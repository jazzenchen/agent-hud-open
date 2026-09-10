import SwiftUI
import QuartzCore

/// The surface, its glow and the canvas cleanup share one finite transition.
enum IslandAnimation {
    static let duration: TimeInterval = 0.4
    static let curve = Animation.timingCurve(0.2, 0.8, 0.2, 1, duration: duration)
    static let mediaCurve = CAMediaTimingFunction(controlPoints: 0.2, 0.8, 0.2, 1)
}
