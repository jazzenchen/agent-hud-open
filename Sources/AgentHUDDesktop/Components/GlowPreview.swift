import SwiftUI
import AgentHUDCore

/// Uses the desktop glow renderer for settings previews, onboarding and snapshots.
/// Breathing is computed from wall-clock time so it always reflects the current settings.
struct GlowPreview: View {
    @Environment(\.displayScale) private var displayScale
    let appearance: GlowAppearance
    let settings: AgentHUDCore.Settings
    let islandSize: CGSize
    let islandRadius: CGFloat
    /// Scale applied to range/blur so small previews keep the proportions of the real notch.
    var scale: CGFloat = 1
    var lightBorder = false

    var body: some View {
        let glow = GlowGeometry.compute(
            islandWidth: islandSize.width,
            islandHeight: islandSize.height,
            islandRadius: islandRadius,
            range: settings.glowRange * scale,
            blur: settings.glowBlur * scale
        )
        // Render outside the timeline so breathing only changes the bitmap's opacity.
        let rendered = appearance.hidden ? nil : GlowRenderer.render(
            glow: glow, islandSize: islandSize, islandRadius: islandRadius,
            outwardOnly: settings.glowOutwardOnly, stops: appearance.stops, scale: displayScale
        )
        TimelineView(.animation(paused: !appearance.breathing)) { context in
            ZStack(alignment: .top) {
                if let rendered {
                    Image(decorative: rendered.image, scale: displayScale)
                        .resizable()
                        .frame(width: rendered.size.width, height: rendered.size.height)
                        .frame(width: glow.width, height: glow.height)
                        .opacity(Self.opacity(appearance, at: context.date))
                        .offset(y: glow.topOffset)
                }
                BottomRoundedRectangle(radius: islandRadius)
                    .fill(Color.black)
                    .overlay {
                        if lightBorder {
                            BottomRoundedRectangle(radius: islandRadius).stroke(Color.white.opacity(0.18), lineWidth: 1)
                        }
                    }
                    .frame(width: islandSize.width, height: islandSize.height)
            }
        }
    }

    /// Cosine breathing between peak and trough with period `breathSeconds`.
    static func opacity(_ appearance: GlowAppearance, at date: Date) -> Double {
        guard appearance.breathing, appearance.breathSeconds > 0 else { return appearance.peakOpacity }
        let phase = date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: appearance.breathSeconds) / appearance.breathSeconds
        let wave = 0.5 - 0.5 * cos(phase * 2 * .pi)
        return appearance.peakOpacity - (appearance.peakOpacity - appearance.troughOpacity) * wave
    }
}
