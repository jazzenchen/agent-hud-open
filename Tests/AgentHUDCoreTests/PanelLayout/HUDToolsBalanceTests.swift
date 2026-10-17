import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// What the tools read and take of an API balance's measures and widgets: its estimated cost, its burn rate and when it
/// runs out at it over its block's period, read from the costs Agent HUD holds, and its number and its chart.
final class HUDToolsBalanceTests: HUDToolsCase {
    /// The sample with DeepSeek's account as given: its CNY balance, read at the sample's time unless its read failed,
    /// and its recorded costs as (hours before the sample's time, amounts by currency).
    private func account(_ total: String = "8.85", trend: Date? = nil, costs: [(Double, [String: String])],
                         failed: Bool = false) throws {
        let demo = try XCTUnwrap(store.report), now = demo.generatedAt
        let quarter = (now.timeIntervalSince1970 / 900).rounded(.down) * 900
        let billing = APIBilling(
            vendor: "DeepSeek",
            balances: [AccountBalance(currency: "CNY", total: Decimal(string: total)!, granted: 0, toppedUp: Decimal(string: total)!,
                                      runsOutAt: trend)],
            isAvailable: true, updatedAt: now,
            costs: costs.map { hours, amounts in
                CostBucket(start: Date(timeIntervalSince1970: quarter - hours * 3600), amounts: amounts.mapValues { Decimal(string: $0)! })
            },
            notice: failed ? "offline" : nil, readingIssue: failed ? .readFailed("offline") : nil)
        store.replace(report: UsageReport(generatedAt: now, snapshots: demo.snapshots, sessions: [], discoveredAgents: demo.discoveredAgents,
                                          consumers: demo.consumers, usage: demo.usage, insightsByAgent: demo.insightsByAgent,
                                          consumerIdsByQuota: demo.consumerIdsByQuota, billing: [billing], accounts: demo.accounts))
    }

    private func balance(in panel: [String: JSONValue]) throws -> [String: JSONValue] {
        try XCTUnwrap(try block("api:DeepSeek", in: panel)["balances"]?.arrayValue?.first?.objectValue)
    }

    private func time(_ value: JSONValue?) throws -> Date {
        try Date(try XCTUnwrap(value?.stringValue), strategy: .iso8601)
    }

    private func amount(_ value: JSONValue?) throws -> Double {
        NSDecimalNumber(decimal: try XCTUnwrap(value?.stringValue.flatMap { Decimal(string: $0) })).doubleValue
    }

    /// The panel's own period over its span so far, in days, as the burn rate spreads a cost over it.
    private func days(_ period: PanelTokenPeriod) -> Double {
        store.dataDate.timeIntervalSince(store.hudTokenInterval(period).start) / 86400
    }

    func testABalanceIsReadWithWhatItsMeasuresReadOverItsBlocksPeriod() throws {
        let trend = Date(timeIntervalSince1970: 1_900_000_000)
        try account(trend: trend, costs: [(1, ["CNY": "0.3"]), (72, ["CNY": "0.4"]), (240, ["CNY": "5"])])
        let read = try XCTUnwrap(store.enabledBilling.first?.updatedAt)

        let day = try balance(in: try result(MCPTools.getHUD))
        XCTAssertEqual(day["amount"], .string("8.85"))
        XCTAssertEqual(try time(day["runsOutAt"]), trend, "the balance's own trend stays as its provider worked it out")
        XCTAssertEqual(day["cost"], .string("0.3"), "the costs of the day, the token chart's period")
        XCTAssertEqual(try amount(day["burnPerDay"]), 0.3 / days(.day), accuracy: 0.000001)
        XCTAssertEqual(try time(day["burnRunsOutAt"]).timeIntervalSince(read), 8.85 / 0.3 * days(.day) * 86400, accuracy: 1,
                       "the balance runs out at that pace from the time it was read")

        let week = try balance(in: try result(MCPTools.draftHUD, ["blocks": .object(["api:DeepSeek": .object(["period": .string("week")])])]))
        XCTAssertEqual(week["cost"], .string("0.7"), "and of the week its block covers, without the cost from before it")
        XCTAssertEqual(try amount(week["burnPerDay"]), 0.7 / days(.week), accuracy: 0.000001)
        XCTAssertEqual(try time(week["burnRunsOutAt"]).timeIntervalSince(read), 8.85 / 0.7 * days(.week) * 86400, accuracy: 1)
        XCTAssertEqual(try time(week["runsOutAt"]), trend, "whatever the period")
        let month = try balance(in: try result(MCPTools.draftHUD, ["tokens": .object(["period": .string("month")])]))
        XCTAssertEqual(month["cost"], .string("5.7"), "a balance without a period of its own follows the token chart's")
    }

    /// A request without a price leaves the cost and the pace unknown, nothing spent gives no time, and a reading that
    /// shows no level has none, as a quota window's has none; what the requests cost is still told.
    func testWhatIsNotKnownIsNotTold() throws {
        try account(costs: [(1, ["CNY": "0.3"]), (2, ["USD": "0.04"])])
        let unpriced = try balance(in: try result(MCPTools.getHUD))
        for field in ["cost", "burnPerDay", "burnRunsOutAt"] { XCTAssertNil(unpriced[field], field) }
        XCTAssertEqual(unpriced["amount"], .string("8.85"))

        try account(costs: [])
        let idle = try balance(in: try result(MCPTools.getHUD))
        XCTAssertEqual(idle["cost"], .string("0"))
        XCTAssertEqual(idle["burnPerDay"], .string("0"))
        XCTAssertNil(idle["burnRunsOutAt"], "nothing spent gives no time")

        try account(costs: [(1, ["CNY": "0.3"])], failed: true)
        let failed = try balance(in: try result(MCPTools.getHUD))
        XCTAssertEqual(failed["state"], .string("read failed"))
        XCTAssertNil(failed["level"])
        XCTAssertEqual(failed["cost"], .string("0.3"))
        XCTAssertNotNil(failed["burnPerDay"])
        XCTAssertNil(failed["burnRunsOutAt"], "a balance whose read failed is no amount to run down")

        try account("0", costs: [(1, ["CNY": "0.3"])])
        let empty = try balance(in: try result(MCPTools.getHUD))
        XCTAssertEqual(empty["level"], .string("critical"))
        XCTAssertNil(empty["burnRunsOutAt"], "a balance at zero has run out")
    }

    func testABalancesNumberAndChartAreProposedAndReadBack() throws {
        let charted = try result(MCPTools.draftHUD, ["blocks": .object(["api:DeepSeek": .object([
            "widget": .string("chart"), "measures": .array([.string("cost")]), "span": .string("half"), "period": .string("month"),
        ])])])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["api:DeepSeek": PanelLayout.Block(widget: .chart, span: .half, period: .month)]),
                       "cost is all a balance's chart offers")
        let chart = try block("api:DeepSeek", in: charted)
        XCTAssertEqual(chart["widget"], .string("chart"))
        XCTAssertEqual(chart["widgets"], .array([.string("number"), .string("chart")]))
        XCTAssertEqual(chart["measures"], .array([.string("cost")]))
        XCTAssertEqual(chart["offeredMeasures"], .array([.string("cost")]))
        XCTAssertEqual(chart["period"], .string("month"))
        XCTAssertNotNil(chart["balances"], "a chart's balance is read as a number's is")

        let picked = try result(MCPTools.draftHUD, ["blocks": .object(["api:DeepSeek": .object([
            "widget": .string("number"), "measures": .array([.string("burn"), .string("cost")]),
        ])])])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["api:DeepSeek": PanelLayout.Block(measures: [.burnRate, .cost])]))
        XCTAssertEqual(try block("api:DeepSeek", in: picked)["measures"], .array([.string("burn"), .string("cost")]))
        XCTAssertEqual(try block("api:DeepSeek", in: picked)["offeredMeasures"], .array(["quota", "burn", "cost"].map(JSONValue.string)))

        // What get_hud reads back is the layout, sent back unchanged.
        let revision = layout.revision
        let sent = try XCTUnwrap(try result(MCPTools.getHUD)["layout"]?.objectValue)
        _ = try result(MCPTools.draftHUD, sent)
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["api:DeepSeek": PanelLayout.Block(measures: [.burnRate, .cost])]))
        XCTAssertEqual(layout.revision, revision, "the same draft is no change")

        // Every measure the number offers, in its order, is the default view.
        _ = try result(MCPTools.draftHUD, ["blocks": .object(["api:DeepSeek": .object([
            "measures": .array(["quota", "burn", "cost"].map(JSONValue.string)),
        ])])])
        XCTAssertNil(layout.draft)
    }

    func testAWidgetOrAMeasureABalanceDoesNotTakeIsRefusedWithTheReason() throws {
        let refused = try refusal(["blocks": .object([
            "api:DeepSeek": .object(["widget": .string("chart"), "measures": .array([.string("cost"), .string("burn")])]),
        ])])
        XCTAssertTrue(refused.contains(#"blocks.api:DeepSeek.measures[1]: The "chart" widget of "api:DeepSeek" does not offer the "#
                                       + #"measure "burn". The "chart" widget of an API balance draws its estimated cost over its "#
                                       + #"period. It offers: "cost"."#), refused)
        let tokens = try refusal(["blocks": .object(["api:DeepSeek": .object(["measures": .array([.string("tokens")])])])])
        XCTAssertTrue(tokens.contains(#"blocks.api:DeepSeek.measures[0]: The "number" widget of "api:DeepSeek" does not offer the "#
                                      + #"measure "tokens". An API balance is measured in money; the tokens of its models are in the "#
                                      + #"token chart. It offers: "quota", "burn", "cost"."#), tokens)
        for widget in ["bar", "ring"] {
            let ceiling = try refusal(["blocks": .object(["api:DeepSeek": .object(["widget": .string(widget)])])])
            XCTAssertTrue(ceiling.contains(#"is an API balance, which has no ceiling, so the "\#(widget)" widget has nothing to "#
                                           + #"measure it against. It accepts: "number", "chart"."#), ceiling)
        }
        let cells = try refusal(["blocks": .object(["api:DeepSeek": .object(["widget": .string("chart"), "columns": .integer(3)])])])
        XCTAssertTrue(cells.contains("drawn as a chart of its estimated cost, which has no cells to set in columns."), cells)
    }
}
