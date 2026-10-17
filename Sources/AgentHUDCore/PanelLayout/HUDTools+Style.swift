import AgentHUDSupport
import Foundation

extension HUDTools {
    /// The style part of a proposal: the style fields of its `hud`, checked against the one screen they are for. A
    /// screen's style belongs to that screen alone, so the shared layout takes none, and only an attached screen has a
    /// resting HUD to draw one on. A proposal is whole: a field it leaves out is what the screen has saved. A field set
    /// to what the screen has saved changes nothing and is no issue, so the style `get_hud` lists can be sent back,
    /// and the style a waiting draft lists asks nothing new, so a draft's style read back can be sent back too, a
    /// version of the screen it brings back included. `reset` true asks for the screen's default style whole, as the
    /// HUD style's Reset to Default gives it, in place of every other style field. A transparency the screen keeps for
    /// a material (`HUDStyle.KeptTransparency`), which `get_hud` lists beside the style, sets nothing: it is taken
    /// where it repeats what the screen keeps now or would keep under the proposal, so the style listed can be sent
    /// back whole, and refused otherwise, since `transparency` is what sets the material's own.
    @MainActor
    struct StyleProposal {
        /// The attached screen the proposal is for; nil for the shared layout or a screen that is away.
        let target: PanelScreen?
        /// What the proposal asks, reduced to what it changes about what the screen has saved, with a dock leaving the
        /// notch placed clear of it as Settings places one.
        let reduced: HUDStyle
        /// Whether the proposal asks for the screen's default style (`hud.reset`).
        let resets: Bool
        /// The mode the screen's resting HUD would have under the proposal; nil without a screen to have one.
        let mode: HUDMode?
        /// Why the style cannot be done as asked, as sentences; what the reading would forgive is said by
        /// `PanelLayout.misreadings(of:)`.
        let reasons: [String]
        /// Whether a dock leaving the notch was placed clear of it at the top left, which the agent did not ask.
        let placedClear: Bool
        /// The glass style the proposal sets on a HUD that would stay solid, kept for when its material is glass, as
        /// Settings keeps it; nil where it sets none or the HUD would be glass.
        let keptForGlass: HUDGlassStyle?

        init(hud fields: [String: JSONValue], scope: PanelLayoutScope, among attached: [PanelScreen], tools: HUDTools) {
            let fieldsGiven = HUDStyle.names.filter { Self.sets($0, fields[$0]) }.map { "hud.\($0)" }
            resets = fields[HUDStyle.resetName] == .bool(true)
            let given = fieldsGiven + (resets ? ["hud.\(HUDStyle.resetName)"] : [])
            let style = HUDStyle(json: fields)
            let paths = HUDStyle.listed(given)
            let sets = given.count == 1 ? "sets" : "set"
            let kept = HUDStyle.KeptTransparency.allCases.filter { (fields[$0.rawValue] ?? .null) != .null }
            let keptPaths = HUDStyle.listed(kept.map { "hud.\($0.rawValue)" })
            let repeats = kept.count == 1 ? "repeats" : "repeat"
            guard case .screen(let key) = scope else {
                target = nil
                reduced = HUDStyle()
                mode = nil
                placedClear = false
                keptForGlass = nil
                let listed = "\(keptPaths) \(repeats) what get_hud lists for one screen's resting HUD, so the "
                    + "proposal needs that screen's key."
                let named = "\(paths) \(sets) the resting HUD of one screen, so the proposal needs that screen's key; "
                    + "with screen \"all\" the hud takes agents alone. \(HUDTools.attachedList(attached))"
                reasons = (given.isEmpty ? [] : [named]) + (kept.isEmpty ? [] : [listed])
                return
            }
            guard let screen = attached.first(where: { $0.key == key }) else {
                target = nil
                reduced = HUDStyle()
                mode = nil
                placedClear = false
                keptForGlass = nil
                let name = tools.name(of: key, among: attached)
                let listed = "\(keptPaths) \(repeats) what get_hud lists for an attached screen's resting HUD, and "
                    + "\(name) has none."
                reasons = (given.isEmpty ? [] : ["\(paths) \(sets) the resting HUD of an attached screen, and \(name) "
                    + "has none to draw it on."]) + (kept.isEmpty ? [] : [listed])
                return
            }
            let settings = tools.store.settings.settings
            let placement = settings.placement(on: key, hasNotch: screen.hasNotch), glow = settings.glow(on: key)
            let name = tools.name(of: key, among: attached)
            target = screen
            let asked = style.reduced(placement: placement, glow: glow)
            var found: [String]
            if resets {
                // The default style, as the store proposes it: what it changes, which the default always draws.
                reduced = HUDStyle(every: .default(hasNotch: screen.hasNotch), glow: settings.glow)
                    .reduced(placement: placement, glow: glow)
                placedClear = false
                found = fieldsGiven.isEmpty ? [] : ["hud.reset proposes the default style of the resting HUD of "
                    + "\(name) whole, so the hud takes no other style field with it: "
                    + "leave out \(HUDStyle.listed(fieldsGiven)), or leave out hud.\(HUDStyle.resetName)."]
            } else if let waiting = tools.layout.styleDraft(of: key), asked == waiting {
                reduced = asked
                placedClear = false
                found = []
            } else {
                let placed = style.placedClearOfTheNotch(from: placement, hasNotch: screen.hasNotch)
                let changes = placed.reduced(placement: placement, glow: glow)
                reduced = changes
                placedClear = placed.position != style.position
                found = changes.issues(placement: placement, glow: glow, hasNotch: screen.hasNotch,
                                       screen: name).map { "\($0.path): \($0.message)" }
            }
            // What the screen keeps now, a style draft included, and what it would keep under the proposal: a reset
            // restores the default placement whole.
            let now = tools.store.settings.effectivePlacement(on: key, hasNotch: screen.hasNotch)
            let after: ScreenPlacement = resets ? .default(hasNotch: screen.hasNotch) : reduced.applied(to: placement)
            for kept in kept {
                // A value of the wrong type or out of range is told by the reading (`HUDStyle.misreadings`).
                guard let number = fields[kept.rawValue]?.numberValue,
                      ScreenPlacement.transparencyRange(for: kept.material).contains(number),
                      number != kept.value(in: now), number != kept.value(in: after) else { continue }
                let keeps = HUDStyle.written(kept.value(in: now)), material = after.material.rawValue
                found.append("hud.\(kept.rawValue) is read only: \(name) keeps \(keeps) for \(kept.material.rawValue), "
                             + "as get_hud lists it, and hud.transparency sets the transparency of the material its "
                             + "HUD would have, \"\(material)\". Leave it out, or give it as listed.")
            }
            reasons = found
            mode = after.mode
            keptForGlass = !resets && after.material == .solid ? reduced.glassStyle : nil
        }

        /// Whether a hud field as sent sets anything: a value given as null is not given, and neither is a glow that has
        /// no property or only null ones.
        private static func sets(_ name: String, _ value: JSONValue?) -> Bool {
            guard let value, value != .null else { return false }
            guard name == "glow", let glow = value.objectValue else { return true }
            return glow.values.contains { $0 != .null }
        }
    }

    /// Shows a proposal's style as `key`'s style draft: the screen's default style for `hud.reset`, else what the
    /// proposal changes. A proposal's style is whole, so one that changes nothing, or a screen that is away, drops the
    /// style draft. The screen's layout, saved or as a draft, is left as it is.
    func proposeStyle(_ style: StyleProposal, for key: String) {
        guard let target = style.target else { return layout.discardStyle(key) }
        if style.resets {
            layout.proposeDefaultLook(for: key, hasNotch: target.hasNotch)
        } else {
            layout.proposeStyle(style.reduced, for: key, hasNotch: target.hasNotch)
        }
    }

    /// The layout in effect for a scope as an agent reads it, which it can send back as it is: for a screen, with the
    /// fields of its style draft in `hud` beside its agents.
    func layoutJSON(_ scope: PanelLayoutScope) -> JSONValue {
        var json = (try? JSONValue.from(effective(scope)))?.objectValue ?? [:]
        guard case .screen(let key) = scope, let style = layout.styleDraft(of: key) else { return .object(json) }
        json["hud"] = .object((json["hud"]?.objectValue ?? [:]).merging(style.json) { $1 })
        return .object(json)
    }

    /// One attached screen's resting HUD as it draws it now, its style draft included: every style field in an agent's
    /// words, each one as a proposal sets it, and beside them the transparency it keeps for each material
    /// (`HUDStyle.KeptTransparency`), which no proposal sets, so that an agent can tell what a change of material
    /// draws.
    func styleInEffect(of screen: PanelScreen) -> JSONValue {
        let settings = store.settings
        let placement = settings.effectivePlacement(on: screen.key, hasNotch: screen.hasNotch)
        var style = HUDStyle(every: placement, glow: settings.effectiveGlow(on: screen.key)).json
        for kept in HUDStyle.KeptTransparency.allCases { style[kept.rawValue] = .number(kept.value(in: placement)) }
        return .object(style)
    }

    /// The look one attached screen's resting HUD draws now, its panel and its cards with it, its style draft included:
    /// its `appearance`, `dark` or `light`, `system` drawn as macOS is set, its `material`, `solid` or `glass`, while
    /// it is glass the `glassStyle` the glass is made of, and its `transparency`, with a `note` where it draws other
    /// than its style asks: Reduce Transparency draws glass solid, with no glass style, and anything opaque.
    func lookInEffect(of screen: PanelScreen) -> JSONValue {
        let placement = store.settings.effectivePlacement(on: screen.key, hasNotch: screen.hasNotch)
        var look: [String: JSONValue] = [
            "appearance": .string(screen.drawsLight(placement) ? HUDAppearance.light.rawValue : HUDAppearance.dark.rawValue),
            "material": .string(screen.drawsGlass(placement) ? HUDMaterial.glass.rawValue : HUDMaterial.solid.rawValue),
        ]
        if screen.drawsGlass(placement) { look["glassStyle"] = .string(placement.glassStyle.rawValue) }
        look["transparency"] = .number(screen.drawsTransparency(placement))
        if screen.reducesTransparency, placement.material == .glass || placement.transparency > 0 {
            look["note"] = .string("This Mac has Reduce Transparency on, so the HUD is drawn solid and opaque.")
        }
        return .object(look)
    }
}
