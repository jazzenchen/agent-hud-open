import AgentHUDSupport
import AppKit
import SwiftUI
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// A screen's resting HUD drawn off screen for an agent, as `get_hud_screenshot` returns it with show "hud": the island
/// with its glow, a Dynamic dock's marks over their backdrop, a Simple panel, each as the screen draws it now with any
/// style draft, read back from its pixels.
@MainActor
final class HUDPictureRendererTests: XCTestCase {
    private let key = "screen:picture"

    /// The sample's store over settings of its own, made when a test first reaches for either.
    private lazy var store: UsageStore = {
        _ = NSApplication.shared
        let suite = "app.agenthud.tests.hud-picture.\(UUID().uuidString)"
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: UserDefaults(suiteName: suite)!, defaultAgents: DemoData.agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, clock: { DemoClock.now })
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: DemoClock.now))
        return store
    }()

    private var layout: PanelLayoutStore { store.settings.panelLayout }
    private let notch = ScreenMetrics(menuBar: 37, notch: CGSize(width: 185, height: 32))

    private func renderer(hasNotch: Bool = true) -> HUDPictureRenderer {
        HUDPictureRenderer(store: store, screen: key, hasNotch: hasNotch, look: .standard, metrics: hasNotch ? notch : .fallback)
    }

    private func bitmap(_ picture: HUDPicture, file: StaticString = #filePath, line: UInt = #line) throws -> NSBitmapImageRep {
        try XCTUnwrap(NSBitmapImageRep(data: picture.png), "the picture is a PNG", file: file, line: line)
    }

    /// How far a pixel's colour is from a grey: a glow's colours are far, the backdrop and the black island are not.
    private func colourfulness(_ rep: NSBitmapImageRep, x: Int, y: Int) -> CGFloat {
        guard let colour = rep.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else { return 0 }
        let parts = [colour.redComponent, colour.greenComponent, colour.blueComponent]
        return (parts.max() ?? 0) - (parts.min() ?? 0)
    }

    /// Whether any pixel of a band of rows is far from grey.
    private func hasColour(_ rep: NSBitmapImageRep, rows: Range<Int>) -> Bool {
        for y in rows where y >= 0 && y < rep.pixelsHigh {
            for x in stride(from: 0, to: rep.pixelsWide, by: 2) where colourfulness(rep, x: x, y: y) > 0.25 { return true }
        }
        return false
    }

    // MARK: Beside the notch

    /// Beside the notch the picture is the island with its glow, drawn at twice its size; the glow is in it, read back
    /// from the colours under the island.
    func testTheNotchIsDrawnWithItsGlow() throws {
        let renderer = renderer()
        XCTAssertEqual(renderer.placement.mode, .notch, "a screen with a notch keeps its island")
        let picture = try XCTUnwrap(renderer.picture())
        XCTAssertTrue(picture.drawsGlow)
        XCTAssertEqual(picture.scale, 2)
        let rep = try bitmap(picture)
        XCTAssertEqual(rep.pixelsWide, Int((picture.width * 2).rounded()))
        XCTAssertEqual(rep.pixelsHigh, Int((picture.height * 2).rounded()))
        XCTAssertGreaterThan(picture.width, 185, "the glow reaches past the island")
        // The island hangs from the picture's top edge, 32 points tall; the glow's colours are just below its foot.
        XCTAssertTrue(hasColour(rep, rows: 64..<80), "the glow is drawn off screen")
        // The island itself is black in the middle, from the very top.
        for y in [0, 32] {
            let island = rep.colorAt(x: rep.pixelsWide / 2, y: y)?.usingColorSpace(.deviceRGB)
            XCTAssertLessThan(island?.brightnessComponent ?? 1, 0.1, "\(y)")
        }
    }

    /// A style draft's glow is in the picture: another style and a wider range make another, larger picture.
    func testAGlowDraftIsDrawn() throws {
        let saved = try XCTUnwrap(renderer().picture())
        XCTAssertTrue(layout.proposeStyle(HUDStyle(glow: HUDStyle.Glow(style: .blur, range: 30)), for: key, hasNotch: true))
        let wider = try XCTUnwrap(renderer().picture())
        XCTAssertGreaterThan(wider.width, saved.width, "a wider range reaches farther")
        XCTAssertTrue(layout.proposeStyle(HUDStyle(glow: HUDStyle.Glow(style: .dots)), for: key, hasNotch: true))
        let dots = try XCTUnwrap(renderer().picture())
        XCTAssertNotEqual(dots.png, saved.png)
        XCTAssertTrue(hasColour(try bitmap(dots), rows: 64..<Int(dots.height * 2)))
    }

    // MARK: Docks

    /// A Dynamic dock is its marks over their backdrop; a draft's larger logos make it longer and taller.
    func testADocksLogosFollowTheDraft() throws {
        let saved = try XCTUnwrap(renderer(hasNotch: false).picture())
        XCTAssertEqual(renderer(hasNotch: false).placement.mode, .logos, "a screen without a notch is a dock")
        XCTAssertTrue(saved.drawsGlow)
        XCTAssertTrue(hasColour(try bitmap(saved), rows: 0..<Int(saved.height * 2)), "the marks and the backdrop are drawn")
        layout.proposeStyle(HUDStyle(logoSize: 36), for: key, hasNotch: false)
        let larger = try XCTUnwrap(renderer(hasNotch: false).picture())
        XCTAssertGreaterThan(larger.width, saved.width)
        XCTAssertGreaterThanOrEqual(larger.height, saved.height)
    }

    /// A Simple panel draws no glow, and a draft that turns a notch into one is drawn as one, along a top edge
    /// whichever edge it is parked on.
    func testADraftThatTurnsTheScreenSimpleIsDrawnAsOne() throws {
        layout.proposeStyle(HUDStyle(mode: .simple, edge: .left), for: key, hasNotch: true)
        let renderer = renderer()
        XCTAssertEqual(renderer.placement.mode, .simple)
        let picture = try XCTUnwrap(renderer.picture())
        XCTAssertFalse(picture.drawsGlow)
        let items = ScreenAgents(store: store, listed: [], mode: .simple).meterItems
        var top = renderer.placement
        top.edge = .top
        let strip = SimpleMeterConfig(items: items, placement: top).stripSize
        XCTAssertEqual(picture.width, Double(strip.width + 2 * HUDPictureRenderer.margin), accuracy: 1, "laid along a top edge")
        XCTAssertEqual(picture.height, Double(strip.height + 2 * HUDPictureRenderer.margin), accuracy: 1)
    }

    /// A Simple panel's marks in the picture follow Bot logos, as the dock's do and as the panel on screen does, and the
    /// panel is the same size either way. Whether a bot is out also depends on Reduce Motion, which this does not read.
    func testASimplePanelsMarksFollowBotLogos() throws {
        layout.proposeStyle(HUDStyle(mode: .simple), for: key, hasNotch: true)
        var sizes: [CGSize] = []
        for on in [true, false] {
            store.settings.update { $0.botLogos = on }
            let renderer = renderer()
            XCTAssertEqual(renderer.placement.mode, .simple, "a switch of the whole app leaves the style draft")
            XCTAssertEqual(renderer.simpleMeter.bots, on)
            let dock = LogoQueueConfig(items: [], placement: renderer.placement, settings: store.settings.settings)
            XCTAssertEqual(renderer.simpleMeter.bots, dock.bots, "as a dock's picture follows it")
            let picture = try XCTUnwrap(renderer.picture())
            sizes.append(CGSize(width: picture.width, height: picture.height))
        }
        XCTAssertEqual(sizes.first, sizes.last)
    }

    /// The picture is drawn in the screen's look: over the same backdrop, a Simple panel's own fill is black in the
    /// standard look and the light surface in the light one, and the panel is the same size in both.
    func testASimplePanelIsDrawnInTheScreensLook() throws {
        layout.proposeStyle(HUDStyle(mode: .simple), for: key, hasNotch: true)
        let standard = try simplePanel(in: .standard), light = try simplePanel(in: HUDLook(appearance: .light))
        XCTAssertLessThan(standard.fill, 0.1, "black as ever")
        XCTAssertGreaterThan(light.fill, 0.8, "the light surface")
        XCTAssertEqual(light.picture.width, standard.picture.width)
        XCTAssertEqual(light.picture.height, standard.picture.height)
    }

    /// Nothing is behind a picture to show through glass: a panel of glass is drawn as its stand-in over the backdrop,
    /// 0.805 of its look's own surface at the standard Transparency, 50 %, which clear glass draws at 0.3, darker than
    /// the backdrop for dark glass and lighter for light glass, short of the solid fill. The stand-in thins as the
    /// dimming beneath the glass does: the surface whole at 0, and at the material's top, 100 % drawn whole, the third
    /// of it a picture always keeps, dark glass still darker than the backdrop and light glass still lighter. Every glass
    /// style is the same stand-in at the transparency it draws, so a picture shows them alike but for that.
    func testAGlassPanelIsDrawnAsItsStandIn() throws {
        layout.proposeStyle(HUDStyle(mode: .simple), for: key, hasNotch: true)
        func glass(_ appearance: HUDLook.Appearance, _ transparency: Double, style: HUDGlassStyle = .clear) throws
            -> (picture: HUDPicture, fill: CGFloat) {
            try simplePanel(in: HUDLook(appearance: appearance, material: .glass, transparency: transparency,
                                        glassStyle: style))
        }
        let standard = ScreenPlacement(material: .glass).drawsTransparency(reducesTransparency: false)
        XCTAssertEqual(standard, 0.3, accuracy: 0.000_001)
        let dark = try glass(.dark, standard), light = try glass(.light, standard)
        let solid = try simplePanel(in: HUDLook(appearance: .light))
        // The backdrop as the picture has it, in its margin.
        let margin = try bitmap(dark.picture).colorAt(x: 4, y: 4)?.usingColorSpace(.deviceRGB)
        let backdrop = margin?.brightnessComponent ?? 0
        XCTAssertGreaterThan(dark.fill, 0.01, "the backdrop shows through dark glass")
        XCTAssertLessThan(dark.fill, backdrop - 0.1, "darker than the backdrop")
        XCTAssertGreaterThan(light.fill, 0.5, "lighter than the backdrop")
        XCTAssertLessThan(light.fill, solid.fill - 0.1, "short of the light surface itself")
        XCTAssertEqual(light.picture.width, solid.picture.width)
        XCTAssertEqual(light.picture.height, solid.picture.height)
        for style in HUDGlassStyle.allCases {
            XCTAssertEqual(try glass(.dark, standard, style: style).fill, dark.fill, accuracy: 0.005, "\(style)")
            XCTAssertEqual(try glass(.light, standard, style: style).fill, light.fill, accuracy: 0.005, "\(style)")
        }

        XCTAssertEqual(try glass(.light, 0).fill, solid.fill, accuracy: 0.02, "the light surface whole at 0")
        XCTAssertLessThan(try glass(.dark, 0).fill, 0.02, "black whole at 0")
        let top = ScreenPlacement(material: .glass, glassStyle: .material, glassTransparency: 1)
            .drawsTransparency(reducesTransparency: false)
        XCTAssertEqual(top, 1)
        let darkBare = try glass(.dark, top, style: .material).fill, lightBare = try glass(.light, top, style: .material).fill
        XCTAssertGreaterThan(darkBare, dark.fill + 0.03, "less of dark glass")
        XCTAssertLessThan(darkBare, backdrop - 0.03, "still darker than the backdrop")
        XCTAssertLessThan(lightBare, light.fill - 0.15, "less of light glass")
        XCTAssertGreaterThan(lightBare, backdrop + 0.15, "still lighter than the backdrop")
    }

    /// The Simple panel's picture in `look`, and the brightness most of its strip has: its fill, which the rings and
    /// figures leave showing.
    private func simplePanel(in look: HUDLook) throws -> (picture: HUDPicture, fill: CGFloat) {
        let items = ScreenAgents(store: store, listed: [], mode: .simple).meterItems
        var top = renderer().placement
        top.edge = .top
        let strip = SimpleMeterConfig(items: items, placement: top).stripSize
        let picture = try XCTUnwrap(HUDPictureRenderer(store: store, screen: key, hasNotch: true, look: look, metrics: notch).picture())
        let rep = try bitmap(picture)
        let margin = Int(HUDPictureRenderer.margin * 2), cap = Int(strip.height)
        var levels: [CGFloat] = []
        for y in stride(from: margin + 8, to: margin + Int(strip.height * 2) - 8, by: 2) {
            for x in stride(from: margin + cap, to: margin + Int(strip.width * 2) - cap, by: 2) {
                levels.append(rep.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB)?.brightnessComponent ?? 0)
            }
        }
        return (picture, levels.sorted()[levels.count / 2])
    }

    // MARK: To the tools

    /// The tools hand the picture over with what it shows, for an attached screen only.
    func testTheToolsReturnTheRestingHUD() throws {
        let screen = PanelScreen(key: key, name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                 agents: ScreenAgents.available(store: store, mode: .notch),
                                 systemIsLight: false, reducesTransparency: false)
        let renderer = renderer()
        let tools = HUDTools(store: store, screens: { [screen] }, hud: { _ in renderer.picture() })
        layout.proposeStyle(HUDStyle(glow: HUDStyle.Glow(effect: .ripple)), for: key, hasNotch: true)
        guard case .result(let said, let images) = tools.answer(MCPToolRequest(
            tool: MCPTools.getHUDScreenshot, arguments: ["screen": .string(key), "show": .string("hud")])) else {
            return XCTFail("the resting HUD was refused")
        }
        XCTAssertEqual(images.count, 1)
        XCTAssertEqual(images.first?.mimeType, "image/png")
        XCTAssertEqual(said["show"], .string("hud"))
        XCTAssertEqual(said["mode"], .string("notch"))
        XCTAssertEqual(said["unsaved"], .bool(true))
        XCTAssertEqual(said["glow"], .bool(true))
        let note = try XCTUnwrap(said["note"]?.stringValue)
        XCTAssertTrue(note.hasPrefix("The resting HUD of \"Built-in display\" as it looks now, with its unsaved style draft: "
                                     + "the island at its notch"), note)
        XCTAssertTrue(note.contains(", with its glow."), note)
        guard case .failure(let refusal) = tools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot,
                                                                       arguments: ["show": .string("hud")])) else {
            return XCTFail("the shared layout has no resting HUD")
        }
        XCTAssertTrue(refusal.hasPrefix("show \"hud\" draws one screen's resting HUD, so it needs that screen's key."), refusal)
    }
}
