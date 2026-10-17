import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tools tell an agent about the panel's look: its picture, which the application draws, and its height,
/// which `get_hud` reads beside the blocks. The drawing and the measuring are the host's, so here they are fakes
/// that answer with what a test gives them.
final class HUDToolsPictureTests: HUDToolsCase {
    private let png = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02])
    private var drawn = 0
    private var measured = 0
    /// The scopes the renderer and the measurer were asked for, in order.
    private var scopes: [PanelLayoutScope] = []

    /// The tools with a renderer that draws `picture` and a measurer that finds `fit`, counting how often each runs.
    private func tools(picture: PanelPicture?, fit: PanelFit? = nil) -> HUDTools {
        HUDTools(store: store, launch: 0, picture: { [unowned self] scope in
            drawn += 1
            scopes.append(scope)
            return picture
        }, fit: { [unowned self] scope in
            measured += 1
            scopes.append(scope)
            return fit
        })
    }

    private func picture(height: Double, maximum: Double = 902, scale: Double = 2) -> PanelPicture {
        PanelPicture(png: png, width: 540, fit: PanelFit(height: height, maximumHeight: maximum), scale: scale)
    }

    private func answer(_ tools: HUDTools, _ arguments: [String: JSONValue] = [:]) throws -> (result: [String: JSONValue], images: [MCPImage]) {
        guard case .result(let result, let images) = tools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: arguments)) else {
            XCTFail("get_hud_screenshot was refused")
            throw CancellationError()
        }
        return (result, images)
    }

    func testThePictureComesWithWhatItShows() throws {
        let (result, images) = try answer(tools(picture: picture(height: 741)))
        XCTAssertEqual(images, [MCPImage(mimeType: "image/png", data: png)])
        XCTAssertEqual(result["revision"], .integer(0))
        XCTAssertEqual(result["unsaved"], .bool(false))
        XCTAssertEqual(result["width"], .integer(540))
        XCTAssertEqual(result["height"], .integer(741))
        XCTAssertEqual(result["scrolls"], .bool(false))
        XCTAssertEqual(result["pixelsPerPoint"], .number(2))
        XCTAssertEqual(result["note"], .string("The hover panel as the saved layout draws it, 540 by 741 points, with account "
                                               + "names masked, session titles left out and any reading's reason or client's "
                                               + "notice drawn as grey bars."))
        XCTAssertEqual(result["screen"], .string("all"), "without a screen, the shared layout")
        XCTAssertEqual(result.keys.sorted(), ["height", "note", "pixelsPerPoint", "revision", "screen", "scrolls", "unsaved", "width"])
        XCTAssertEqual(drawn, 1)
        XCTAssertEqual(scopes, [.shared])
        XCTAssertEqual(measured, 0, "the picture carries its own measure")
    }

    func testAPictureOfADraftSaysSoAndChangesNothing() throws {
        layout.propose(PanelLayout(order: ["Codex"]))
        let before = (layout.revision, layout.draft)
        changes = 0
        let (result, _) = try answer(tools(picture: picture(height: 741)))
        XCTAssertEqual(result["unsaved"], .bool(true))
        XCTAssertEqual(result["revision"], .integer(Int64(layout.revision)))
        XCTAssertTrue(result["note"]?.stringValue?.hasPrefix("The hover panel as the unsaved draft draws it") == true)
        XCTAssertEqual(layout.revision, before.0)
        XCTAssertEqual(layout.draft, before.1)
        XCTAssertEqual(changes, 0)
    }

    func testAPanelThatScrollsOrIsDrawnSmallerSaysSo() throws {
        let tall = try answer(tools(picture: picture(height: 1_500, maximum: 1_360, scale: 1.6))).result
        XCTAssertEqual(tall["scrolls"], .bool(true))
        XCTAssertEqual(tall["pixelsPerPoint"], .number(1.6))
        XCTAssertEqual(tall["note"], .string("The hover panel as the saved layout draws it, 540 by 1500 points, with account names "
                                             + "masked, session titles left out and any reading's reason or client's notice drawn as grey "
                                             + "bars. The main display opens it at most 1360 points tall, so a person scrolls to see the "
                                             + "rest; the picture shows all of it. It is drawn at 1.6 pixels a point instead of 2 to stay "
                                             + "within 2400 pixels tall."))
        let fits = try answer(tools(picture: picture(height: 1_300, maximum: 1_360, scale: PanelPicture.scale(forHeight: 1_300)))).result
        XCTAssertEqual(fits["scrolls"], .bool(false))
        XCTAssertEqual(fits["pixelsPerPoint"], .number(1.84), "to the hundredth below what was drawn")
        XCTAssertTrue(fits["note"]?.stringValue?.contains("1.84 pixels a point instead of 2") == true)
        XCTAssertFalse(fits["note"]?.stringValue?.contains("scrolls") == true)
    }

    func testThePictureToolTakesOnlyAScreenWhatToShowAndAnAgent() {
        let tools = tools(picture: picture(height: 741))
        XCTAssertEqual(tools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: ["scale": .integer(1), "block": .string("Claude")])),
                       .failure("get_hud_screenshot takes only screen, show and agent. Unknown: \"block\", \"scale\"."))
        XCTAssertEqual(tools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: ["show": .string("glow")])),
                       .failure("show is \"panel\", the default, or \"hud\"."))
        XCTAssertEqual(drawn, 0, "nothing is drawn for a refused call")
        guard case .result = tools.answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot, arguments: ["show": .string("panel")])) else {
            return XCTFail("show panel is the panel")
        }
        XCTAssertEqual(drawn, 1)
    }

    /// A screen whose resting HUD has an unsaved style shows the bar that says so in its panel, which the note names
    /// beside the saved layout it draws.
    func testAPictureOfAScreenWithAStyleDraftSaysWhyItIsUnsaved() throws {
        layout.proposeStyle(HUDStyle(logoSize: 30), for: "SCREEN-A", hasNotch: false)
        let (result, _) = try answer(tools(picture: picture(height: 785)), ["screen": .string("SCREEN-A")])
        XCTAssertEqual(result["unsaved"], .bool(true))
        XCTAssertTrue(result["note"]?.stringValue?.hasPrefix("The hover panel on the screen \"SCREEN-A\", which is not attached "
                                                             + "now, as the saved shared layout draws it, with the bar above its "
                                                             + "footer that says its resting HUD's style is unsaved, 540 by 785")
                      == true, "\(result["note"] ?? .null)")
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "SCREEN-A")
        let (both, _) = try answer(tools(picture: picture(height: 785)), ["screen": .string("SCREEN-A")])
        XCTAssertTrue(both["note"]?.stringValue?.contains("with the bar above its footer for the unsaved layout and its resting "
                                                          + "HUD's unsaved style") == true, "\(both["note"] ?? .null)")
    }

    /// The app's settings unsaved show the bar in every screen's panel, the shared layout's too, which the note names
    /// beside whatever else waits there.
    func testAPictureWithTheAppsDraftSaysWhyItIsUnsaved() throws {
        layout.proposeApp(AppDraft(botLogos: false))
        let (shared, _) = try answer(tools(picture: picture(height: 785)), [:])
        XCTAssertEqual(shared["unsaved"], .bool(true))
        XCTAssertTrue(shared["note"]?.stringValue?.hasPrefix("The hover panel as the saved layout draws it, with the bar above "
                                                             + "its footer that says the app's settings are unsaved, 540 by 785")
                      == true, "\(shared["note"] ?? .null)")
        layout.proposeStyle(HUDStyle(logoSize: 30), for: "SCREEN-A", hasNotch: false)
        let (styled, _) = try answer(tools(picture: picture(height: 785)), ["screen": .string("SCREEN-A")])
        XCTAssertTrue(styled["note"]?.stringValue?.contains("with the bar above its footer that says its resting HUD's style "
                                                            + "and the app's settings are unsaved, 540") == true,
                      "\(styled["note"] ?? .null)")
        layout.propose(.own(PanelLayout(order: ["Codex"])), for: "SCREEN-A")
        let (all, _) = try answer(tools(picture: picture(height: 785)), ["screen": .string("SCREEN-A")])
        XCTAssertTrue(all["note"]?.stringValue?.contains("with the bar above its footer for the unsaved layout, its resting "
                                                         + "HUD's unsaved style and the app's unsaved settings, 540") == true,
                      "\(all["note"] ?? .null)")
    }

    func testWithoutARendererThePictureIsRefused() {
        XCTAssertEqual(HUDTools(store: store, launch: 0).answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot)),
                       .failure("This copy of Agent HUD cannot draw the panel, so there is no picture of it here. "
                                + "get_hud reads the same panel as data."))
        XCTAssertEqual(tools(picture: nil).answer(MCPToolRequest(tool: MCPTools.getHUDScreenshot)),
                       .failure("The panel's picture could not be drawn. Try again, or read the panel with get_hud."))
        XCTAssertEqual(drawn, 1)
    }

    // MARK: Height

    func testThePanelIsReadWithItsHeightAndWhetherItScrolls() throws {
        let tools = tools(picture: nil, fit: PanelFit(height: 741, maximumHeight: 902))
        guard case .result(let panel, let images) = tools.answer(MCPToolRequest(tool: MCPTools.getHUD)) else {
            return XCTFail("get_hud was refused")
        }
        XCTAssertEqual(panel["height"], .integer(741))
        XCTAssertEqual(panel["scrolls"], .bool(false))
        XCTAssertTrue(images.isEmpty, "get_hud draws nothing")
        XCTAssertEqual(drawn, 0)
        XCTAssertEqual(measured, 1)
        XCTAssertEqual(scopes, [.shared], "the shared layout, measured on the main display")

        guard case .result(let proposed, _) = tools.answer(MCPToolRequest(tool: MCPTools.draftHUD,
                                                                          arguments: ["order": .array([.string("Codex")])])) else {
            return XCTFail("the proposal was refused")
        }
        XCTAssertEqual(proposed["height"], .integer(741), "a proposal's answer is the panel read again, its height included")
        let tall = self.tools(picture: nil, fit: PanelFit(height: 1_400, maximumHeight: 1_360))
        guard case .result(let scrolling, _) = tall.answer(MCPToolRequest(tool: MCPTools.getHUD)) else { return XCTFail() }
        XCTAssertEqual(scrolling["scrolls"], .bool(true))
    }

    func testWithoutAMeasurerTheHeightIsLeftOut() throws {
        let panel = try result(MCPTools.getHUD)
        XCTAssertNil(panel["height"])
        XCTAssertNil(panel["scrolls"])
        let unmeasured = tools(picture: nil, fit: nil)
        guard case .result(let read, _) = unmeasured.answer(MCPToolRequest(tool: MCPTools.getHUD)) else { return XCTFail() }
        XCTAssertNil(read["height"], "a measurer that finds nothing leaves the fields out too")
        XCTAssertNil(read["scrolls"])
    }
}
