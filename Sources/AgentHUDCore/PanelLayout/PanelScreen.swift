import Foundation

/// One attached display as the host tells the MCP tools of it (`HUDTools`). It is named by its role, never by its
/// system name, which can be a personal device name, and it says nothing of whose agents or accounts it shows.
public struct PanelScreen: Equatable, Sendable {
    /// The display's key, under which its placement, glow and own layout are kept.
    let key: String
    /// Its role in English: "Built-in display", "Main display", or "Display 2" and on, by its place among the displays.
    let name: String
    /// How it presents the HUD now, a style draft included.
    let mode: HUDMode
    let edge: HUDEdge
    let hasNotch: Bool
    /// The agents its resting HUD would show now without a list in its layout, by vendor, one for each mark, in the
    /// order it shows them: the glow's beside the notch, the marks of a Dynamic dock, the items of a Simple panel.
    let agents: [String]
    /// What it would show the same way in each other mode, for a proposal that changes its mode.
    let otherModes: [HUDMode: [String]]
    /// Whether macOS is set to light now, which a resting HUD whose appearance is `system` is drawn in. The Mac's, the
    /// same for every screen.
    let systemIsLight: Bool
    /// Whether Reduce Transparency is on now, which draws a resting HUD of glass solid. The Mac's, the same for every
    /// screen.
    let reducesTransparency: Bool
    /// Whether it is the display the person is at: the one the pointer is on, else the main display. An edit of the
    /// shared layout holds this display's panel open where it draws that layout, else the first display's that does
    /// (`PanelHolds`).
    let hasPointer: Bool

    /// - otherModes: the agents its resting HUD would show in each other mode; a mode left out is taken to show
    ///   `agents`.
    /// - systemIsLight, reducesTransparency: how the Mac is set now, which decides what a resting HUD that matches
    ///   macOS, or is made of glass, draws.
    /// - hasPointer: whether it is the display the person is at.
    public init(key: String, name: String, mode: HUDMode, edge: HUDEdge, hasNotch: Bool, agents: [String],
                otherModes: [HUDMode: [String]] = [:], systemIsLight: Bool, reducesTransparency: Bool,
                hasPointer: Bool = false) {
        self.key = key
        self.name = name
        self.mode = mode
        self.edge = edge
        self.hasNotch = hasNotch
        self.agents = agents
        self.otherModes = otherModes
        self.systemIsLight = systemIsLight
        self.reducesTransparency = reducesTransparency
        self.hasPointer = hasPointer
    }

    /// The mode as Agent HUD names it to a person: notch, dynamic or simple.
    var modeName: String { mode.agentName }

    /// The agents its resting HUD would show in `mode` now.
    func agents(in mode: HUDMode) -> [String] {
        mode == self.mode ? agents : otherModes[mode] ?? agents
    }

    /// Whether its resting HUD, drawing `placement`, is drawn light now (`ScreenPlacement.drawsLight(systemIsLight:)`).
    func drawsLight(_ placement: ScreenPlacement) -> Bool {
        placement.drawsLight(systemIsLight: systemIsLight)
    }

    /// Whether its resting HUD, drawing `placement`, is drawn as glass now
    /// (`ScreenPlacement.drawsGlass(reducesTransparency:)`).
    func drawsGlass(_ placement: ScreenPlacement) -> Bool {
        placement.drawsGlass(reducesTransparency: reducesTransparency)
    }

    /// How much of the desktop its resting HUD, drawing `placement`, lets through now
    /// (`ScreenPlacement.drawsTransparency(reducesTransparency:)`).
    func drawsTransparency(_ placement: ScreenPlacement) -> Double {
        placement.drawsTransparency(reducesTransparency: reducesTransparency)
    }
}
