import AgentHUDSupport
import Foundation

extension HUDStyle {
    /// What is wrong with one style field of a hud as an agent sent it, its `reset`, or a transparency a screen keeps
    /// (`KeptTransparency`), as sentences: a value of the wrong type or out of its range, a mode, edge, appearance,
    /// material, glass style, glow style or effect this build does not know, or a property the glow does not have. Nil
    /// for a key that names no style field, which the caller words. A value given as null is not given. Whether a kept
    /// transparency repeats what the screen keeps is the proposal's to tell (`HUDTools.StyleProposal`).
    static func misreadings(ofField name: String, _ value: JSONValue) -> [String]? {
        if name == "glow" { return glowMisreadings(value) }
        if name == resetName { return value == .null || value.boolValue != nil ? [] : ["hud.\(name) is true or false."] }
        if let kept = KeptTransparency(rawValue: name) {
            if value == .null { return [] }
            return number(value, at: "hud.\(name)", within: ScreenPlacement.transparencyRange(for: kept.material))
        }
        guard let field = Field(rawValue: name) else { return nil }
        if value == .null { return [] }
        let path = "hud.\(name)"
        switch field {
        case .mode:
            return HUDMode(agentName: value.stringValue ?? "") == nil
                ? ["\(path) is one of \(PanelLayout.quoted(HUDMode.allCases.map(\.agentName)))."] : []
        case .edge:
            return HUDEdge(rawValue: value.stringValue ?? "") == nil
                ? ["\(path) is one of \(PanelLayout.quoted(HUDEdge.allCases.map(\.rawValue)))."] : []
        case .appearance:
            return HUDAppearance(rawValue: value.stringValue ?? "") == nil
                ? ["\(path) is one of \(PanelLayout.quoted(HUDAppearance.allCases.map(\.rawValue)))."] : []
        case .material:
            return HUDMaterial(rawValue: value.stringValue ?? "") == nil
                ? ["\(path) is one of \(PanelLayout.quoted(HUDMaterial.allCases.map(\.rawValue)))."] : []
        case .glassStyle:
            return HUDGlassStyle(rawValue: value.stringValue ?? "") == nil
                ? ["\(path) is one of \(PanelLayout.quoted(HUDGlassStyle.allCases.map(\.rawValue)))."] : []
        case .showLogos, .showRings, .showNumbers, .separateAgents:
            return value.boolValue == nil ? ["\(path) is true or false."] : []
        case .position, .logoSize, .logoGap, .edgeDistance, .simpleSize, .simpleGap, .ringWidth, .transparency:
            return number(value, at: path, within: field.range)
        }
    }

    private static func glowMisreadings(_ value: JSONValue) -> [String] {
        if value == .null { return [] }
        guard let glow = value.objectValue else { return ["hud.glow is an object."] }
        var found: [String] = []
        for property in glow.keys.sorted() {
            let value = glow[property] ?? .null
            let path = "hud.glow.\(property)"
            guard let field = GlowField(rawValue: property) else {
                found.append("hud.glow has no property \"\(property)\"; the glow takes "
                             + "\(listed(GlowField.allCases.map(\.rawValue))).")
                continue
            }
            if value == .null { continue }
            switch field {
            case .style:
                if GlowStyle(rawValue: value.stringValue ?? "") == nil {
                    found.append("\(path) is one of \(PanelLayout.quoted(GlowStyle.allCases.map(\.rawValue))).")
                }
            case .effect:
                if GlowEffect(rawValue: value.stringValue ?? "") == nil {
                    found.append("\(path) is one of \(PanelLayout.quoted(GlowEffect.allCases.map(\.rawValue))).")
                }
            case .outwardOnly:
                if value.boolValue == nil { found.append("\(path) is true or false.") }
            case .brightness, .range, .blur, .speed, .idleSpeed, .amplitude, .gridPitch, .gridDensity, .gridCore,
                 .gridFade:
                found += number(value, at: path, within: field.range, step: field.step)
            }
        }
        return found
    }

    /// What is wrong with a number of a hud: none, or one out of `range`, worded with the `step` it keeps to. Whether
    /// one within the range keeps to the step is the screen's issue (`issues`), since one repeating what is saved is
    /// none.
    private static func number(_ value: JSONValue, at path: String, within range: ClosedRange<Double>?,
                               step: Double? = nil) -> [String] {
        guard let range else { return [] }
        if let number = value.numberValue, range.contains(number) { return [] }
        return ["\(path) is \(numbered(range, step: step))."]
    }

    /// A number's range as a sentence writes it, with the step it keeps to: "a number from 0 to 1", "a whole number
    /// from 4 to 12", "a number from 0.5 to 1.5 in steps of 0.05".
    static func numbered(_ range: ClosedRange<Double>, step: Double?) -> String {
        let bounds = "from \(written(range.lowerBound)) to \(written(range.upperBound))"
        guard let step else { return "a number \(bounds)" }
        return step == 1 ? "a whole number \(bounds)" : "a number \(bounds) in steps of \(written(step))"
    }

    /// Whether `value` lies on a step of `step` from `start`, as a slider that moves by it sets it, within what
    /// floating point leaves of a decimal step.
    static func keeps(_ value: Double, to step: Double, from start: Double) -> Bool {
        let steps = (value - start) / step
        return abs(steps - steps.rounded()) < 0.000_001
    }

    /// A bound as a sentence writes it: a whole number without a point, such as 12, else as it is, such as 0.1.
    static func written(_ value: Double) -> String {
        value.rounded() == value ? "\(Int(value))" : "\(value)"
    }

    /// Names joined as a sentence lists them: "a, b and c".
    static func listed(_ names: [String]) -> String {
        guard let last = names.last, names.count > 1 else { return names.first ?? "" }
        return names.dropLast().joined(separator: ", ") + " and " + last
    }

    // MARK: What a screen can draw

    /// Every field the style sets that the screen's resting HUD would not draw as asked, by path, worded for the agent:
    /// mode `notch` on a screen without a notch, a field that the mode the HUD would have does not draw, as Settings
    /// shows each only for the modes it changes: an edge or a position beside the notch, a logo's size on a Simple panel,
    /// a glow on a Simple panel, which draws none, a range, feather or outward fade on a grid glow, a pitch, density,
    /// core or fade on the soft one, as Settings › Glow shows each row only for its styles, and a breath's depth
    /// under another effect; and a glow's number between the steps Settings moves it by. It is asked of a style
    /// reduced to what it changes (`reduced(placement:glow:)`), so a field that only repeats what is saved is never
    /// one.
    ///
    /// - placement, glow: what the screen has saved, which the style is drawn over.
    /// - screen: the screen as a sentence names it, such as "\"Display 2\"".
    func issues(placement: ScreenPlacement, glow saved: GlowSettings, hasNotch: Bool, screen: String) -> [PanelLayoutIssue] {
        let placed = applied(to: placement), drawn = applied(to: saved)
        let mode = placed.mode
        var issues: [PanelLayoutIssue] = []
        let docks = HUDMode.allCases.filter(\.isDocked).map { "\"\($0.agentName)\"" }.joined(separator: " or ")
        if self.mode == .notch, !hasNotch {
            issues.append(PanelLayoutIssue(path: "hud.mode", message: "\(screen) has no notch, so its HUD cannot be "
                                           + "\"notch\". It can be \(docks)."))
        }
        let modeNamed = "the HUD of \(screen) would be \"\(mode.agentName)\""
        for field in Field.allCases where field != .mode && value(field) != nil && !field.modes.contains(mode) {
            let modes = field.modes.map { "\"\($0.agentName)\"" }.joined(separator: " or ")
            issues.append(PanelLayoutIssue(path: "hud.\(field.rawValue)", message: "\(field.rawValue) sets a \(modes) HUD, and "
                                           + "\(modeNamed). Leave it out, or set mode to \(modes)."))
        }
        let grids = PanelLayout.quoted(GlowStyle.allCases.filter { $0 != .blur }.map(\.rawValue))
        for field in GlowField.allCases {
            guard let value = self.glow.value(field) else { continue }
            let path = "hud.glow.\(field.rawValue)"
            if mode == .simple {
                issues.append(PanelLayoutIssue(path: path, message: "A \"simple\" HUD draws no glow, and \(modeNamed)."))
            } else if !field.isDrawn(in: drawn.style) {
                let styles = drawn.style == .blur ? "a grid glow style, one of \(grids)," : "the \"blur\" glow style,"
                issues.append(PanelLayoutIssue(path: path, message: "\(field.rawValue) sets \(styles) and the glow of "
                                               + "\(screen) would be \"\(drawn.style.rawValue)\"."))
            } else if field == .amplitude, drawn.effect != .breathe {
                issues.append(PanelLayoutIssue(path: path, message: "amplitude sets the \"breathe\" effect, and the glow of "
                                               + "\(screen) would play \"\(drawn.effect.rawValue)\"."))
            } else if let step = field.step, let range = field.range, let number = value.numberValue,
                      !Self.keeps(number, to: step, from: range.lowerBound) {
                let steps = Self.numbered(range, step: step)
                issues.append(PanelLayoutIssue(path: path, message: "\(field.rawValue) is \(steps), as Settings sets "
                                               + "it, which \(Self.written(number)) is not."))
            }
        }
        return issues
    }

    /// The style as a screen saved with `placement` on a display with a notch would take it, as Settings' own change of
    /// mode does: a dock that leaves the notch for the top edge, where it has never been moved from the centre, starts
    /// at the top left, clear of the hardware. A position given as the saved centre is no position, so the style
    /// `get_hud` lists, sent back with a new mode, is placed as Settings places it.
    func placedClearOfTheNotch(from placement: ScreenPlacement, hasNotch: Bool) -> HUDStyle {
        guard hasNotch, mode?.isDocked == true, placement.mode == .notch, (position ?? placement.offset) == placement.offset,
              (edge ?? placement.edge) == .top, placement.offset == 0.5 else { return self }
        var style = self
        style.position = 0
        return style
    }
}
