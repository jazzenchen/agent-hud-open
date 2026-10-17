import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tools read and take of the options a layout sets beside its widgets: a block's measures, countdown,
/// columns and ring size, and the panel's style.
final class HUDToolsOptionsTests: HUDToolsCase {
    /// The countdown, the columns, the ring size and the style are read as strictly as the rest: a misspelt or
    /// mistyped value is refused with its path and what is accepted, and so is one the block has nothing to set for.
    func testTheNewOptionsAreReadStrictly() throws {
        let typed = try refusal([
            "blocks": .object(["Claude": .object(["widget": .string("ring"), "countdown": .string("off"), "columns": .integer(7),
                                                  "ringSize": .string("huge")]),
                               "Codex": .object(["widget": .string("number"), "columns": .string("3")])]),
            "style": .object(["spacing": .string("airy"), "separators": .integer(0), "dividers": .bool(false)]),
        ])
        for part in ["blocks.Claude.countdown is true or false.", "blocks.Claude.columns is a whole number from 2 to 6.",
                     "blocks.Claude.ringSize is one of \"small\", \"medium\", \"large\".",
                     "blocks.Codex.columns is a whole number from 2 to 6.",
                     "style.spacing is one of \"compact\", \"regular\", \"roomy\".", "style.separators is true or false.",
                     "style has no property \"dividers\"; the style takes spacing, separators and width."] {
            XCTAssertTrue(typed.contains(part), "\(part) — \(typed)")
        }
        XCTAssertTrue(try refusal(["style": .string("compact")]).contains("style is an object."))
        for columns in [JSONValue.integer(1), .number(2.5), .integer(-4), .bool(true)] {
            let sentence = try refusal(["blocks": .object(["Claude": .object(["widget": .string("ring"), "columns": columns])])])
            XCTAssertTrue(sentence.contains("blocks.Claude.columns is a whole number from 2 to 6."), "\(columns) — \(sentence)")
        }

        let unfit = try refusal(["blocks": .object([
            "Claude": .object(["columns": .integer(3), "ringSize": .string("small")]),
            "api:DeepSeek": .object(["countdown": .bool(false)]),
        ])])
        for part in ["blocks.Claude.columns: The \"bar\" widget of \"Claude\" draws no cells.",
                     "blocks.Claude.ringSize: The \"bar\" widget of \"Claude\" draws no ring.",
                     "blocks.api:DeepSeek.countdown: \"api:DeepSeek\" is an API balance, which has no reset to count down to."] {
            XCTAssertTrue(unfit.contains(part), "\(part) — \(unfit)")
        }
    }

    /// What a proposal sets for the countdown, the cells and the whole panel is drawn and told back.
    func testTheNewOptionsAreProposedAndToldBack() throws {
        let panel = try result(MCPTools.draftHUD, [
            "blocks": .object(["Claude": .object(["widget": .string("ring"), "columns": .integer(2), "ringSize": .string("large"),
                                                  "countdown": .bool(false), "measures": .array([.string("cost")])]),
                               "Codex": .object(["widget": .string("number"), "columns": .number(6)])]),
            "style": .object(["spacing": .string("compact"), "separators": .bool(false)]),
        ])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: [
            "Claude": PanelLayout.Block(widget: .ring, measures: [.cost], countdown: false, columns: 2, ringSize: .large),
            "Codex": PanelLayout.Block(widget: .number, columns: 6),
        ], style: PanelStyle(spacing: .compact, separators: false)))
        XCTAssertEqual(panel["style"], .object(["spacing": .string("compact"), "separators": .bool(false), "width": .integer(540)]))
        let claude = try block("Claude", in: panel)
        XCTAssertEqual(claude["measures"], .array([.string("cost")]))
        XCTAssertEqual(claude["countdown"], .bool(false))
        XCTAssertEqual(claude["columns"], .integer(2))
        XCTAssertEqual(claude["ringSize"], .string("large"))
        let codex = try block("Codex", in: panel)
        XCTAssertEqual(codex["columns"], .integer(6))
        XCTAssertNil(codex["ringSize"], "numbers have no rings")
        XCTAssertEqual(codex["countdown"], .bool(true))

        // The layout's own countdown is Settings' Reset countdown: off, the blocks that set none of their own show none.
        let off = try result(MCPTools.draftHUD, ["countdown": .bool(false),
                                                       "blocks": .object(["Claude": .object(["countdown": .bool(true)])])])
        XCTAssertEqual(off["countdown"], .bool(false))
        XCTAssertEqual(try block("Codex", in: off)["countdown"], .bool(false))
        XCTAssertEqual(try block("Claude", in: off)["countdown"], .bool(true), "a block's own countdown overrides the layout's")
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(countdown: true)], countdown: false))
    }
}
