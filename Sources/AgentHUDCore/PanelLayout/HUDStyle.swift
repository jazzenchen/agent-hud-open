import AgentHUDSupport
import Foundation

/// A change an agent proposed to one screen's resting HUD: the fields of that screen's placement and glow it sets, each
/// nil where it leaves Settings as they are. Settings stay the saved truth. A style waits as that screen's draft, drawn
/// over its settings (`applied(to:)`) until a person saves it into them or drops it (`PanelLayoutStore`), and a person's
/// own edit of that screen drops it first. Its values are within the ranges Settings keep them in.
///
/// It is written in an agent's words, as `draft_hud`'s `hud` takes it beside `agents`: `mode` is `notch`,
/// `dynamic` or `simple`; `position` is the placement's `offset`, `logoGap` its `gapScale`, `simpleGap` its
/// `simpleGapScale`, `edgeDistance` its
/// `logoEdgeDistance`, `showLogos`, `showRings` and `showNumbers` its `showsLogos`, `showsRings` and `showsNumbers`,
/// `separateAgents` its `separatesAgents` and `transparency` the material's own, while `appearance`, `material` and
/// `glassStyle` keep their names; under `glow`, `speed` is the glow's working period, `breathSeconds`, `idleSpeed` its
/// idle one, `idleBreathSeconds`, and `amplitude` its `breathAmplitude`, while the grid's `gridPitch`, `gridDensity`,
/// `gridCore` and `gridFade` keep their names.
public struct HUDStyle: Hashable, Sendable {
    public var mode: HUDMode?
    public var edge: HUDEdge?
    /// The HUD's centre along its edge, 0 to 1.
    public var position: Double?
    public var logoSize: Double?
    /// The gap between two logos, as a share of a logo.
    public var logoGap: Double?
    /// From a logo, its outline included, to its edge, in points.
    public var edgeDistance: Double?
    public var showLogos: Bool?
    public var simpleSize: Double?
    /// The gap between two agents in Simple mode, as a share of a ring.
    public var simpleGap: Double?
    public var ringWidth: Double?
    public var showRings: Bool?
    public var showNumbers: Bool?
    /// Whether a dock answers for one agent at a time.
    public var separateAgents: Bool?
    /// The colours of the HUD, its panel and its cards; `system` follows macOS.
    public var appearance: HUDAppearance?
    /// Their surface, solid or glass.
    public var material: HUDMaterial?
    /// What their glass is made of, clear or regular Liquid Glass or a material. It is kept while the material is
    /// solid, as Settings keeps it, and drawn once the material is glass.
    public var glassStyle: HUDGlassStyle?
    /// How much of the desktop the HUD lets through, 0 to 1, as the material in effect is: glass's, how little of its
    /// colour lies beneath the glass, or solid's, how little of its fill is drawn; Solid, Clear and Regular draw three
    /// fifths of it, Material all of it (`ScreenPlacement.transparencyScale(material:glassStyle:)`).
    public var transparency: Double?
    public var glow: Glow

    /// The glow's part of a style.
    public struct Glow: Hashable, Sendable {
        public var style: GlowStyle?
        public var effect: GlowEffect?
        public var brightness: Double?
        public var range: Double?
        public var blur: Double?
        /// The period every effect plays at while an agent works, in seconds.
        public var speed: Double?
        /// The period it slows to while every agent is idle, in seconds.
        public var idleSpeed: Double?
        /// How deep a breath dips.
        public var amplitude: Double?
        public var outwardOnly: Bool?
        /// A grid glow's spacing between cells, in points.
        public var gridPitch: Double?
        /// How much of its cell each of a grid glow's marks fills.
        public var gridDensity: Double?
        /// The rows a grid glow holds at full strength.
        public var gridCore: Double?
        /// The rows a grid glow fades over after them.
        public var gridFade: Double?

        public init(style: GlowStyle? = nil, effect: GlowEffect? = nil, brightness: Double? = nil, range: Double? = nil,
                    blur: Double? = nil, speed: Double? = nil, idleSpeed: Double? = nil, amplitude: Double? = nil,
                    outwardOnly: Bool? = nil, gridPitch: Double? = nil, gridDensity: Double? = nil,
                    gridCore: Double? = nil, gridFade: Double? = nil) {
            self.style = style
            self.effect = effect
            self.brightness = brightness
            self.range = range
            self.blur = blur
            self.speed = speed
            self.idleSpeed = idleSpeed
            self.amplitude = amplitude
            self.outwardOnly = outwardOnly
            self.gridPitch = gridPitch
            self.gridDensity = gridDensity
            self.gridCore = gridCore
            self.gridFade = gridFade
        }
    }

    public init(mode: HUDMode? = nil, edge: HUDEdge? = nil, position: Double? = nil, logoSize: Double? = nil,
                logoGap: Double? = nil, edgeDistance: Double? = nil, showLogos: Bool? = nil, simpleSize: Double? = nil,
                simpleGap: Double? = nil, ringWidth: Double? = nil, showRings: Bool? = nil, showNumbers: Bool? = nil,
                separateAgents: Bool? = nil, appearance: HUDAppearance? = nil, material: HUDMaterial? = nil,
                glassStyle: HUDGlassStyle? = nil, transparency: Double? = nil, glow: Glow = Glow()) {
        self.mode = mode
        self.edge = edge
        self.position = position
        self.logoSize = logoSize
        self.logoGap = logoGap
        self.edgeDistance = edgeDistance
        self.showLogos = showLogos
        self.simpleSize = simpleSize
        self.simpleGap = simpleGap
        self.ringWidth = ringWidth
        self.showRings = showRings
        self.showNumbers = showNumbers
        self.separateAgents = separateAgents
        self.appearance = appearance
        self.material = material
        self.glassStyle = glassStyle
        self.transparency = transparency
        self.glow = glow
    }

    /// Whether the style changes nothing.
    var isEmpty: Bool { self == HUDStyle() }

    /// `placement` with the fields this style sets in place of its own.
    func applied(to placement: ScreenPlacement) -> ScreenPlacement {
        var next = placement
        if let mode { next.mode = mode }
        if let edge { next.edge = edge }
        if let position { next.offset = position }
        if let logoSize { next.logoSize = logoSize }
        if let logoGap { next.gapScale = logoGap }
        if let edgeDistance { next.logoEdgeDistance = edgeDistance }
        if let showLogos { next.showsLogos = showLogos }
        if let simpleSize { next.simpleSize = simpleSize }
        if let simpleGap { next.simpleGapScale = simpleGap }
        if let ringWidth { next.ringWidth = ringWidth }
        if let showRings { next.showsRings = showRings }
        if let showNumbers { next.showsNumbers = showNumbers }
        if let separateAgents { next.separatesAgents = separateAgents }
        if let appearance { next.appearance = appearance }
        if let material { next.material = material }
        if let glassStyle { next.glassStyle = glassStyle }
        // The material in effect, this style's or the saved one, is the one whose transparency it sets.
        if let transparency { next.transparency = transparency }
        return next
    }

    /// `glow` with the fields this style sets in place of its own.
    func applied(to glow: GlowSettings) -> GlowSettings {
        var next = glow
        if let style = self.glow.style { next.style = style }
        if let effect = self.glow.effect { next.effect = effect }
        if let brightness = self.glow.brightness { next.brightness = brightness }
        if let range = self.glow.range { next.range = range }
        if let blur = self.glow.blur { next.blur = blur }
        if let speed = self.glow.speed { next.breathSeconds = speed }
        if let idleSpeed = self.glow.idleSpeed { next.idleBreathSeconds = idleSpeed }
        if let amplitude = self.glow.amplitude { next.breathAmplitude = amplitude }
        if let outwardOnly = self.glow.outwardOnly { next.outwardOnly = outwardOnly }
        if let gridPitch = self.glow.gridPitch { next.gridPitch = gridPitch }
        if let gridDensity = self.glow.gridDensity { next.gridDensity = gridDensity }
        if let gridCore = self.glow.gridCore { next.gridCore = gridCore }
        if let gridFade = self.glow.gridFade { next.gridFade = gridFade }
        return next
    }

    /// Every field an agent can set, as `placement` and `glow` have it: a screen's style in effect, as `get_hud` lists it.
    init(every placement: ScreenPlacement, glow: GlowSettings) {
        self.init(mode: placement.mode, edge: placement.edge, position: placement.offset, logoSize: placement.logoSize,
                  logoGap: placement.gapScale, edgeDistance: placement.logoEdgeDistance, showLogos: placement.showsLogos,
                  simpleSize: placement.simpleSize, simpleGap: placement.simpleGapScale, ringWidth: placement.ringWidth,
                  showRings: placement.showsRings,
                  showNumbers: placement.showsNumbers, separateAgents: placement.separatesAgents,
                  appearance: placement.appearance, material: placement.material, glassStyle: placement.glassStyle,
                  transparency: placement.transparency,
                  glow: Glow(style: glow.style, effect: glow.effect, brightness: glow.brightness, range: glow.range,
                             blur: glow.blur, speed: glow.breathSeconds, idleSpeed: glow.idleBreathSeconds,
                             amplitude: glow.breathAmplitude, outwardOnly: glow.outwardOnly, gridPitch: glow.gridPitch,
                             gridDensity: glow.gridDensity, gridCore: glow.gridCore, gridFade: glow.gridFade))
    }

    /// The style reduced to what it changes about a screen saved with `placement` and `glow`: a field set to the value
    /// the screen has saved is left out, so a style that spells out what is saved is empty. The transparency is the one
    /// of the material the style puts in effect, its own or the saved one, so a change of material keeps a transparency
    /// that only repeats the other material's.
    func reduced(placement: ScreenPlacement, glow: GlowSettings) -> HUDStyle {
        var saved = HUDStyle(every: placement, glow: glow)
        var inEffect = placement
        if let material { inEffect.material = material }
        saved.transparency = inEffect.transparency
        var kept = self
        for field in Field.allCases where kept.value(field) == saved.value(field) { kept.set(field, .null) }
        for field in GlowField.allCases where kept.glow.value(field) == saved.glow.value(field) { kept.glow.set(field, .null) }
        return kept
    }
}

/// A screen's style draft: the style, as an agent reads it, and whether the screen has a notch, which decides the
/// placement it is drawn over where the screen has none of its own saved. A draft that brings a version of the screen
/// back, its previous one or the default, also holds that version's look as it was kept (`restoring`): the screen draws
/// that look whole, and Save writes it back as it was, a placement or a glow the screen had none of included.
struct PanelStyleDraft: Equatable, Sendable {
    let style: HUDStyle
    let hasNotch: Bool
    var restoring: ScreenLook? = nil

    /// The placement the screen draws with the draft over the `saved` one; `hasNotch` is the screen's.
    func placement(over saved: ScreenPlacement, hasNotch: Bool) -> ScreenPlacement {
        guard let restoring else { return style.applied(to: saved) }
        return restoring.placement ?? .default(hasNotch: hasNotch)
    }

    /// The glow the screen draws with the draft over the `saved` one; `defaultGlow` is `Settings.glow`.
    func glow(over saved: GlowSettings, defaultGlow: GlowSettings) -> GlowSettings {
        guard let restoring else { return style.applied(to: saved) }
        return restoring.glow ?? defaultGlow
    }

    /// Whether the draft draws more than its style lists over the `saved` placement and glow: a version it brings back
    /// that differs in what no style field names, such as the transparency kept for the material not drawn.
    func restoresUnlisted(over saved: ScreenPlacement, glow: GlowSettings, defaultGlow: GlowSettings) -> Bool {
        guard restoring != nil else { return false }
        return style.applied(to: saved) != placement(over: saved, hasNotch: hasNotch)
            || style.applied(to: glow) != self.glow(over: glow, defaultGlow: defaultGlow)
    }

    /// The screen's own placement and glow once the draft is written over `look`: the version it restores, else the
    /// style over `look`, where a part the style changes nothing of stays as it was, none included, so a screen that
    /// draws a default keeps drawing it.
    /// - defaultGlow: the glow a screen without one of its own draws (`Settings.glow`).
    func applied(to look: ScreenLook, defaultGlow: GlowSettings) -> ScreenLook {
        if let restoring { return restoring }
        let placement = look.placement ?? .default(hasNotch: hasNotch), glow = look.glow ?? defaultGlow
        let drawnPlacement = style.applied(to: placement), drawnGlow = style.applied(to: glow)
        return ScreenLook(placement: drawnPlacement == placement ? look.placement : drawnPlacement,
                          glow: drawnGlow == glow ? look.glow : drawnGlow)
    }
}

extension HUDMode {
    /// The mode as Agent HUD names it to a person and to an agent: notch, dynamic or simple.
    var agentName: String {
        switch self {
        case .notch: "notch"
        case .logos: "dynamic"
        case .simple: "simple"
        }
    }

    /// The mode an agent names; nil for a name that is none.
    init?(agentName: String) {
        guard let mode = HUDMode.allCases.first(where: { $0.agentName == agentName }) else { return nil }
        self = mode
    }
}

// MARK: Fields

extension HUDStyle {
    /// A field of a screen's placement an agent can set, by the name it uses for it.
    enum Field: String, CaseIterable, Sendable {
        case mode, edge, position, logoSize, logoGap, edgeDistance, showLogos, simpleSize, simpleGap, ringWidth,
             showRings, showNumbers, separateAgents, appearance, material, glassStyle, transparency

        /// The range a number keeps within, Settings' own; nil for a field that is no number.
        var range: ClosedRange<Double>? {
            switch self {
            case .position: 0...1
            case .logoSize: ScreenPlacement.logoSizeRange
            case .logoGap, .simpleGap: ScreenPlacement.gapScaleRange
            case .edgeDistance: ScreenPlacement.logoEdgeDistanceRange
            case .simpleSize: ScreenPlacement.simpleSizeRange
            case .ringWidth: ScreenPlacement.ringWidthRange
            case .transparency: ScreenPlacement.glassTransparencyRange
            case .mode, .edge, .showLogos, .showRings, .showNumbers, .separateAgents, .appearance, .material,
                 .glassStyle: nil
            }
        }

        /// The modes whose HUD the field changes, as Settings shows it for them. The look's fields change every mode's.
        var modes: [HUDMode] {
            switch self {
            case .mode, .appearance, .material, .glassStyle, .transparency: HUDMode.allCases
            case .edge, .position, .separateAgents: [.logos, .simple]
            case .logoSize, .logoGap, .edgeDistance, .showLogos: [.logos]
            case .simpleSize, .simpleGap, .ringWidth, .showRings, .showNumbers: [.simple]
            }
        }
    }

    /// A field of a screen's glow an agent can set, by the name it uses for it.
    enum GlowField: String, CaseIterable, Sendable {
        case style, effect, brightness, range, blur, speed, idleSpeed, amplitude, outwardOnly, gridPitch, gridDensity,
             gridCore, gridFade

        /// The range a number keeps within, Settings' own; nil for a field that is no number.
        var range: ClosedRange<Double>? {
            switch self {
            case .brightness: GlowSettings.brightnessRange
            case .range, .blur: GlowSettings.sizeRange
            case .speed: GlowSettings.workingPeriodRange
            case .idleSpeed: GlowSettings.breathSecondsRange
            case .amplitude: GlowSettings.amplitudeRange
            case .gridPitch: GlowSettings.gridPitchRange
            case .gridDensity: GlowSettings.gridDensityRange
            case .gridCore: GlowSettings.gridCoreRange
            case .gridFade: GlowSettings.gridFadeRange
            case .style, .effect, .outwardOnly: nil
            }
        }

        /// The step a number keeps to from the start of its range, the one Settings' slider moves it by: a grid's pitch
        /// in whole points, its core and fade in whole rows, its density in twentieths and the idle period in half
        /// seconds. Nil for a field that takes any number in its range.
        var step: Double? {
            switch self {
            case .gridPitch, .gridCore, .gridFade: 1
            case .gridDensity: 0.05
            case .idleSpeed: 0.5
            case .style, .effect, .brightness, .range, .blur, .speed, .amplitude, .outwardOnly: nil
            }
        }

        /// Whether the glow draws the field in `style`, as Settings shows its row only for that: a range, a feather and
        /// an outward fade for the soft glow, `blur`, a pitch, a density, a core and a fade for the grid glows, and
        /// every other field for any style.
        func isDrawn(in style: GlowStyle) -> Bool {
            switch self {
            case .range, .blur, .outwardOnly: style == .blur
            case .gridPitch, .gridDensity, .gridCore, .gridFade: style != .blur
            case .style, .effect, .brightness, .speed, .idleSpeed, .amplitude: true
            }
        }
    }

    /// A field's value as an agent writes it; nil while the style leaves it.
    func value(_ field: Field) -> JSONValue? {
        switch field {
        case .mode: mode.map { .string($0.agentName) }
        case .edge: edge.map { .string($0.rawValue) }
        case .position: position.map(JSONValue.number)
        case .logoSize: logoSize.map(JSONValue.number)
        case .logoGap: logoGap.map(JSONValue.number)
        case .edgeDistance: edgeDistance.map(JSONValue.number)
        case .showLogos: showLogos.map(JSONValue.bool)
        case .simpleSize: simpleSize.map(JSONValue.number)
        case .simpleGap: simpleGap.map(JSONValue.number)
        case .ringWidth: ringWidth.map(JSONValue.number)
        case .showRings: showRings.map(JSONValue.bool)
        case .showNumbers: showNumbers.map(JSONValue.bool)
        case .separateAgents: separateAgents.map(JSONValue.bool)
        case .appearance: appearance.map { .string($0.rawValue) }
        case .material: material.map { .string($0.rawValue) }
        case .glassStyle: glassStyle.map { .string($0.rawValue) }
        case .transparency: transparency.map(JSONValue.number)
        }
    }

    /// Sets a field from what an agent wrote: a value the field takes, within its range, or nothing for any other.
    mutating func set(_ field: Field, _ json: JSONValue) {
        let number = json.numberValue.flatMap { value in field.range.map { $0.contains(value) } == true ? value : nil }
        switch field {
        case .mode: mode = json.stringValue.flatMap(HUDMode.init(agentName:))
        case .edge: edge = json.stringValue.flatMap(HUDEdge.init(rawValue:))
        case .position: position = number
        case .logoSize: logoSize = number
        case .logoGap: logoGap = number
        case .edgeDistance: edgeDistance = number
        case .showLogos: showLogos = json.boolValue
        case .simpleSize: simpleSize = number
        case .simpleGap: simpleGap = number
        case .ringWidth: ringWidth = number
        case .showRings: showRings = json.boolValue
        case .showNumbers: showNumbers = json.boolValue
        case .separateAgents: separateAgents = json.boolValue
        case .appearance: appearance = json.stringValue.flatMap(HUDAppearance.init(rawValue:))
        case .material: material = json.stringValue.flatMap(HUDMaterial.init(rawValue:))
        case .glassStyle: glassStyle = json.stringValue.flatMap(HUDGlassStyle.init(rawValue:))
        case .transparency: transparency = number
        }
    }

    /// The style as an agent writes it: the fields it sets, the glow's under `glow`, and none it leaves.
    var json: [String: JSONValue] {
        var fields: [String: JSONValue] = [:]
        for field in Field.allCases { fields[field.rawValue] = value(field) }
        var glowFields: [String: JSONValue] = [:]
        for field in GlowField.allCases { glowFields[field.rawValue] = glow.value(field) }
        if !glowFields.isEmpty { fields["glow"] = .object(glowFields) }
        return fields
    }

    /// The style fields of a hud as an agent wrote them, each one a value it takes; `agents` and keys that name no
    /// field are passed over. A proposal is checked first (`misreadings(of:)`), so nothing it asks for is lost here.
    init(json fields: [String: JSONValue]) {
        self.init()
        for field in Field.allCases { if let value = fields[field.rawValue] { set(field, value) } }
        for field in GlowField.allCases { if let value = fields["glow"]?.objectValue?[field.rawValue] { glow.set(field, value) } }
    }

    /// The names of the style's fields, as a hud takes them beside `agents`.
    static var names: [String] { Field.allCases.map(\.rawValue) + ["glow"] }

    /// The hud's switch that asks for a screen's default style whole, in place of the style's fields: the placement its
    /// hardware is given, dark and solid, and the default glow, as the HUD style's Reset to Default gives them.
    static let resetName = "reset"

    /// A transparency a screen keeps for one material, solid's or glass's, which `get_hud` lists beside the style so
    /// that an agent can tell what a change of material draws. No proposal sets one, since `transparency` sets the one
    /// of the material the HUD would have, but a hud may give one as it was listed, so the style listed can be sent
    /// back whole.
    enum KeptTransparency: String, CaseIterable, Sendable {
        case solidTransparency, glassTransparency

        /// The material whose transparency it is.
        var material: HUDMaterial { self == .solidTransparency ? .solid : .glass }

        /// What `placement` keeps of it.
        func value(in placement: ScreenPlacement) -> Double {
            self == .solidTransparency ? placement.solidTransparency : placement.glassTransparency
        }
    }
}

extension HUDStyle.Glow {
    func value(_ field: HUDStyle.GlowField) -> JSONValue? {
        switch field {
        case .style: style.map { .string($0.rawValue) }
        case .effect: effect.map { .string($0.rawValue) }
        case .brightness: brightness.map(JSONValue.number)
        case .range: range.map(JSONValue.number)
        case .blur: blur.map(JSONValue.number)
        case .speed: speed.map(JSONValue.number)
        case .idleSpeed: idleSpeed.map(JSONValue.number)
        case .amplitude: amplitude.map(JSONValue.number)
        case .outwardOnly: outwardOnly.map(JSONValue.bool)
        case .gridPitch: gridPitch.map(JSONValue.number)
        case .gridDensity: gridDensity.map(JSONValue.number)
        case .gridCore: gridCore.map(JSONValue.number)
        case .gridFade: gridFade.map(JSONValue.number)
        }
    }

    /// Sets a field from what an agent wrote: a value the field takes, within its range, or nothing for any other. A
    /// number between the steps it keeps to is read, and refused as the screen's issue (`HUDStyle.issues`), so that one
    /// repeating what is saved changes nothing.
    mutating func set(_ field: HUDStyle.GlowField, _ json: JSONValue) {
        let number = json.numberValue.flatMap { value in field.range.map { $0.contains(value) } == true ? value : nil }
        switch field {
        case .style: style = json.stringValue.flatMap(GlowStyle.init(rawValue:))
        case .effect: effect = json.stringValue.flatMap(GlowEffect.init(rawValue:))
        case .brightness: brightness = number
        case .range: range = number
        case .blur: blur = number
        case .speed: speed = number
        case .idleSpeed: idleSpeed = number
        case .amplitude: amplitude = number
        case .outwardOnly: outwardOnly = json.boolValue
        case .gridPitch: gridPitch = number
        case .gridDensity: gridDensity = number
        case .gridCore: gridCore = number
        case .gridFade: gridFade = number
        }
    }
}
