import AppKit
import SwiftUI
import AgentHUDCore

/// A piece of the HUD that a light or glass look gives a shadow of its own, where it is on screen: the island or the
/// panel or reminder it has grown into, a detached card, or the Simple panel. `ScreenHUD` lays it out in the shape
/// `IslandRootView` fills it with.
struct HUDShell: Equatable, Sendable {
    /// The shell's core on screen, without its flares.
    let frame: CGRect
    /// The radius of its two corners away from the screen edge, or of all four on a shell that floats.
    let radius: CGFloat
    /// How far each flare reaches along the edge past the core; nothing on a shell that floats.
    let flare: CGFloat
    /// It floats off the edge, rounded all round, rather than growing out of it.
    let floats: Bool
    /// It is drawn transparent: a collapsed logo queue's silhouette, kept so that the panel grows out of it with its
    /// shadow rather than the shadow appearing at the panel's full size.
    let bare: Bool
}

/// The shells one screen's HUD shows at once, which a light or glass look gives shadows.
struct HUDShells: Equatable, Sendable {
    /// The island, the panel or reminder it has grown into, or a detached card; nothing while a dock that separates
    /// its agents shows only its marks.
    let surface: HUDShell?
    /// The Simple panel beside a detached card, or at rest on a dock that separates its agents. It stays where it is,
    /// and its shadow with it, while a card comes and goes beside it.
    let dock: HUDShell?
}

extension HUDLook {
    /// A light or see-through HUD's shells cast shadows of their own (`HUDShells`): a light shell needs one to stand off a
    /// light desktop, and glass or a see-through solid needs its shadow round it rather than under it, where it would
    /// show through. The standard look keeps the one shadow it has always had, under the glow's island and only with the
    /// glow.
    var castsShellShadows: Bool { appearance == .light || seeThrough }
}

/// The shape a shell fills, as the island draws it (`IslandShape`, or a detached card's continuous rounded rectangle),
/// in the glow window's top-edge space: y up, the parked edge above. Grown out of the edge, it has a flare each side
/// where it meets it and rounds the two corners away from it; floating, it rounds all four.
struct ShellShape: Hashable, Sendable {
    /// The core, without its flares.
    let size: CGSize
    let radius: CGFloat
    let flare: CGFloat
    let floats: Bool

    /// The shape with its core's lower-left corner at `origin`.
    func path(at origin: CGPoint) -> CGPath {
        let core = CGRect(origin: origin, size: size)
        guard !floats else {
            return Path(roundedRect: core, cornerRadius: max(0, min(radius, size.width / 2, size.height / 2)),
                        style: .continuous).cgPath
        }
        // `IslandShape`'s silhouette with the y axis turned up: the flares curve out to the edge above, and the corners
        // round below. Its rect is the core with the flares beside it, and it clamps both radii to it the same way.
        let rect = core.insetBy(dx: -flare, dy: 0)
        let top = max(0, min(flare, rect.width / 2, rect.height / 2))
        let bottom = max(0, min(radius, (rect.width - 2 * top) / 2, rect.height - top))
        let path = CGMutablePath()
        path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.addQuadCurve(to: CGPoint(x: rect.minX + top, y: rect.maxY - top), control: CGPoint(x: rect.minX + top, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX + top, y: rect.minY + bottom))
        path.addArc(tangent1End: CGPoint(x: rect.minX + top, y: rect.minY),
                    tangent2End: CGPoint(x: rect.minX + top + bottom, y: rect.minY), radius: bottom)
        path.addLine(to: CGPoint(x: rect.maxX - top - bottom, y: rect.minY))
        path.addArc(tangent1End: CGPoint(x: rect.maxX - top, y: rect.minY),
                    tangent2End: CGPoint(x: rect.maxX - top, y: rect.minY + bottom), radius: bottom)
        path.addLine(to: CGPoint(x: rect.maxX - top, y: rect.maxY - top))
        path.addQuadCurve(to: CGPoint(x: rect.maxX, y: rect.maxY), control: CGPoint(x: rect.maxX - top, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

/// A shell cut out of a shadow that a glass HUD would otherwise show through it: the shell's shape, and how far its
/// core sits from the core of the shell that casts the shadow.
struct ShellCutout: Hashable, Sendable {
    let shape: ShellShape
    let offset: CGSize
}

/// The island cut out of a glow drawn round it, which a glass island would otherwise show through it: the island's
/// shape as it draws it, flares included, where the glow's geometry puts it. A solid island hides that part itself.
struct GlowCutout: Hashable, Sendable {
    /// How far the island's flares reach along the screen edge past its sides.
    let flare: CGFloat

    /// The island inside a glow drawn in `rect`: inset by the glow's range at the sides and the foot, and reaching up
    /// to the screen edge, which lies the glow's top offset below the top of `rect`.
    func path(glow: GlowGeometry, islandRadius: CGFloat, in rect: CGRect) -> CGPath {
        let island = CGRect(x: rect.minX + glow.sideInset, y: rect.minY + glow.sideInset,
                            width: max(0, glow.width - glow.sideInset * 2),
                            height: max(0, glow.height - glow.sideInset + glow.topOffset))
        return ShellShape(size: island.size, radius: islandRadius, flare: flare, floats: false).path(at: island.origin)
    }
}

extension GlowCutout {
    /// The cutout a glow round an island of `look` needs: one flared by `flare` where the desktop shows through the look
    /// (`HUDLook.seeThrough`), none where the island is opaque and hides the glow under it.
    init?(look: HUDLook, flare: CGFloat) {
        guard look.seeThrough else { return nil }
        self.init(flare: flare)
    }
}
