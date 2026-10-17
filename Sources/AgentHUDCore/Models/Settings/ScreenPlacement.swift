import Foundation

/// How one screen presents the HUD.
public enum HUDMode: String, Codable, Sendable, CaseIterable {
    /// The island: the notch itself, or a bar standing in for one on displays without.
    case notch
    /// A queue of the agents' own logos, parked on a screen edge, with the glow behind them as a backdrop.
    case logos
    /// A small floating panel on a screen edge with no glow behind it: each agent's mark, with its tightest
    /// window as a ring round it, a percentage beside it, or both.
    case simple

    /// Parked on a screen edge and opening inward from it, rather than standing at the notch.
    public var isDocked: Bool { self != .notch }
}

/// The screen edge a HUD is parked on. It runs along that edge and opens inward.
public enum HUDEdge: String, Codable, Sendable, CaseIterable {
    case top, right, bottom, left

    public var isHorizontal: Bool { self == .top || self == .bottom }

    /// Unit vector pointing into the screen from this edge.
    public var inward: (x: Double, y: Double) {
        switch self {
        case .top: return (0, -1)
        case .bottom: return (0, 1)
        case .left: return (1, 0)
        case .right: return (-1, 0)
        }
    }
}

/// The colours one screen's HUD is drawn in: light ink on a dark surface, the HUD as it has always been, dark ink on a
/// light one, or whichever of the two macOS is set to. It is the HUD's own, apart from the Appearance the app's windows
/// take (`Settings.appearance`).
public enum HUDAppearance: String, Codable, Sendable, CaseIterable {
    case dark
    case light
    /// Light or dark as macOS is set, whatever the app's own Appearance.
    case system
}

/// What one screen's HUD surface is made of: an opaque fill, or glass that lets the desktop behind it show through,
/// blurred. Glass keeps the HUD's appearance, dark glass with light ink or light glass with dark ink, whatever is behind
/// it.
public enum HUDMaterial: String, Codable, Sendable, CaseIterable {
    case solid
    case glass
}

/// What a screen's glass is made of (`ScreenPlacement.glassStyle`), for the HUD and, with Match screen on, the app's
/// windows on that screen: macOS 26's Liquid Glass in its clear or its regular variant, or the frosted blur of a SwiftUI
/// regular material. Clear glass shows the most of the desktop; regular glass more of its own frost, and it is lighter
/// or darker as the piece it is drawn in is small or large, so the HUD, whose pieces sit side by side, draws Regular as
/// clear glass too, and only a piece on its own, a window or the menu-bar menu, the regular glass; the material blurs
/// evenly, without glass's lit rim.
public enum HUDGlassStyle: String, Codable, Sendable, CaseIterable {
    case clear
    case regular
    case material
}

/// One screen's HUD. Everything here is per screen: two displays can run different modes, sit on
/// different edges and size their logos differently. The glow is per screen too, kept beside this in
/// `Settings.screenGlow` under the same display key, with `Settings.glow` for a display that has none.
public struct ScreenPlacement: Hashable, Codable, Sendable {
    /// Side of one mark in points, independent of the display's menu bar height.
    public static let logoSizeRange: ClosedRange<Double> = 20...60
    /// Distance from the visible logo, including its outline, to its parked screen edge, in points.
    public static let logoEdgeDistanceRange: ClosedRange<Double> = 0...20
    /// Gap between logos, as a share of the logo's height, and in Simple mode between rings, as a share of a ring. A
    /// gap wider than about half a mark reads as separate marks rather than one queue.
    public static let gapScaleRange: ClosedRange<Double> = 0.1...0.6
    /// Diameter of one ring in Simple mode, in points. The marks and numbers are sized from it.
    public static let simpleSizeRange: ClosedRange<Double> = 20...60
    /// Stroke of that ring, in points.
    public static let ringWidthRange: ClosedRange<Double> = 1...4
    /// The Transparency chosen for glass, the slider's 0 to 100 %: how little of the look's own colour lies beneath the
    /// glass, as it is drawn (`drawsTransparency(reducesTransparency:)`), which Liquid Glass scales down. The glass stays
    /// whole and frosted at every step.
    public static let glassTransparencyRange: ClosedRange<Double> = 0...1
    /// The Transparency chosen for a solid surface, the slider's 0 to 100 %: how little of its fill is drawn, scaled down
    /// as Liquid Glass's is.
    public static let solidTransparencyRange: ClosedRange<Double> = 0...1
    /// How much of the Transparency chosen a surface draws: Solid and Liquid Glass, clear or regular, three fifths of it,
    /// so that their top, 100 %, draws 60 %, and the material all of it. The owner chose it: a solid fill or clear glass
    /// thinner than that leaves the HUD's ink on all but the bare desktop, and regular glass with little of the look's
    /// colour beneath it turns light or dark by itself, piece by piece, by the size of each, so that a narrow strip came
    /// out white beside a dark dock. The material frosts what is behind it evenly on its own.
    public static func transparencyScale(material: HUDMaterial, glassStyle: HUDGlassStyle) -> Double {
        material == .glass && glassStyle == .material ? 1 : 0.6
    }
    /// The range of the transparency of `material`, glass's or solid's.
    public static func transparencyRange(for material: HUDMaterial) -> ClosedRange<Double> {
        material == .glass ? glassTransparencyRange : solidTransparencyRange
    }
    /// Glass at first: the slider at half, which Clear and Regular draw at 0.3, 70 % of the look's colour beneath the
    /// glass, glass that is plainly dark or light and plainly glass, and the material at 0.5.
    public static let standardGlassTransparency = 0.5
    /// A solid surface at first: opaque, as the HUD has always been.
    public static let standardSolidTransparency = 0.0
    public var mode: HUDMode
    public var edge: HUDEdge
    /// The queue's centre along its edge, as a fraction of that edge's length, so it survives a
    /// resolution change.
    public var offset: Double
    public var logoSize: Double
    public var logoEdgeDistance: Double
    public var gapScale: Double
    /// Draw the marks. Turning them off leaves the backdrop alone — it still spans what the marks would
    /// have occupied, so the field keeps its place and its width; only the logos stop being drawn.
    public var showsLogos: Bool
    public var simpleSize: Double
    /// Gap between two agents in Simple mode, as a share of a ring (`gapScaleRange`). Marks alone in a row, with no
    /// numbers between them, sit at 70 % of it.
    public var simpleGapScale: Double
    public var ringWidth: Double
    /// Simple mode draws each agent's tightest window as a ring round its mark.
    public var showsRings: Bool
    /// Simple mode writes the percentage used beside each mark.
    public var showsNumbers: Bool
    /// A docked HUD answers for one agent at a time: hovering a mark opens that agent's own panel from the mark. A
    /// request or reminder grows from the mark of the agent that sent it on every dock, separated or not.
    public var separatesAgents: Bool
    /// The colours of the HUD, its panel and its cards: dark, the HUD as it has always been, unless chosen otherwise.
    public var appearance: HUDAppearance
    /// The surface of the HUD, its panel and its cards: solid unless chosen otherwise.
    public var material: HUDMaterial
    /// What glass is made of when the material is glass: clear Liquid Glass unless chosen otherwise. Kept while the
    /// material is solid.
    public var glassStyle: HUDGlassStyle
    /// How much of the desktop shows through glass as the slider sets it, from 0, the look's colour whole beneath it, to
    /// 1, which Clear and Regular draw at 0.6, 40 % of the colour left, and the material as its bare self
    /// (`glassTransparencyRange`, `transparencyScale(material:glassStyle:)`). Kept while the material is solid, for when
    /// glass is chosen again.
    public var glassTransparency: Double
    /// How much of the desktop shows through a solid surface, unblurred, as the slider sets it, 0 to 1
    /// (`solidTransparencyRange`), drawn at three fifths of it; 0, opaque, by default. Kept while the material is glass.
    public var solidTransparency: Double

    public init(
        mode: HUDMode = .logos,
        edge: HUDEdge = .top,
        offset: Double = 0.5,
        logoSize: Double = 20,
        logoEdgeDistance: Double = 2,
        gapScale: Double = 0.4,
        showsLogos: Bool = true,
        simpleSize: Double = 22,
        simpleGapScale: Double = 0.45,
        ringWidth: Double = 1.5,
        showsRings: Bool = true,
        showsNumbers: Bool = true,
        separatesAgents: Bool = false,
        appearance: HUDAppearance = .dark,
        material: HUDMaterial = .solid,
        glassStyle: HUDGlassStyle = .clear,
        glassTransparency: Double = ScreenPlacement.standardGlassTransparency,
        solidTransparency: Double = ScreenPlacement.standardSolidTransparency
    ) {
        self.mode = mode
        self.edge = edge
        self.offset = Self.clamp(offset, to: 0...1)
        self.logoSize = Self.clamp(logoSize, to: Self.logoSizeRange)
        self.logoEdgeDistance = Self.clamp(logoEdgeDistance, to: Self.logoEdgeDistanceRange)
        self.gapScale = Self.clamp(gapScale, to: Self.gapScaleRange)
        self.showsLogos = showsLogos
        self.simpleSize = Self.clamp(simpleSize, to: Self.simpleSizeRange)
        self.simpleGapScale = Self.clamp(simpleGapScale, to: Self.gapScaleRange)
        self.ringWidth = Self.clamp(ringWidth, to: Self.ringWidthRange)
        self.showsRings = showsRings
        self.showsNumbers = showsNumbers
        self.separatesAgents = separatesAgents
        self.appearance = appearance
        self.material = material
        self.glassStyle = glassStyle
        self.glassTransparency = Self.clamp(glassTransparency, to: Self.glassTransparencyRange)
        self.solidTransparency = Self.clamp(solidTransparency, to: Self.solidTransparencyRange)
    }

    /// A display's starting point: a notched screen keeps its island, anything else shows the queue, both dark and solid.
    public static func `default`(hasNotch: Bool) -> ScreenPlacement {
        ScreenPlacement(mode: hasNotch ? .notch : .logos)
    }

    private static func clamp(_ value: Double, to range: ClosedRange<Double>) -> Double {
        min(range.upperBound, max(range.lowerBound, value.isFinite ? value : range.lowerBound))
    }

    private enum CodingKeys: String, CodingKey {
        case mode, edge, offset, logoSize, logoEdgeDistance, gapScale, showsLogos
        case simpleSize, simpleGapScale, ringWidth, showsRings, showsNumbers, separatesAgents
        case appearance, material, glassStyle, glassTransparency, solidTransparency
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = ScreenPlacement()
        self.init(
            mode: (try? c.decodeIfPresent(HUDMode.self, forKey: .mode)) ?? d.mode,
            edge: (try? c.decodeIfPresent(HUDEdge.self, forKey: .edge)) ?? d.edge,
            offset: try c.decodeIfPresent(Double.self, forKey: .offset) ?? d.offset,
            logoSize: try c.decodeIfPresent(Double.self, forKey: .logoSize) ?? d.logoSize,
            logoEdgeDistance: try c.decodeIfPresent(Double.self, forKey: .logoEdgeDistance) ?? d.logoEdgeDistance,
            gapScale: try c.decodeIfPresent(Double.self, forKey: .gapScale) ?? d.gapScale,
            showsLogos: try c.decodeIfPresent(Bool.self, forKey: .showsLogos) ?? d.showsLogos,
            simpleSize: try c.decodeIfPresent(Double.self, forKey: .simpleSize) ?? d.simpleSize,
            simpleGapScale: try c.decodeIfPresent(Double.self, forKey: .simpleGapScale) ?? d.simpleGapScale,
            ringWidth: try c.decodeIfPresent(Double.self, forKey: .ringWidth) ?? d.ringWidth,
            showsRings: try c.decodeIfPresent(Bool.self, forKey: .showsRings) ?? d.showsRings,
            showsNumbers: try c.decodeIfPresent(Bool.self, forKey: .showsNumbers) ?? d.showsNumbers,
            separatesAgents: try c.decodeIfPresent(Bool.self, forKey: .separatesAgents) ?? d.separatesAgents,
            // A name saved by a newer build falls back to the default instead of failing the whole placement, and with
            // it every screen's (`Settings.screens`).
            appearance: (try? c.decodeIfPresent(HUDAppearance.self, forKey: .appearance)) ?? d.appearance,
            material: (try? c.decodeIfPresent(HUDMaterial.self, forKey: .material)) ?? d.material,
            glassStyle: (try? c.decodeIfPresent(HUDGlassStyle.self, forKey: .glassStyle)) ?? d.glassStyle,
            glassTransparency: try c.decodeIfPresent(Double.self, forKey: .glassTransparency) ?? d.glassTransparency,
            solidTransparency: try c.decodeIfPresent(Double.self, forKey: .solidTransparency) ?? d.solidTransparency
        )
    }
}

// MARK: The look drawn

extension ScreenPlacement {
    /// Whether the HUD is drawn light: its appearance, `system` as macOS is set now (`systemIsLight`). Every mode draws
    /// its look, Notch mode at a hardware notch too, where the camera housing itself stays black and the look grows
    /// round it.
    public func drawsLight(systemIsLight: Bool) -> Bool {
        switch appearance {
        case .dark: return false
        case .light: return true
        case .system: return systemIsLight
        }
    }

    /// Whether the HUD is drawn as glass: its material, solid while Reduce Transparency is on (`reducesTransparency`).
    public func drawsGlass(reducesTransparency: Bool) -> Bool {
        material == .glass && !reducesTransparency
    }

    /// The transparency of the material chosen, glass's or solid's.
    public var transparency: Double {
        get { material == .glass ? glassTransparency : solidTransparency }
        set {
            let value = Self.clamp(newValue, to: Self.transparencyRange(for: material))
            if material == .glass { glassTransparency = value } else { solidTransparency = value }
        }
    }

    /// How much of the desktop the HUD lets through as it is drawn: its material's Transparency scaled for the surface
    /// (`transparencyScale(material:glassStyle:)`), 60 % at most for Solid, Clear and Regular, and none at all while Reduce
    /// Transparency is on, which draws glass solid and a see-through solid opaque.
    public func drawsTransparency(reducesTransparency: Bool) -> Double {
        reducesTransparency ? 0 : transparency * Self.transparencyScale(material: material, glassStyle: glassStyle)
    }
}
