import AgentHUDSupport
import Darwin
import Foundation
import XCTest
@testable import AgentHUDCore

/// A tool call's way from the MCP process to the running application and back, over the socket in the data directory.
@MainActor
final class MCPAppChannelTests: XCTestCase {
    /// A short path: with the socket's name and the lock beside it, it has to fit a socket address.
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("mcp-" + UUID().uuidString.prefix(8))
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        XCTAssertTrue(UnixSocket.fits(MCPAppChannel.socketPath(in: url) + ".lock"))
        return url
    }

    /// Calls the way the process does, off the main actor, which the application answers on. The listener comes up
    /// on the main queue, so the first call waits for it.
    private func call(_ request: MCPToolRequest, at path: String, timeout: TimeInterval = 5) async -> MCPAppClient.Answer {
        var answer = MCPAppClient.Answer.notRunning
        for _ in 0..<300 {
            answer = await Task.detached { MCPAppClient.call(request, socket: path, timeout: timeout) }.value
            guard answer == .notRunning else { break }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return answer
    }

    /// Sends `bytes` as one request and returns everything that came back.
    private func exchange(_ bytes: Data, at path: String) async -> Data? {
        await Task.detached {
            guard let descriptor = UnixSocket.connect(to: path) else { return nil }
            defer { close(descriptor) }
            guard UnixSocket.send(descriptor, bytes) else { return nil }
            shutdown(descriptor, SHUT_WR)
            return UnixSocket.readToEnd(descriptor, limit: 1024 * 1024)
        }.value
    }

    /// A socket at `path` made without the application: bound, and listening when asked to.
    private func bareSocket(at path: String, listening: Bool) throws -> Int32 {
        let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let size = MemoryLayout.size(ofValue: address.sun_path)
        _ = withUnsafeMutablePointer(to: &address.sun_path) { field in
            field.withMemoryRebound(to: CChar.self, capacity: size) { strlcpy($0, path, size) }
        }
        let bound = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
        }
        XCTAssertEqual(bound, 0, String(cString: strerror(errno)))
        if listening { XCTAssertEqual(listen(descriptor, 4), 0) }
        return descriptor
    }

    func testACallTravelsToTheApplicationAndItsReplyBack() async throws {
        let directory = try directory()
        var received: [MCPToolRequest] = []
        var reply = MCPToolReply.result(["revision": .integer(4), "blocks": .array([.object(["key": .string("claude")])])])
        let channel = MCPAppChannel(directory: directory) { request in
            received.append(request)
            return reply
        }
        XCTAssertEqual(channel.path, directory.appendingPathComponent("mcp.sock").path)
        XCTAssertFalse(channel.isListening)
        XCTAssertTrue(channel.start())
        XCTAssertTrue(channel.start(), "starting twice is starting once")
        XCTAssertTrue(channel.isListening)
        addTeardownBlock { await channel.stop() }
        let path = channel.path

        let read = MCPToolRequest(tool: "get_hud")
        let answered = await call(read, at: path)
        XCTAssertEqual(answered, .reply(reply))

        reply = .failure("The layout changed since revision 3. Call get_hud and send the layout again.")
        let layout = MCPToolRequest(tool: "draft_hud", arguments: [
            "order": .array([.string("codex"), .string("claude")]), "revision": .integer(3),
            "blocks": .object(["claude": .object(["widget": .string("ring"), "hidden": .bool(false)])]),
        ])
        let refused = await call(layout, at: path)
        XCTAssertEqual(refused, .reply(reply), "a refusal travels back as its sentence")
        XCTAssertEqual(received, [read, layout], "each request reaches the handler once, as it was sent")

        channel.stop()
        XCTAssertFalse(channel.isListening)
        XCTAssertFalse(FileManager.default.fileExists(atPath: path), "the application removes the socket it made")
        let gone = await Task.detached { MCPAppClient.call(read, socket: path) }.value
        XCTAssertEqual(gone, .notRunning)
    }

    /// A picture is larger than any request, and larger than every reply the channel carried before there were
    /// pictures: it has to come back whole, with the result beside it.
    func testAReplyWithAPictureTravelsBackWhole() async throws {
        var bytes = [UInt8](repeating: 0, count: 3 * 1024 * 1024)
        arc4random_buf(&bytes, bytes.count)
        let png = Data(bytes)
        let reply = MCPToolReply.result(["width": .integer(540), "height": .integer(1_200), "scrolls": .bool(false)],
                                        images: [MCPImage(mimeType: "image/png", data: png)])
        XCTAssertGreaterThan(reply.message.count, 4 * 1024 * 1024)
        let channel = MCPAppChannel(directory: try directory()) { _ in reply }
        XCTAssertTrue(channel.start())
        addTeardownBlock { await channel.stop() }
        let answered = await call(MCPToolRequest(tool: MCPTools.getHUDScreenshot), at: channel.path)
        XCTAssertEqual(answered, .reply(reply))
        XCTAssertLessThanOrEqual(MCPToolRequest.sizeLimit, 1024 * 1024, "only replies carry pictures; a request is no larger than it was")
    }

    func testASecondChannelLeavesTheSocketToTheOneThatServesIt() async throws {
        let directory = try directory()
        let first = MCPAppChannel(directory: directory) { _ in .result(["served": .string("first")]) }
        let second = MCPAppChannel(directory: directory) { _ in .result(["served": .string("second")]) }
        XCTAssertTrue(first.start())
        addTeardownBlock { await first.stop() }
        XCTAssertFalse(second.start(), "what another copy of the application opened beside the first one finds")
        XCTAssertFalse(second.isListening)
        second.stop()
        let answered = await call(MCPToolRequest(tool: "get_hud"), at: first.path)
        XCTAssertEqual(answered, .reply(.result(["served": .string("first")])), "stopping the second removed nothing")
    }

    func testAMessageThatIsNoRequestIsAnsweredWithTheReason() async throws {
        var handled = 0
        let channel = MCPAppChannel(directory: try directory()) { _ in
            handled += 1
            return .result([:])
        }
        XCTAssertTrue(channel.start())
        addTeardownBlock { await channel.stop() }
        let path = channel.path
        let first = await call(MCPToolRequest(tool: "get_hud"), at: path)
        XCTAssertEqual(first, .reply(.result([:])), "the channel is up")

        let unreadable = ["not json": "The request is not a JSON object.",
                          #"["get_hud"]"#: "The request is not a JSON object.",
                          #"{"tool":"get_hud","arguments":{"#: "The request is not a JSON object.",
                          #"{"arguments":{}}"#: "The request names no tool.",
                          #"{"tool":7}"#: "The request names no tool.",
                          #"{"tool":"get_hud","arguments":[1]}"#: "The request's arguments are not a JSON object."]
        for (message, reason) in unreadable {
            let returned = await exchange(Data(message.utf8), at: path)
            let answer = try XCTUnwrap(returned, message)
            XCTAssertEqual(try JSONValue.parse(answer), .object(["ok": .bool(false), "error": .string(reason)]), message)
            XCTAssertEqual(MCPToolReply.read(answer), .failure(reason))
        }
        XCTAssertEqual(handled, 1, "none of them reaches the handler")
    }

    func testALineTheProcessReadsReachesTheApplicationAndItsAnswerIsWrittenBack() async throws {
        var received: [MCPToolRequest] = []
        let channel = MCPAppChannel(directory: try directory()) { request in
            received.append(request)
            return request.tool == MCPTools.getHUD ? .result(["revision": .integer(7)]) : .failure("There is no draft to drop.")
        }
        XCTAssertTrue(channel.start())
        addTeardownBlock { await channel.stop() }
        let path = channel.path
        let first = await call(MCPToolRequest(tool: "get_hud"), at: path)
        XCTAssertEqual(first, .reply(.result(["revision": .integer(7)])), "the channel is up")
        received.removeAll()

        let input = [#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}"#,
                     #"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                     #"{"jsonrpc":"2.0","id":"read","method":"tools/call","params":{"name":"get_hud","arguments":{}}}"#,
                     #"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"discard_hud_draft"}}"#].joined(separator: "\n") + "\n"
        // The process as a client runs it, with this test's socket in place of the data directory's.
        let written = await Task.detached { () -> String in
            var inbound: [Int32] = [0, 0], outbound: [Int32] = [0, 0]
            guard pipe(&inbound) == 0, pipe(&outbound) == 0 else { return "" }
            let bytes = Array(input.utf8)
            _ = write(inbound[1], bytes, bytes.count)
            close(inbound[1])
            let server = MCPServer(version: "1.2.3") { MCPAppClient.call($0, socket: path, timeout: 5) }
            _ = MCPStdio.run(server, input: inbound[0], output: outbound[1])
            close(inbound[0])
            close(outbound[1])
            let output = FileHandle(fileDescriptor: outbound[0], closeOnDealloc: true).readDataToEndOfFile()
            return String(decoding: output, as: UTF8.self)
        }.value
        let messages = try written.split(separator: "\n").map { try JSONValue.parse(Data($0.utf8)) }
        XCTAssertEqual(messages.map { $0["id"] }, [.integer(0), .string("read"), .integer(2)])
        XCTAssertEqual(messages[1]["result"].blockText, #"{"revision":7}"#)
        XCTAssertEqual(messages[1]["result"]["isError"], .bool(false))
        XCTAssertEqual(messages[2]["result"].blockText, "There is no draft to drop.")
        XCTAssertEqual(messages[2]["result"]["isError"], .bool(true))
        XCTAssertEqual(received, [MCPToolRequest(tool: "get_hud"), MCPToolRequest(tool: "discard_hud_draft")])
    }

    func testWithNoApplicationServingTheSocketACallSaysNotRunning() throws {
        let path = MCPAppChannel.socketPath(in: try directory())
        let request = MCPToolRequest(tool: "get_hud")
        XCTAssertEqual(MCPAppClient.call(request, socket: path), .notRunning, "no socket")
        try Data().write(to: URL(fileURLWithPath: path))
        XCTAssertEqual(MCPAppClient.call(request, socket: path), .notRunning, "a file that is no socket")
        try FileManager.default.removeItem(atPath: path)
        // What an application that crashed leaves behind: its socket, with nobody accepting.
        close(try bareSocket(at: path, listening: false))
        XCTAssertTrue(FileManager.default.fileExists(atPath: path))
        XCTAssertEqual(MCPAppClient.call(request, socket: path), .notRunning, "a socket nothing accepts on")
        XCTAssertEqual(MCPAppClient.call(request, socket: path + String(repeating: "x", count: 120)), .notRunning)
    }

    func testAnApplicationThatTakesTheCallAndSaysNothingIsUnanswered() throws {
        let path = MCPAppChannel.socketPath(in: try directory())
        // Connections queue up on a listening socket whether or not anything accepts them.
        let wedged = try bareSocket(at: path, listening: true)
        defer { close(wedged) }
        let started = Date()
        XCTAssertEqual(MCPAppClient.call(MCPToolRequest(tool: "get_hud"), socket: path, timeout: 0.2), .unanswered)
        XCTAssertLessThan(Date().timeIntervalSince(started), 5, "the wait ends with the time allowed, here a fifth of a second")
        XCTAssertEqual(MCPAppClient.timeout, 10)
    }

    func testACallLargerThanTheApplicationReadsIsRefusedBeforeItIsSent() throws {
        let path = MCPAppChannel.socketPath(in: try directory())
        let listening = try bareSocket(at: path, listening: true)
        defer { close(listening) }
        let rows = JSONValue.array([.string(String(repeating: "x", count: MCPToolRequest.sizeLimit))])
        let large = MCPToolRequest(tool: "draft_hud", arguments: ["order": rows])
        XCTAssertEqual(MCPAppClient.call(large, socket: path, timeout: 0.2),
                       .reply(.failure("The call is larger than Agent HUD reads. Send less in one call.")),
                       "the application would drop it unanswered, so the process says why itself")
    }

    func testRequestsAndRepliesReadBackWhatWasWritten() throws {
        let request = MCPToolRequest(tool: "draft_hud", arguments: ["from": .string("previous"), "revision": .integer(9)])
        XCTAssertEqual(try JSONValue.parse(request.message),
                       .object(["tool": .string("draft_hud"), "arguments": .object(["from": .string("previous"), "revision": .integer(9)])]))
        XCTAssertEqual(try MCPToolRequest.read(request.message), request)
        XCTAssertEqual(try MCPToolRequest.read(Data(#"{"tool":"get_hud"}"#.utf8)), MCPToolRequest(tool: "get_hud"))

        let result = MCPToolReply.result(["revision": .integer(9), "draft": .bool(true)])
        XCTAssertEqual(try JSONValue.parse(result.message), .object(["ok": .bool(true), "result": .object(["revision": .integer(9), "draft": .bool(true)])]))
        XCTAssertEqual(MCPToolReply.read(result.message), result)
        let failure = MCPToolReply.failure("No block has the key \"claud\".")
        XCTAssertEqual(try JSONValue.parse(failure.message), .object(["ok": .bool(false), "error": .string("No block has the key \"claud\".")]))
        XCTAssertEqual(MCPToolReply.read(failure.message), failure)
        for junk in ["", "{}", #"{"ok":true}"#, #"{"ok":true,"result":[1]}"#, #"{"ok":false}"#, #"{"ok":"yes","result":{}}"#, "[]"] {
            XCTAssertNil(MCPToolReply.read(Data(junk.utf8)), junk)
        }
        let picture = MCPToolReply.result(["height": .integer(741)], images: [MCPImage(mimeType: "image/png", data: Data([1, 2, 3]))])
        XCTAssertEqual(try JSONValue.parse(picture.message), .object([
            "ok": .bool(true), "result": .object(["height": .integer(741)]),
            "images": .array([.object(["mimeType": .string("image/png"), "data": .string("AQID")])]),
        ]), "the pictures travel beside the result")
        XCTAssertEqual(MCPToolReply.read(picture.message), picture)
        XCTAssertEqual(MCPToolReply.read(Data(#"{"ok":true,"result":{"a":1},"images":null}"#.utf8)), .result(["a": .integer(1)]))
        XCTAssertEqual(MCPToolReply.read(Data(#"{"ok":true,"result":{},"images":[]}"#.utf8)), .result([:]))
        for broken in [#"{"ok":true,"result":{},"images":{}}"#, #"{"ok":true,"result":{},"images":[{"data":"AQID"}]}"#,
                       #"{"ok":true,"result":{},"images":[{"mimeType":"image/png","data":"not base64!"}]}"#,
                       #"{"ok":true,"result":{},"images":[{"mimeType":"","data":"AQID"}]}"#,
                       #"{"ok":true,"result":{},"images":["AQID"]}"#] {
            XCTAssertNil(MCPToolReply.read(Data(broken.utf8)), broken)
        }
        let unwritable = MCPToolReply.result(["share": .number(.infinity)])
        XCTAssertEqual(MCPToolReply.read(unwritable.message), .failure("The result could not be written as JSON."),
                       "a result JSON cannot hold still reaches the agent, as the failure it is")
    }

    func testTheSocketIsNamedForTheProtocolInTheDataDirectory() {
        XCTAssertEqual(MCPAppChannel.socketPath(), AppSupport.directory.appendingPathComponent("mcp.sock").path)
        XCTAssertEqual(MCPAppChannel.socketPath(in: URL(fileURLWithPath: "/tmp/hud")), "/tmp/hud/mcp.sock")
    }
}
