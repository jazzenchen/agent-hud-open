import AgentHUDSupport
import Darwin
import Foundation
import XCTest
@testable import AgentHUDCore

/// The `--mcp` process: which arguments start it, how its input is cut into messages, and when it ends.
final class MCPStdioTests: XCTestCase {
    private func pipe() throws -> (read: Int32, write: Int32) {
        var ends: [Int32] = [0, 0]
        XCTAssertEqual(Darwin.pipe(&ends), 0)
        return (ends[0], ends[1])
    }

    /// Runs the loop over `input` as a client would write it before closing its end, and returns the exit status
    /// with the lines written back.
    private func run(_ input: String, probe: MCPServerProbe = MCPServerProbe()) throws -> (status: Int32, lines: [String]) {
        let inbound = try pipe(), outbound = try pipe()
        let bytes = Array(input.utf8)
        XCTAssertEqual(write(inbound.write, bytes, bytes.count), bytes.count)
        close(inbound.write)
        let status = MCPStdio.run(probe.server, input: inbound.read, output: outbound.write)
        close(inbound.read)
        close(outbound.write)
        let output = FileHandle(fileDescriptor: outbound.read, closeOnDealloc: true).readDataToEndOfFile()
        let text = String(decoding: output, as: UTF8.self)
        XCTAssertTrue(text.isEmpty || text.hasSuffix("\n"), "every message ends its line")
        return (status, text.split(separator: "\n", omittingEmptySubsequences: false).dropLast().map(String.init))
    }

    func testOnlyTheSwitchAloneAsksForTheServer() {
        XCTAssertTrue(MCPStdio.isRequested(by: ["/Applications/Agent HUD Open.app/Contents/MacOS/Agent HUD Open", "--mcp"]))
        for arguments in [["/tmp/hud"], ["--mcp"], ["/tmp/hud", "--mcp", "claude"], ["/tmp/hud", "--demo", "--mcp"],
                          ["/tmp/hud", "--MCP"], ["/tmp/hud", "mcp"], []] {
            XCTAssertFalse(MCPStdio.isRequested(by: arguments), "\(arguments)")
        }
    }

    func testArgumentsThatNameNoCommandStillStartTheApplication() {
        for arguments in [["/tmp/hud"], ["/tmp/hud", "--demo"], ["/tmp/hud", "--show-settings", "--demo"],
                          ["/tmp/hud", "--mcp", "claude"], ["/tmp/hud", "--demo", "--mcp"], ["/tmp/hud", "--open-panel"],
                          ["/tmp/hud", "--demo", "--permission-hook", "claude"], ["/tmp/hud", "--snapshot", "/tmp/stills"],
                          ["/tmp/hud", "--lang", "en", "--demo"]] {
            XCTAssertNil(HookEntry.handle(arguments: arguments), "\(arguments)")
        }
    }

    /// A hook names one executable path, and a copy of another build may sit at it: a hook's flag or a client that copy
    /// does not know ends quietly, where starting whole would rewrite every client's hooks to its own path.
    func testAHookFlagOrClientThisBuildDoesNotKnowEndsQuietly() throws {
        let unknown = [["/tmp/hud", "--permission-hook", "nobody"], ["/tmp/hud", "--completion-hook", "nobody"],
                       ["/tmp/hud", "--attention-hook", "codex"], ["/tmp/hud", "--session-origin-hook", "nobody"],
                       ["/tmp/hud", "--later-hook", "claude"],
                       ["/tmp/hud", "--later-hook"], ["/tmp/hud", "--permission-hook"],
                       ["/tmp/hud", "--permission-hook", "claude", "--later"]]
        let outbound = try pipe()
        // What this process had printed and not yet written must not count as the commands' output.
        fflush(nil)
        let statuses: [Int32?] = redirecting(STDOUT_FILENO, to: outbound.write) {
            let statuses = unknown.map { HookEntry.handle(arguments: $0) }
            fflush(nil)
            return statuses
        }
        close(outbound.write)
        let output = FileHandle(fileDescriptor: outbound.read, closeOnDealloc: true).readDataToEndOfFile()

        for (arguments, status) in zip(unknown, statuses) { XCTAssertEqual(status, 0, "\(arguments)") }
        XCTAssertEqual(String(decoding: output, as: UTF8.self), "", "prints nothing")
        XCTAssertTrue(HookEntry.isHookFlag("--compaction-hook"))
        // An install command is typed by a person, not run by a client: a client it does not know is an error to say.
        XCTAssertEqual(HookEntry.handle(arguments: ["/tmp/hud", "--install-completion-hook", "nobody"]), 1)
        XCTAssertFalse(HookEntry.isHookFlag("--hooks"))
        XCTAssertFalse(HookEntry.isHookFlag("permission-hook"))
    }

    /// Points the process's own descriptor `target` at `replacement` while `body` runs, and back afterwards.
    private func redirecting<T>(_ target: Int32, to replacement: Int32, _ body: () throws -> T) rethrows -> T {
        let kept = dup(target)
        dup2(replacement, target)
        defer {
            if kept >= 0 {
                dup2(kept, target)
                close(kept)
            } else {
                close(target)
            }
        }
        return try body()
    }

    func testTheSwitchRunsTheServerOnStandardInputAndOutput() throws {
        let inbound = try pipe(), outbound = try pipe()
        // No tool is called: the command meets the application in the real data directory.
        let input = [#"{"jsonrpc":"2.0","id":"open","method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}"#,
                     #"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                     #"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#].joined(separator: "\n") + "\n"
        let bytes = Array(input.utf8)
        XCTAssertEqual(write(inbound.write, bytes, bytes.count), bytes.count)
        close(inbound.write)
        // What this process had printed and not yet written must not land among the answers.
        fflush(nil)
        let found = signal(SIGPIPE, SIG_DFL)
        let status = redirecting(STDIN_FILENO, to: inbound.read) {
            redirecting(STDOUT_FILENO, to: outbound.write) { HookEntry.handle(arguments: ["/tmp/hud", "--mcp"]) }
        }
        let left = signal(SIGPIPE, found)
        close(inbound.read)
        close(outbound.write)
        let output = FileHandle(fileDescriptor: outbound.read, closeOnDealloc: true).readDataToEndOfFile()

        XCTAssertEqual(status, 0, "the command ran, and ended with its input")
        XCTAssertEqual(unsafeBitCast(left, to: Int.self), unsafeBitCast(SIG_IGN, to: Int.self),
                       "a client that went away ends the process through a failed write, not a signal")
        let text = String(decoding: output, as: UTF8.self)
        XCTAssertTrue(text.hasSuffix("\n"))
        let messages = try text.split(separator: "\n").map { try JSONValue.parse(Data($0.utf8)) }
        XCTAssertEqual(messages.map { $0["id"] }, [.string("open"), .integer(2)], "standard output carries the answers and nothing else")
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
        XCTAssertEqual(messages.first?["result"]["serverInfo"],
                       .object(["name": .string("agent-hud"), "title": .string("Agent HUD"), "version": .string(version)]),
                       "the server's version is the bundle's, and 0 for an executable outside one")
        XCTAssertEqual(messages.last?["result"]["tools"].arrayValue?.count, 5)
    }

    func testInputIsCutIntoLinesHoweverTheReadsFall() {
        var buffer = MCPLineBuffer()
        XCTAssertEqual(buffer.take(Data(#"{"jsonrpc":"#.utf8)), [])
        XCTAssertEqual(buffer.take(Data(#""2.0"}"#.utf8)), [])
        let lines = buffer.take(Data("\nsecond\r\n\nthi".utf8))
        XCTAssertEqual(lines.map { String(decoding: $0, as: UTF8.self) }, [#"{"jsonrpc":"2.0"}"#, "second", ""],
                       "a carriage return before the line end is not part of the message")
        XCTAssertEqual(buffer.take(Data("rd\n".utf8)), [Data("third".utf8)])
        XCTAssertNil(buffer.rest(), "nothing is left after a line end")
        XCTAssertEqual(buffer.take(Data("no line end".utf8)), [])
        XCTAssertEqual(buffer.rest(), Data("no line end".utf8))
        XCTAssertNil(buffer.rest())
    }

    func testALineLongerThanAnyMessageKeepsOnlyItsStart() {
        var buffer = MCPLineBuffer()
        buffer.limit = 8
        XCTAssertEqual(buffer.take(Data("0123456789abcdef\nnext\n".utf8)), [Data("01234567".utf8), Data("next".utf8)])
        XCTAssertEqual(MCPLineBuffer().limit, 4 * 1024 * 1024)
    }

    func testTheLoopAnswersEachRequestInOrderAndEndsWithItsInput() throws {
        let probe = MCPServerProbe(answer: .reply(.result(["revision": .integer(1)])))
        let input = [#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}"#,
                     #"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                     "",
                     #"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_hud","arguments":{}}}"#,
                     "not json",
                     #"{"jsonrpc":"2.0","id":"last","method":"ping"}"#].joined(separator: "\n")
        let (status, lines) = try run(input, probe: probe)
        XCTAssertEqual(status, 0)
        let messages = try lines.map { try JSONValue.parse(Data($0.utf8)) }
        XCTAssertEqual(messages.map { $0.objectValue?["id"] }, [.integer(0), .integer(1), .null, .string("last")],
                       "one response per request, in the order they came; the last line needs no line end")
        XCTAssertEqual(messages[1]["result"].blockText, #"{"revision":1}"#)
        XCTAssertEqual(messages[2]["error"]["code"], .integer(-32700))
        XCTAssertEqual(probe.calls, [MCPToolRequest(tool: "get_hud")])
    }

    func testInputThatEndsAtOnceEndsTheProcessWithNothingWritten() throws {
        let (status, lines) = try run("")
        XCTAssertEqual(status, 0)
        XCTAssertEqual(lines, [])
        let quiet = try run(#"{"jsonrpc":"2.0","method":"notifications/initialized"}"# + "\n\n")
        XCTAssertEqual(quiet.lines, [], "nothing but protocol messages is ever written")
    }

    func testTheLoopEndsOnceNobodyReadsItsAnswers() throws {
        let inbound = try pipe(), outbound = try pipe()
        // The process ignores the signal a closed pipe raises; here only this descriptor does.
        XCTAssertEqual(fcntl(outbound.write, F_SETNOSIGPIPE, 1), 0)
        close(outbound.read)
        let bytes = Array((#"{"jsonrpc":"2.0","id":1,"method":"ping"}"# + "\n" + #"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_hud"}}"# + "\n").utf8)
        XCTAssertEqual(write(inbound.write, bytes, bytes.count), bytes.count)
        defer { close(inbound.write); close(inbound.read); close(outbound.write) }
        let probe = MCPServerProbe()
        // The input is still open: only the failed write can end the loop.
        XCTAssertEqual(MCPStdio.run(probe.server, input: inbound.read, output: outbound.write), 0)
        XCTAssertTrue(probe.calls.isEmpty, "no call is made for a client that is gone")
    }
}
