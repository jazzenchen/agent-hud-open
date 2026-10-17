import AgentHUDSupport
import Foundation

/// What `get_hud_screenshot` tells an agent: a picture of a screen's hover panel, of one agent's own panel there, or
/// of its resting HUD.
extension HUDTools {
    // MARK: The panel

    /// The picture with what it shows: which screen and layout and its revision, the panel's size in points, whether it
    /// scrolls on its display, and the pixels a point it is drawn at, with a note that says the same in words, and that
    /// glass is a stand-in where the panel is drawn in it.
    func image(_ scope: PanelLayoutScope, among attached: [PanelScreen]) -> MCPToolReply {
        guard let picture else {
            return .failure("This copy of Agent HUD cannot draw the panel, so there is no picture of it here. "
                            + "get_hud reads the same panel as data.")
        }
        guard let drawn = picture(scope) else {
            return .failure("The panel's picture could not be drawn. Try again, or read the panel with get_hud.")
        }
        var note: String
        var display = "The main display"
        switch scope {
        case .shared:
            note = "The hover panel as \(layout.hasDraft ? "the unsaved draft" : "the saved layout") draws it"
        case .screen(let key):
            note = "The hover panel on \(name(of: key, among: attached)) as \(drawing(key)) draws it"
            if attached.contains(where: { $0.key == key }) { display = "That display" }
        }
        // The shared layout is drawn in the main display's look, the first attached.
        let looked = scope.screenKey.flatMap { key in attached.first { $0.key == key } } ?? attached.first
        let glass = looked.map { $0.drawsGlass(store.settings.effectivePlacement(on: $0.key, hasNotch: $0.hasNotch)) } ?? false
        note += barNote(scope.screenKey) + measures(of: drawn, on: display, glass: glass)
        return .result(fields(of: drawn, on: scope, note: note), images: [MCPImage(mimeType: "image/png", data: drawn.png)])
    }

    /// What a panel's picture is said to be beside its note: which screen and layout and its revision, whether a draft
    /// waits, its size in points, whether it scrolls on its display, and the pixels a point it is drawn at.
    private func fields(of drawn: PanelPicture, on scope: PanelLayoutScope, note: String) -> [String: JSONValue] {
        [
            "screen": .string(Self.name(of: scope)), "revision": .integer(revision), "unsaved": .bool(unsaved(scope)),
            "width": .integer(Int64(drawn.width.rounded())), "height": .integer(Int64(drawn.fit.height.rounded())),
            "scrolls": .bool(drawn.fit.scrolls), "pixelsPerPoint": .number(Self.hundredths(drawn.scale)), "note": .string(note),
        ]
    }

    /// What a panel's note says after what it draws: its size, what is masked, whether `display` lets it open whole,
    /// the scale it is drawn at where that is less than the usual, and that glass is a stand-in where it is drawn in it.
    private func measures(of drawn: PanelPicture, on display: String, glass: Bool) -> String {
        let fit = drawn.fit
        var note = ", \(Self.points(drawn.width)) by \(Self.points(fit.height)) points, with account names masked, session "
            + "titles left out and any reading's reason or client's notice drawn as grey bars."
        if fit.scrolls {
            note += " \(display) opens it at most \(Self.points(fit.maximumHeight)) points tall, so a person scrolls to see "
                + "the rest; the picture shows all of it."
        }
        if drawn.isScaledDown {
            note += " It is drawn at \(Self.hundredths(drawn.scale)) pixels a point instead of \(Self.points(PanelPicture.pointScale)) "
                + "to stay within \(PanelPicture.pixelLimit) pixels tall."
        }
        if glass { note += " " + Self.glassStandIn }
        return note
    }

    /// What the bar above the panel's footer says waits unsaved beside the layout, which counts in the height, as the note
    /// names it after the layout the panel draws: a screen's style, and the app's settings, which every panel's bar shows.
    /// Nothing where nothing but a layout waits, which the layout named says, or nothing waits.
    /// - key: the screen's; nil for the shared layout's panel.
    private func barNote(_ key: String?) -> String {
        let style = key.map(layout.hasStyleDraft) ?? false, app = layout.appDraft != nil
        let laid = key.map { layout.draftScope(on: $0) != nil } ?? layout.hasDraft
        switch (style, app) {
        case (false, false):
            return ""
        case (true, false):
            return laid ? ", with the bar above its footer for the unsaved layout and its resting HUD's unsaved style"
                : ", with the bar above its footer that says its resting HUD's style is unsaved"
        case (true, true):
            return laid ? ", with the bar above its footer for the unsaved layout, its resting HUD's unsaved style and "
                + "the app's unsaved settings"
                : ", with the bar above its footer that says its resting HUD's style and the app's settings are unsaved"
        case (false, true):
            return laid ? ", with the bar above its footer for the unsaved layout and the app's unsaved settings"
                : ", with the bar above its footer that says the app's settings are unsaved"
        }
    }

    /// Which layout a screen's panel draws, in words.
    private func drawing(_ key: String) -> String {
        switch layout.draft(of: key) {
        case .own?: return "its own unsaved draft"
        case .shared?: return "the shared layout, in an unsaved draft that has it use the shared layout"
        case nil: break
        }
        if layout.ownLayout(of: key) != nil { return "its own saved layout" }
        return layout.hasDraft ? "the shared layout's unsaved draft" : "the saved shared layout"
    }

    /// What a picture says of glass: it is drawn off screen, where no desktop is behind it to show through, so every
    /// glass style is drawn as the same stand-in, at the transparency the style draws, and a picture shows no
    /// difference between them but that.
    static let glassStandIn = "Its glass is drawn as a stand-in, the same for clear, regular and material glass but "
        + "for the transparency each draws: a picture is drawn off screen, with no desktop behind it to show through."

    // MARK: One agent's own panel

    /// A picture of one agent's own panel on an attached screen whose dock separates its agents, the card a click on the
    /// agent's mark opens beside it, at the screen's `agentPanelWidth`, as the layout in effect there draws it, a draft
    /// and its bar included: what the whole panel's picture says, with the agent as the screen lists it. The agent is
    /// named as `get_hud` names it: as the screen's `shows` lists it, matched by the mark it is drawn with, or by its
    /// block's key, an API balance's standing for the vendor its accounts name.
    func agentImage(_ scope: PanelLayoutScope, agent given: JSONValue, among attached: [PanelScreen]) -> MCPToolReply {
        guard let asked = given.stringValue, !asked.isEmpty else {
            return .failure("agent is an agent's name from that screen's shows in get_hud, or its block's key.")
        }
        guard case .screen(let key) = scope else {
            return .failure("agent draws one agent's own panel on a screen whose dock separates its agents, so it needs "
                            + "that screen's key. " + Self.attachedList(attached))
        }
        guard let screen = attached.first(where: { $0.key == key }) else {
            return .failure("\(name(of: key, among: attached, starting: true)) has no agent's own panel to draw. "
                            + Self.attachedList(attached))
        }
        let placement = store.settings.effectivePlacement(on: key, hasNotch: screen.hasNotch)
        guard placement.mode.isDocked, placement.separatesAgents else {
            return .failure(Self.oneSharedPanel(on: screen, placement))
        }
        let shown = HUDAgentOrder(listed: effective(scope).hud.agents, available: screen.agents(in: placement.mode))
            .arrange(screen.agents(in: placement.mode)) { $0 }
        // A quota block's key is its vendor; an API balance's is its provider's, whose accounts name their vendor.
        let vendor = store.enabledBilling.first { $0.panelBlockKey == asked }?.vendor ?? asked
        guard let agent = shown.first(where: { VendorMark.key($0) == VendorMark.key(vendor) }) else {
            let offered = shown.isEmpty ? "It shows no agent now."
                : "It shows: \(PanelLayout.quoted(VendorMark.merged(shown)))."
            return .failure("\"\(asked)\" has no mark on the resting HUD of \"\(screen.name)\" now, so it has no panel of its "
                            + "own there. \(offered)")
        }
        guard let agentPicture else {
            return .failure("This copy of Agent HUD cannot draw an agent's own panel, so there is no picture of it here. "
                            + "get_hud reads the panel as data.")
        }
        guard let drawn = agentPicture(key, agent) else {
            return .failure("The agent's own panel could not be drawn. Try again, or read the panel with get_hud.")
        }
        var note = "\"\(agent)\"'s own panel on \"\(screen.name)\", the card a click on its mark opens beside it, as "
            + "\(drawing(key)) draws it" + barNote(key)
        note += measures(of: drawn, on: "That display", glass: screen.drawsGlass(placement))
        let saved = store.settings.settings.placement(on: key, hasNotch: screen.hasNotch)
        if !(saved.mode.isDocked && saved.separatesAgents) {
            note += " The dock separates its agents only in the unsaved style draft."
        }
        var told = fields(of: drawn, on: scope, note: note)
        told["agent"] = .string(agent)
        return .result(told, images: [MCPImage(mimeType: "image/png", data: drawn.png)])
    }

    /// Why a screen's resting HUD gives no agent a panel of its own, and how it would: a Dynamic dock or a Simple panel
    /// with `separateAgents` on, which a proposal for that screen can draw as a style draft.
    private static func oneSharedPanel(on screen: PanelScreen, _ placement: ScreenPlacement) -> String {
        guard placement.mode.isDocked else {
            return "The resting HUD of \"\(screen.name)\" is a notch HUD, whose panel is every agent's, so no agent has one "
                + "of its own there. A Dynamic dock or a Simple panel that separates its agents gives each its own: propose "
                + "hud.mode \"dynamic\" or \"simple\" with hud.separateAgents true for that screen, then draw it."
        }
        return "The resting HUD of \"\(screen.name)\", a \"\(placement.mode.agentName)\" HUD, does not separate its agents, "
            + "so its panel is every agent's and no agent has one of its own there. Propose hud.separateAgents true for that "
            + "screen, then draw it."
    }

    // MARK: The resting HUD

    /// A picture of one attached screen's resting HUD as it looks now, its style draft included, drawn off screen without
    /// opening anything: which screen, its mode and edge, the appearance and material it is drawn in, whether a style
    /// draft waits, whether the glow is in the picture, its size in points and the pixels a point it is drawn at, with a
    /// note that says the same in words, and that glass is a stand-in.
    func hudImage(_ scope: PanelLayoutScope, among attached: [PanelScreen]) -> MCPToolReply {
        guard case .screen(let key) = scope else {
            return .failure("show \"hud\" draws one screen's resting HUD, so it needs that screen's key. "
                            + Self.attachedList(attached))
        }
        guard let screen = attached.first(where: { $0.key == key }) else {
            return .failure("\(name(of: key, among: attached, starting: true)) has no resting HUD to draw. "
                            + Self.attachedList(attached))
        }
        guard let hud else {
            return .failure("This copy of Agent HUD cannot draw a resting HUD, so there is no picture of it here. "
                            + "get_hud reads its style as data.")
        }
        guard let drawn = hud(key) else {
            return .failure("The resting HUD's picture could not be drawn. Try again, or read its style with get_hud.")
        }
        let placement = store.settings.effectivePlacement(on: key, hasNotch: screen.hasNotch)
        let draft = layout.hasStyleDraft(key)
        // At a hardware notch the island at rest is black whatever the look; the look shows on what it opens into.
        let atNotch = placement.mode == .notch && screen.hasNotch
        let lookAppearance: HUDAppearance = screen.drawsLight(placement) ? .light : .dark
        let lookMaterial: HUDMaterial = screen.drawsGlass(placement) ? .glass : .solid
        let appearance: HUDAppearance = atNotch ? .dark : lookAppearance
        let material: HUDMaterial = atNotch ? .solid : lookMaterial
        var note = "The resting HUD of \"\(screen.name)\" as it looks now, " + (draft ? "with its unsaved style draft" : "as saved")
        switch placement.mode {
        case .notch:
            note += screen.hasNotch ? ": the island at its notch" : ": the bar that stands in for a notch at its top centre"
        case .logos, .simple:
            note += ": a \"\(placement.mode.agentName)\" HUD on its \(placement.edge.rawValue) edge"
            if placement.edge != .top { note += ", drawn here along a top edge as Settings previews it" }
        }
        if atNotch {
            note += ", black at rest, as the notch is; a reminder's wings are dark and \(lookMaterial.rawValue), and the "
                + "panel and the cards it opens into \(lookAppearance.rawValue) and \(lookMaterial.rawValue)"
        } else {
            note += ", \(appearance.rawValue) and \(material.rawValue)"
        }
        note += ", on a dark backdrop, \(Self.points(drawn.width)) by \(Self.points(drawn.height)) points"
        if placement.mode == .simple {
            note += "; a simple HUD draws no glow."
        } else {
            note += drawn.drawsGlow ? ", with its glow." : ", without its glow, which only its window on the display draws."
        }
        if material == .glass { note += " " + Self.glassStandIn }
        note += " It shows marks and figures, and no account or session."
        let result: [String: JSONValue] = [
            "screen": .string(key), "show": .string("hud"), "revision": .integer(revision), "unsaved": .bool(draft),
            "mode": .string(placement.mode.agentName), "edge": .string(placement.mode.isDocked ? placement.edge.rawValue : "top"),
            "appearance": .string(appearance.rawValue), "material": .string(material.rawValue),
            "glow": .bool(drawn.drawsGlow), "width": .integer(Int64(drawn.width.rounded())),
            "height": .integer(Int64(drawn.height.rounded())), "pixelsPerPoint": .number(Self.hundredths(drawn.scale)),
            "note": .string(note),
        ]
        return .result(result, images: [MCPImage(mimeType: "image/png", data: drawn.png)])
    }
}
