import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// Revision 2026-07-28, which has no handshake: every request says which revision it speaks, and a client finds out
/// about the server by asking.
final class MCPStatelessTests: XCTestCase {
    private let probe = MCPServerProbe()
    private let meta = MCPServerProbe.meta
    private let serverInfo: JSONValue = .object(["name": .string("agent-hud"), "title": .string("Agent HUD"), "version": .string("1.2.3")])
    private let revisions: JSONValue = .array(["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"].map(JSONValue.string))

    func testDiscoveryNamesTheRevisionsTheCapabilitiesAndTheServer() throws {
        // The first line Claude Code 2.1.296 writes to a stdio server.
        let response = try probe.response(to: #"{"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover","params":{\#(meta)}}"#)
        XCTAssertEqual(response["id"], .string("server-discover-probe-1"))
        let result = response["result"]
        XCTAssertEqual(result["resultType"], .string("complete"))
        XCTAssertEqual(result["supportedVersions"], revisions, "newest first")
        XCTAssertEqual(result["capabilities"], .object(["tools": .object(["listChanged": .bool(false)])]))
        XCTAssertEqual(result["_meta"]["io.modelcontextprotocol/serverInfo"], serverInfo)
        XCTAssertEqual(result["instructions"].stringValue, MCPTools.instructions)
        XCTAssertEqual(result["ttlMs"], .integer(300_000))
        XCTAssertEqual(result["cacheScope"], .string("public"))
    }

    func testARequestThatCarriesItsRevisionGetsACompleteResult() throws {
        let listed = try probe.response(to: #"{"method":"tools/list","jsonrpc":"2.0","id":0,"params":{\#(meta)}}"#)["result"]
        XCTAssertEqual(listed["tools"].arrayValue?.count, 5)
        XCTAssertEqual(listed["tools"], .array(MCPTools.definitions))
        XCTAssertEqual(listed["resultType"], .string("complete"))
        XCTAssertEqual(listed["ttlMs"], .integer(300_000))
        XCTAssertEqual(listed["cacheScope"], .string("public"), "the list is the same for everyone")
        XCTAssertEqual(listed["_meta"]["io.modelcontextprotocol/serverInfo"], serverInfo)

        // Claude Code adds keys of its own beside the protocol's.
        let call = #"{"method":"tools/call","params":{"name":"get_hud","arguments":{},\#(meta.dropLast()),"claudecode/toolUseId":"toolu_1","progressToken":1}},"jsonrpc":"2.0","id":1}"#
        let called = try probe.response(to: call)["result"]
        XCTAssertEqual(called["resultType"], .string("complete"))
        XCTAssertEqual(called["isError"], .bool(true))
        XCTAssertEqual(called.blockText, "Agent HUD is not running. Open Agent HUD and try again.")
        XCTAssertEqual(called["_meta"]["io.modelcontextprotocol/serverInfo"], serverInfo)
        XCTAssertNil(called.objectValue?["ttlMs"], "what a tool returns is not for caching")

        probe.answer = .reply(.result(["revision": .integer(1)]))
        let read = try probe.response(to: #"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_hud",\#(meta)}}"#)["result"]
        XCTAssertEqual(read.blockText, #"{"revision":1}"#)
        XCTAssertEqual([read["isError"], read["resultType"]], [.bool(false), .string("complete")])

        let unknown = try probe.response(to: #"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope",\#(meta)}}"#)
        XCTAssertEqual(unknown["error"], .object(["code": .integer(-32602), "message": .string("Unknown tool: nope")]))
    }

    func testARevisionTheServerDoesNotSpeakIsRefusedWithTheOnesItDoes() throws {
        let future = meta.replacingOccurrences(of: "2026-07-28", with: "2099-01-01")
        for method in ["server/discover", "tools/list", "tools/call", "subscriptions/listen", "somebody/elses"] {
            let response = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"\#(method)","params":{\#(future)}}"#)
            XCTAssertEqual(response["error"]["code"], .integer(-32022), method)
            XCTAssertEqual(response["error"]["message"], .string("Unsupported protocol version"))
            XCTAssertEqual(response["error"]["data"], .object(["supported": revisions, "requested": .string("2099-01-01")]))
        }
        XCTAssertTrue(probe.calls.isEmpty)
    }

    func testARequestWithoutARequiredFieldIsRefused() throws {
        let version = #""io.modelcontextprotocol/protocolVersion":"2026-07-28""#
        let capabilities = #""io.modelcontextprotocol/clientCapabilities":{}"#
        let broken = [#"{"jsonrpc":"2.0","id":1,"method":"server/discover"}"#,
                      #"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{}}}"#,
                      #"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{\#(version)}}}"#,
                      #"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{\#(version)}}}"#,
                      #"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{\#(capabilities)}}}"#,
                      #"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":20260728,\#(capabilities)}}}"#,
                      #"{"jsonrpc":"2.0","id":1,"method":"subscriptions/listen","params":{"notifications":{"toolsListChanged":true}}}"#]
        for line in broken {
            let response = try probe.response(to: line)
            XCTAssertEqual(response["error"]["code"], .integer(-32602), line)
            XCTAssertEqual(response["id"], .integer(1))
        }
        let whole = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{\#(version),\#(capabilities)}}}"#)
        XCTAssertEqual(whole["result"]["resultType"], .string("complete"), "the client's name is optional")
    }

    func testAMethodTheRevisionDoesNotHaveIsStillRefusedAtOnce() throws {
        let response = try probe.response(to: #"{"jsonrpc":"2.0","id":"r","method":"resources/list","params":{\#(meta)}}"#)
        XCTAssertEqual(response["error"]["code"], .integer(-32601))
        let ping = try probe.response(to: #"{"jsonrpc":"2.0","id":"p","method":"ping","params":{\#(meta)}}"#)
        XCTAssertEqual(ping["result"]["resultType"], .string("complete"), "a client that still pings is told the server is there")
    }

    func testASubscriptionIsAcknowledgedAsHoldingNothingAndClosed() throws {
        for id in ["1", #""listen-a""#] {
            let line = #"{"jsonrpc":"2.0","id":\#(id),"method":"subscriptions/listen","params":{\#(meta),"notifications":{"toolsListChanged":true,"resourceSubscriptions":["file:///x"]}}}"#
            let messages = try probe.messages(for: line)
            let tag = try JSONValue.parse(Data(id.utf8))
            XCTAssertEqual(messages.count, 2)
            XCTAssertEqual(messages.first, .object([
                "jsonrpc": .string("2.0"), "method": .string("notifications/subscriptions/acknowledged"),
                "params": .object(["_meta": .object(["io.modelcontextprotocol/subscriptionId": tag]), "notifications": .object([:])]),
            ]), "the acknowledgement comes first and agrees to none of the kinds asked for: nothing here changes")
            let closed = try XCTUnwrap(messages.last)
            XCTAssertEqual(closed["id"], tag)
            XCTAssertEqual(closed["result"]["resultType"], .string("complete"), "then the subscription ends cleanly")
            XCTAssertEqual(closed["result"]["_meta"]["io.modelcontextprotocol/subscriptionId"], tag)
        }
    }

    func testAHandshakeRevisionNamedInARequestIsAnsweredAsThatRevisionDefines() throws {
        // What a client does that was told which revisions the server speaks and picked an older one.
        let older = meta.replacingOccurrences(of: "2026-07-28", with: "2025-11-25")
        let listed = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{\#(older)}}"#)
        XCTAssertEqual(listed["result"].objectValue?.keys.sorted(), ["tools"], "no result type, no caching hints")
        let called = try probe.response(to: #"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_hud",\#(older)}}"#)
        XCTAssertEqual(called["result"].objectValue?.keys.sorted(), ["content", "isError"])
        XCTAssertEqual(try probe.response(to: #"{"jsonrpc":"2.0","id":3,"method":"ping","params":{\#(older)}}"#)["result"], .object([:]))
        for method in ["server/discover", "subscriptions/listen"] {
            let response = try probe.response(to: #"{"jsonrpc":"2.0","id":4,"method":"\#(method)","params":{\#(older)}}"#)
            XCTAssertEqual(response["error"]["code"], .integer(-32601), "\(method) is not a method of 2025-11-25")
        }
        let bare = try probe.response(to: #"{"jsonrpc":"2.0","id":5,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2025-06-18"}}}"#)
        XCTAssertEqual(bare["result"]["tools"].arrayValue?.count, 5, "a handshake revision asks for no capabilities in the request")
    }

    func testTheListIsOnePageSoACursorIsUnknown() throws {
        let paged = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":"eyJwYWdlIjogMn0=",\#(meta)}}"#)
        XCTAssertEqual(paged["error"], .object(["code": .integer(-32602), "message": .string("Invalid params: unknown cursor")]))
        let first = try probe.response(to: #"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":null,\#(meta)}}"#)
        XCTAssertEqual(first["result"]["tools"].arrayValue?.count, 5)
        XCTAssertNil(first["result"].objectValue?["nextCursor"], "nothing follows the one page")
    }

    func testAHandshakeIsAnsweredWhateverElseTheRequestCarries() throws {
        let response = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25",\#(meta)}}"#)
        XCTAssertEqual(response["result"]["protocolVersion"], .string("2025-11-25"))
        XCTAssertNil(response["result"].objectValue?["resultType"])
    }
}
