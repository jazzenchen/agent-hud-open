import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// Which agents a screen's resting HUD shows: the layout's list, how it is read and written, and how it narrows and
/// orders what the HUD would show.
final class PanelHUDTests: XCTestCase {
    private func json(_ layout: PanelLayout) throws -> String {
        String(decoding: try RecordCoding.encoder().encode(layout), as: UTF8.self)
    }

    private func read(_ text: String) throws -> PanelLayout {
        try JSONDecoder().decode(PanelLayout.self, from: Data(text.utf8))
    }

    // MARK: The document

    func testTheListIsWrittenOnlyWhenItNamesAnAgent() throws {
        XCTAssertEqual(try json(PanelLayout(hud: PanelLayout.HUD())), "{}", "every agent, as it comes, is the default")
        XCTAssertTrue(PanelLayout(hud: PanelLayout.HUD(agents: [""])).isDefault)
        let listed = PanelLayout(hud: PanelLayout.HUD(agents: ["Codex", "Claude"]))
        XCTAssertFalse(listed.isDefault)
        XCTAssertEqual(try json(listed), #"{"hud":{"agents":["Codex","Claude"]}}"#)
        XCTAssertEqual(try read(#"{"hud":{"agents":["Codex","Claude"]}}"#), listed)
    }

    /// A mark is named once, by the first name given for it: Codex and ChatGPT are one mark, as on the HUD.
    func testTheListNamesEachMarkOnceInTheOrderGiven() {
        let untidy = PanelLayout(hud: PanelLayout.HUD(agents: ["Codex", "", "Claude", "ChatGPT", "Claude", "Grok CLI", "Grok"]))
        XCTAssertEqual(untidy.normalized().hud.agents, ["Codex", "Claude", "Grok CLI"])
        XCTAssertEqual(untidy.normalized(), untidy.normalized().normalized())
    }

    func testAStoredListIsReadForgivingly() throws {
        XCTAssertEqual(try read(#"{"hud":{"agents":["Claude",7,null],"size":"large"}}"#).hud.agents, ["Claude"])
        XCTAssertEqual(try read(#"{"hud":"Claude","order":["Codex"]}"#), PanelLayout(order: ["Codex"]), "a hud that is not an object")
        XCTAssertEqual(try read(#"{"hud":{"agents":"Claude"}}"#).hud, PanelLayout.HUD())
    }

    func testAnAgentIsToldWhatItsHUDCannotRead() {
        let found = PanelLayout.misreadings(of: .object([
            "hud": .object(["agents": .array([.string("Claude"), .integer(2), .string("")]), "size": .string("large")]),
        ]))
        XCTAssertTrue(found.contains("hud.agents[1] is a string."), "\(found)")
        XCTAssertTrue(found.contains("hud.agents[2] is an agent's name from get_hud's screens."), "\(found)")
        XCTAssertTrue(found.contains("hud has no property \"size\"; the hud takes agents, mode, edge, position, logoSize, logoGap, "
                                     + "edgeDistance, showLogos, simpleSize, simpleGap, ringWidth, showRings, showNumbers, "
                                     + "separateAgents, appearance, material, glassStyle, transparency, glow and reset, "
                                     + "and solidTransparency and glassTransparency as get_hud lists them."), "\(found)")
        XCTAssertEqual(PanelLayout.misreadings(of: .object(["hud": .string("Claude")])), ["hud is an object."])
        XCTAssertEqual(PanelLayout.misreadings(of: .object(["hud": .object(["agents": .string("Claude")])])), ["hud.agents is a list."])
        XCTAssertEqual(PanelLayout.misreadings(of: .object(["hud": .null])), [])
    }

    // MARK: Narrowing what the HUD shows

    func testAListNarrowsAndOrdersWhatTheHUDWouldShow() {
        let available = ["Claude", "Codex", "Cursor", "Kimi"]
        let order = HUDAgentOrder(listed: ["Kimi", "Claude"], available: available)
        XCTAssertTrue(order.narrows)
        XCTAssertEqual(order.arrange(available) { $0 }, ["Kimi", "Claude"])
        XCTAssertTrue(order.shows("Kimi"))
        XCTAssertFalse(order.shows("Codex"))
    }

    /// The HUD merges vendors that share a mark, and so does the list: "ChatGPT" names Codex's mark, and two items of
    /// one mark keep the order they came in.
    func testTheListMatchesByMark() {
        let order = HUDAgentOrder(listed: ["ChatGPT", "Grok"], available: ["Claude", "Codex", "Grok CLI"])
        XCTAssertEqual(order.arrange(["Claude", "Grok CLI", "Codex", "Grok"]) { $0 }, ["Codex", "Grok CLI", "Grok"])
        XCTAssertTrue(order.shows("Codex"))
        XCTAssertTrue(order.shows("Grok CLI"))
    }

    /// An agent listed that is not on the HUD now is passed over; a list that names none of the agents there narrows
    /// nothing, so the screen shows every agent rather than an empty HUD.
    func testAListThatNamesNoAgentShownNowNarrowsNothing() {
        let available = ["Claude", "Codex"]
        let some = HUDAgentOrder(listed: ["Pi", "Codex"], available: available)
        XCTAssertEqual(some.arrange(available) { $0 }, ["Codex"])
        for listed in [[], ["Pi"], ["", "Cursor"]] {
            let order = HUDAgentOrder(listed: listed, available: available)
            XCTAssertFalse(order.narrows, "\(listed)")
            XCTAssertEqual(order.arrange(available) { $0 }, available, "\(listed)")
            XCTAssertTrue(order.shows("Pi"), "\(listed)")
        }
        XCTAssertFalse(HUDAgentOrder(listed: ["Claude"], available: []).narrows, "an empty HUD stays as it is")
    }

    // MARK: What an agent may list

    func testAnAgentTheScreenCannotShowIsAnIssueUnlessTheLayoutHoldsIt() {
        let proposal = PanelLayout(hud: PanelLayout.HUD(agents: ["Codex", "Pi", "ChatGPT", "Zed"]))
        let issues = proposal.hudIssues(against: ["Claude", "Codex"], whose: "this screen's resting HUD")
        XCTAssertEqual(issues.map(\.path), ["hud.agents[1]", "hud.agents[3]"], "ChatGPT is Codex's mark")
        XCTAssertEqual(issues.first?.message, "\"Pi\" is not an agent this screen's resting HUD can show now. It can show: \"Claude\", \"Codex\".")

        let held = PanelLayout(hud: PanelLayout.HUD(agents: ["Pi"]))
        XCTAssertEqual(proposal.hudIssues(against: ["Claude", "Codex"], whose: "it", holding: [held]).map(\.path), ["hud.agents[3]"],
                       "an agent the layout already lists may come back with it")
        XCTAssertEqual(PanelLayout(hud: PanelLayout.HUD(agents: ["Claude"])).hudIssues(against: [], whose: "it").first?.message,
                       "\"Claude\" is not an agent it can show now. It shows no agent now.")
        XCTAssertTrue(PanelLayout().hudIssues(against: [], whose: "it").isEmpty)
    }
}
