import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tools read and tell of a block's span: a proposal's span is read strictly, half on bar rows is refused,
/// and `get_hud` tells the span each block takes where its panel lays it out, with a note where a half takes the
/// whole line.
final class HUDToolsSpanTests: HUDToolsCase {
    private func half(_ widget: String? = nil) -> JSONValue {
        var block: [String: JSONValue] = ["span": .string("half")]
        if let widget { block["widget"] = .string(widget) }
        return .object(block)
    }

    func testASpanIsReadStrictly() throws {
        let typed = try refusal(["blocks": .object([
            "Claude": .object(["widget": .string("ring"), "span": .string("wide")]),
            "Codex": .object(["widget": .string("ring"), "span": .integer(2)]),
        ])])
        for part in ["blocks.Claude.span is one of \"full\", \"half\".", "blocks.Codex.span is one of \"full\", \"half\"."] {
            XCTAssertTrue(typed.contains(part), "\(part) — \(typed)")
        }
        let bars = try refusal(["blocks": .object(["Claude": half(), "Codex": half("bar")])])
        for key in ["Claude", "Codex"] {
            let part = "blocks.\(key).span: Bar rows need the full width of the panel, so the \"bar\" widget of \"\(key)\" "
                + "cannot take half a line."
            XCTAssertTrue(bars.contains(part), "\(part) — \(bars)")
        }
        XCTAssertFalse(try refusal(["blocks": .object(["Claude": half("ring"), "Codex": half(), "api:DeepSeek": half()])])
            .contains("blocks.Claude"), "only the bar rows are refused")
    }

    /// A proposal's halves are drawn and told back: each block's span where the panel lays it out, the whole line for a
    /// block the layout leaves at the default, and columns as a half sets them.
    func testHalvesAreProposedAndToldBack() throws {
        let panel = try result(MCPTools.draftHUD, [
            "blocks": .object(["Claude": half("ring"), "Codex": .object(["widget": .string("number"), "span": .string("half"),
                                                                         "columns": .integer(6)])]),
        ])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(widget: .ring, span: .half),
                                                          "Codex": PanelLayout.Block(widget: .number, columns: 6, span: .half)]))
        XCTAssertEqual(try block("Claude", in: panel)["span"], .string("half"))
        XCTAssertEqual(try block("Claude", in: panel)["columns"], .integer(2), "two to a line in half of one")
        XCTAssertEqual(try block("Codex", in: panel)["span"], .string("half"))
        XCTAssertEqual(try block("Codex", in: panel)["columns"], .integer(3), "six asked, three fit half of 540")
        XCTAssertEqual(try block("api:DeepSeek", in: panel)["span"], .string("full"))
        XCTAssertFalse(panel["note"]?.stringValue?.contains("whole line") ?? true, "every half shares a line")
        XCTAssertEqual(try result(MCPTools.getHUD)["blocks"], panel["blocks"], "get_hud reads the same")

        // The whole line, spelt out, is the default and leaves no draft.
        layout.discard()
        _ = try result(MCPTools.draftHUD, ["blocks": .object(["Claude": .object(["span": .string("full")])])])
        XCTAssertFalse(layout.hasDraft)
    }

    /// A half block that takes the whole line is told so, and why: no half beside it, or too little room for two.
    func testANoteSaysWhichHalvesTakeTheWholeLineAndWhy() throws {
        let lone = try result(MCPTools.draftHUD, ["blocks": .object(["Claude": half("ring")])])
        XCTAssertEqual(try block("Claude", in: lone)["span"], .string("full"))
        XCTAssertEqual(try block("Claude", in: lone)["columns"], .integer(4), "across the whole line, four as before")
        XCTAssertTrue(lone["note"]?.stringValue?.hasSuffix(
            " \"Claude\" takes the whole line: no half block is left next to it to share a line with.") == true, "\(lone)")

        let narrow = try result(MCPTools.draftHUD, [
            "blocks": .object(["Claude": half("ring"), "Codex": half("number")]), "style": .object(["width": .integer(480)]),
        ])
        XCTAssertEqual(try block("Claude", in: narrow)["span"], .string("full"))
        XCTAssertEqual(try block("Codex", in: narrow)["span"], .string("full"))
        XCTAssertTrue(narrow["note"]?.stringValue?.hasSuffix(
            " \"Claude\", \"Codex\" take the whole line: the panel is too narrow here for two halves side by side.") == true,
                      "\(narrow)")
        XCTAssertEqual(layout.draft?.blocks["Claude"]?.span, .half, "the layout keeps what was asked")

        // A hidden half is told the span it asks for, and is in no line.
        let hidden = try result(MCPTools.draftHUD, [
            "blocks": .object(["Claude": .object(["widget": .string("ring"), "span": .string("half"), "hidden": .bool(true)])]),
        ])
        XCTAssertEqual(try block("Claude", in: hidden)["span"], .string("half"))
        XCTAssertEqual(try block("Claude", in: hidden)["shown"], .bool(false))
    }

    /// The span in effect is where the host lays the panel out: a display whose insets leave less room than the notch
    /// sets the same halves across the whole line.
    func testTheSpanFollowsTheWidthTheHostMeasures() throws {
        let proposal: [String: JSONValue] = ["blocks": .object(["Claude": half("ring"), "Codex": half("chart")])]
        _ = try result(MCPTools.draftHUD, proposal)
        var width = 540.0 - 36
        tools = HUDTools(store: store, launch: 0, fit: { _ in PanelFit(height: 700, maximumHeight: 900, contentWidth: width) })
        XCTAssertEqual(try block("Codex", in: try result(MCPTools.getHUD))["span"], .string("half"))
        width = 540 - 36 - 52
        XCTAssertEqual(try block("Codex", in: try result(MCPTools.getHUD))["span"], .string("full"), "a side dock's strip")
    }
}
