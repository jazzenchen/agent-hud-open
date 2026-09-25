import XCTest
import AgentHUDSupport
@testable import AgentHUDCore

final class TurnCallsTests: XCTestCase {
    private func at(_ time: String) -> Int64 { RecordCoding.milliseconds(DateParsing.iso8601("2026-09-25T\(time)Z")!) }
    private func date(_ time: String) -> Date { DateParsing.iso8601("2026-09-25T\(time)Z")! }

    func testClaudeCallsNameTheToolsOfEveryLineOfTheirResponse() {
        let log = """
        {"type":"assistant","message":{"id":"m1","content":[{"type":"thinking","thinking":"…"}],"usage":{"input_tokens":10,"output_tokens":5}},"requestId":"r1","timestamp":"2026-09-25T02:00:00.000Z"}
        {"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","name":"Read","input":{}}],"usage":{"input_tokens":10,"output_tokens":5}},"requestId":"r1","timestamp":"2026-09-25T02:00:01.500Z"}
        {"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"\\"type\\":\\"assistant\\""}]},"timestamp":"2026-09-25T02:00:02.000Z"}
        {"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","name":"mcp__server__fetch","input":{}},{"type":"tool_use","name":"Bash","input":{}}],"usage":{"input_tokens":20,"output_tokens":5}},"requestId":"r2","timestamp":"2026-09-25T02:00:03.000Z"}
        {"type":"assistant","message":{"id":"m3","content":[{"type":"text","text":"Done"}],"usage":{"input_tokens":30,"output_tokens":5}},"requestId":"r3","timestamp":"2026-09-25T03:00:00.000Z"}
        """
        let tools = CallTools.claude(Data(log.utf8), from: date("02:00:00.000"), through: date("02:00:03.000"))
        XCTAssertEqual(tools[at("02:00:00.000")], ["Read"], "a response is keyed by its first line")
        XCTAssertEqual(tools[at("02:00:03.000")], ["mcp__server__fetch", "Bash"])
        XCTAssertNil(tools[at("03:00:00.000")], "lines well past the turn are not read")
    }

    func testCodexCallsTakeTheToolsRequestedBeforeTheCountThatClosesThem() {
        let log = """
        {"timestamp":"2026-09-25T02:00:00.000Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{}"}}
        {"timestamp":"2026-09-25T02:00:00.500Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c","output":"ok"}}
        {"timestamp":"2026-09-25T02:00:01.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":10}}}}
        {"timestamp":"2026-09-25T02:00:01.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":10}}}}
        {"timestamp":"2026-09-25T02:00:02.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"…"}}
        {"timestamp":"2026-09-25T02:00:03.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"output_tokens":30}}}}
        {"timestamp":"2026-09-25T02:00:05.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":400,"output_tokens":60}}}}
        """
        let tools = CallTools.codex(Data(log.utf8), from: date("02:00:01.000"), through: date("02:00:05.000"))
        XCTAssertEqual(tools[at("02:00:01.000")], ["shell"])
        XCTAssertNil(tools[at("02:00:01.100")], "a count that does not move the totals closes no call")
        XCTAssertEqual(tools[at("02:00:03.000")], ["apply_patch"])
        XCTAssertEqual(tools[at("02:00:05.000")], [], "a call that asked for no tool")
    }
}
