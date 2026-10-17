import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// JSON-RPC's rules and the handshake revisions: what every client relies on before it calls a tool.
final class MCPServerTests: XCTestCase {
    private let probe = MCPServerProbe()

    func testTheHandshakeAnswersWithTheRevisionAskedFor() throws {
        // Claude Code 2.1.296 told to skip the newest revision, and Codex 0.162.1, as they open.
        let claude = #"{"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"roots":{"listChanged":true},"elicitation":{"form":{},"url":{}}},"clientInfo":{"name":"claude-code","title":"Claude Code","version":"2.1.296"}},"jsonrpc":"2.0","id":0}"#
        let codex = #"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"experimental":{"codex/auth-change":{}},"elicitation":{"form":{},"url":{}}},"clientInfo":{"name":"codex-mcp-client","title":"Codex","version":"0.162.1"}}}"#
        XCTAssertEqual(try probe.response(to: claude)["result"]["protocolVersion"], .string("2025-11-25"))
        XCTAssertEqual(try probe.response(to: codex)["result"]["protocolVersion"], .string("2025-06-18"))
        for revision in ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"] {
            let line = #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"\#(revision)","capabilities":{}}}"#
            XCTAssertEqual(try probe.response(to: line)["result"]["protocolVersion"], .string(revision))
        }

        let result = try probe.response(to: claude)["result"]
        XCTAssertEqual(result["capabilities"], .object(["tools": .object(["listChanged": .bool(false)])]))
        XCTAssertEqual(result["serverInfo"], .object(["name": .string("agent-hud"), "title": .string("Agent HUD"), "version": .string("1.2.3")]))
        XCTAssertEqual(result["instructions"].stringValue, "Agent HUD shows AI coding agents' quota, usage and sessions at the Mac's notch. "
                       + "These tools read what its hover panel shows and rearrange it, for every screen or for one, "
                       + "restyle one screen's resting HUD, and change an app setting the user asks for by name. "
                       + "A change is a draft until the user presses Save in the panel. "
                       + "A layout edit holds the panel open for two minutes; call finish_hud_edit when you are done.")
        XCTAssertEqual(result.objectValue?.keys.sorted(), ["capabilities", "instructions", "protocolVersion", "serverInfo"],
                       "a handshake client gets the result its revision defines and nothing else")
    }

    func testARevisionTheServerDoesNotSpeakGetsTheNewestHandshakeRevision() throws {
        for params in [#"{"protocolVersion":"2099-01-01"}"#, #"{"protocolVersion":"1.0.0","capabilities":{}}"#,
                       #"{"protocolVersion":"2026-07-28"}"#, #"{}"#] {
            let response = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":\#(params)}"#)
            XCTAssertEqual(response["result"]["protocolVersion"], .string("2025-11-25"), params)
        }
    }

    func testANotificationGetsNoAnswer() throws {
        for line in [#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                     #"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1,"reason":"gave up"}}"#,
                     #"{"jsonrpc":"2.0","method":"notifications/roots/list_changed"}"#,
                     #"{"jsonrpc":"2.0","method":"somebody/elses","params":{"x":1}}"#,
                     #"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"get_hud"}}"#] {
            XCTAssertEqual(probe.server.lines(for: line), [], line)
        }
        XCTAssertTrue(probe.calls.isEmpty, "a tool call nobody waits on is not made")
    }

    func testPingIsAnsweredWithAnEmptyResult() throws {
        let response = try probe.response(to: #"{"jsonrpc":"2.0","id":"123","method":"ping"}"#)
        XCTAssertEqual(response, .object(["jsonrpc": .string("2.0"), "id": .string("123"), "result": .object([:])]))
    }

    func testAMethodTheServerDoesNotHaveIsRefusedAtOnce() throws {
        for method in ["resources/list", "prompts/list", "logging/setLevel", "tools/save", "panel/save"] {
            let response = try probe.response(to: #"{"jsonrpc":"2.0","id":4,"method":"\#(method)","params":{}}"#)
            XCTAssertEqual(response["id"], .integer(4))
            XCTAssertEqual(response["error"]["code"], .integer(-32601), method)
            XCTAssertEqual(response["error"]["message"].stringValue, "Method not found: \(method)")
            XCTAssertNil(response.objectValue?["result"])
        }
    }

    func testInputThatIsNotJSONIsAParseErrorWithNoID() throws {
        for line in ["garbage", "{", #"{"jsonrpc":"2.0","id":1,"method":"ping""#, #"{"jsonrpc":"2.0" "id":1}"#, "{}{}"] {
            let response = try probe.response(to: line)
            XCTAssertEqual(response["error"]["code"], .integer(-32700), line)
            XCTAssertEqual(response.objectValue?["id"], .null, "the id could not be read")
        }
        // {"a":"<one byte that is not UTF-8>"}
        let notText = probe.server.lines(for: Data([0x7B, 0x22, 0x61, 0x22, 0x3A, 0x22, 0xFF, 0x22, 0x7D]))
        XCTAssertEqual(try JSONValue.parse(Data(try XCTUnwrap(notText.first).utf8))["error"]["code"], .integer(-32700),
                       "bytes that are not UTF-8 are no message")
    }

    func testWhatIsNoRequestIsAnInvalidRequest() throws {
        let unreadable = ["42", #""ping""#, "[]", "null", #"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
                          #"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#, #"{"jsonrpc":"2.0","id":true,"method":"ping"}"#,
                          #"{"jsonrpc":"2.0"}"#, #"{"method":"ping"}"#]
        for line in unreadable {
            let response = try probe.response(to: line)
            XCTAssertEqual(response["error"]["code"], .integer(-32600), line)
            XCTAssertEqual(response.objectValue?["id"], .null, line)
        }
        // An id that can be read comes back, so the client can tell which of its messages was refused.
        for line in [#"{"jsonrpc":"2.0","id":7}"#, #"{"jsonrpc":"1.0","id":7,"method":"ping"}"#, #"{"id":7,"method":"ping"}"#,
                     #"{"jsonrpc":"2.0","id":7,"method":12}"#] {
            let response = try probe.response(to: line)
            XCTAssertEqual(response["error"]["code"], .integer(-32600), line)
            XCTAssertEqual(response["id"], .integer(7), line)
        }
        let listed = try probe.response(to: #"{"jsonrpc":"2.0","id":7,"method":"tools/list","params":[1,2]}"#)
        XCTAssertEqual(listed["error"]["code"], .integer(-32602), "params are an object or left out")
    }

    func testAnIDComesBackAsItArrived() throws {
        for id in [#""abc""#, "7", #""7""#, "0", "-1", "9007199254740993", #""server-discover-probe-1""#, #""""#] {
            let line = try XCTUnwrap(probe.server.lines(for: #"{"jsonrpc":"2.0","id":\#(id),"method":"ping"}"#).first)
            XCTAssertTrue(line.contains(#""id":\#(id),"#), "\(id) in \(line)")
            let unknown = try XCTUnwrap(probe.server.lines(for: #"{"jsonrpc":"2.0","id":\#(id),"method":"nope"}"#).first)
            XCTAssertTrue(unknown.contains(#""id":\#(id),"#), "\(id) in \(unknown)")
        }
    }

    func testABlankLineAndAStrayResponseAreNotAnswered() {
        for line in ["", "   ", "\t", "\r", #"{"jsonrpc":"2.0","id":1,"result":{}}"#,
                     #"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#] {
            XCTAssertEqual(probe.server.lines(for: line), [], line)
        }
    }

    func testSeveralMessagesSentAsOneAreAnsweredAsOne() throws {
        let batch = #"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},"#
            + #"{"jsonrpc":"2.0","id":"b","method":"nope"},7]"#
        let lines = try probe.messages(for: batch)
        XCTAssertEqual(lines.count, 1, "the responses travel as one line too")
        let responses = try XCTUnwrap(lines.first?.arrayValue)
        XCTAssertEqual(responses.map { $0.objectValue?["id"] }, [.integer(1), .string("b"), .null])
        XCTAssertEqual(responses[0]["result"], .object([:]))
        XCTAssertEqual(responses[1]["error"]["code"], .integer(-32601))
        XCTAssertEqual(responses[2]["error"]["code"], .integer(-32600))
        XCTAssertEqual(probe.server.lines(for: #"[{"jsonrpc":"2.0","method":"notifications/initialized"}]"#), [],
                       "notifications alone get no answer")

        let listen = #"[{"jsonrpc":"2.0","id":9,"method":"subscriptions/listen","params":{\#(MCPServerProbe.meta),"notifications":{}}}]"#
        let mixed = try probe.messages(for: listen)
        XCTAssertEqual(mixed.count, 2)
        XCTAssertEqual(mixed.first?["method"], .string("notifications/subscriptions/acknowledged"), "a notification is no response and travels alone")
        XCTAssertEqual(mixed.last?.arrayValue?.map { $0["id"] }, [.integer(9)])
    }
}
