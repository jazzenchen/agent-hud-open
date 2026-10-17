import Foundation
import XCTest
@testable import AgentHUDCore

/// A call that finds no socket served: the `--mcp` process tells an application that runs with its agent tools switched
/// off from none running at all, by the instance lock, which it reads without taking.
final class MCPToolsOffTests: XCTestCase, @unchecked Sendable {
    /// A short path: the socket's address has about a hundred bytes.
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("off-" + UUID().uuidString.prefix(8))
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private let request = MCPToolRequest(tool: MCPTools.getHUD)

    func testAnUnservedSocketIsOffWhileTheApplicationHoldsTheLockAndNotRunningOtherwise() throws {
        let directory = try directory()
        let socket = MCPAppChannel.socketPath(in: directory)
        let lock = directory.appendingPathComponent("instance.lock")
        func answer(_ name: String = "Agent HUD Open") -> MCPAppClient.Answer {
            MCPAppClient.call(request, socket: socket, lock: lock, dataDirectoryName: name, timeout: 1)
        }
        XCTAssertEqual(answer(), .notRunning, "no lock and no socket")
        XCTAssertNil(InstanceLock.runningApplication(at: lock))

        // A build outside a bundle keeps its data where the open application does.
        var running: InstanceLock.Claim? = InstanceLock.claim(at: lock, executable: URL(fileURLWithPath: "/tmp/hud/.build/debug/AgentHUDOpen"))
        guard case .acquired = running else { return XCTFail("the fake application holds the lock") }
        XCTAssertEqual(InstanceLock.runningApplication(at: lock)?.path, "/tmp/hud/.build/debug/AgentHUDOpen")
        XCTAssertEqual(answer(), .off, "the application runs and serves no socket")
        XCTAssertEqual(answer("Agent HUD"), .notRunning, "a copy of another application, with its own data directory, does not count")
        guard case .held = InstanceLock.claim(at: lock) else {
            return XCTFail("asking never took or freed the lock: a second copy is still turned away")
        }

        running = nil
        XCTAssertNotNil(InstanceLock.holder(at: lock), "the name of the copy that quit stays in the file")
        XCTAssertNil(InstanceLock.runningApplication(at: lock), "but nobody holds the lock")
        XCTAssertEqual(answer(), .notRunning)
        guard case .acquired = InstanceLock.claim(at: lock) else {
            return XCTFail("asking holds nothing: a copy that starts next is not turned away")
        }
    }

    func testABundledHolderCountsByTheDataDirectoryItsBundleNames() throws {
        let directory = try directory()
        let app = directory.appendingPathComponent("Agent HUD.app")
        let executables = app.appendingPathComponent("Contents/MacOS", isDirectory: true)
        try FileManager.default.createDirectory(at: executables, withIntermediateDirectories: true)
        let info: [String: Any] = ["CFBundleIdentifier": "app.agenthud.tests.off.\(UUID().uuidString)", "CFBundleExecutable": "Agent HUD",
                                   "CFBundlePackageType": "APPL", "AgentHUDDataDirectory": "Agent HUD"]
        try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
            .write(to: app.appendingPathComponent("Contents/Info.plist"))
        let lock = directory.appendingPathComponent("instance.lock")
        let running = InstanceLock.claim(at: lock, executable: executables.appendingPathComponent("Agent HUD"))
        guard case .acquired = running else { return XCTFail("the fake application holds the lock") }
        withExtendedLifetime(running) {
            XCTAssertEqual(InstanceLock.runningApplication(at: lock)?.lastPathComponent, "Agent HUD.app")
            XCTAssertTrue(MCPAppClient.applicationRuns(lock: lock, dataDirectoryName: "Agent HUD"))
            XCTAssertFalse(MCPAppClient.applicationRuns(lock: lock, dataDirectoryName: "Agent HUD Open"))
        }
    }

    /// The lock decides nothing while the socket is served: the call goes through.
    @MainActor
    func testAServedSocketAnswersWhateverTheLockSays() async throws {
        let directory = try directory()
        let lock = directory.appendingPathComponent("instance.lock")
        let running = InstanceLock.claim(at: lock, executable: URL(fileURLWithPath: "/tmp/hud/AgentHUDOpen"))
        guard case .acquired = running else { return XCTFail("the fake application holds the lock") }
        let channel = MCPAppChannel(directory: directory) { _ in .result(["revision": .integer(1)]) }
        XCTAssertTrue(channel.start())
        addTeardownBlock { await channel.stop() }
        let socket = channel.path, request = request
        var answer = MCPAppClient.Answer.off
        // The listener comes up on the main queue, so the first calls may find nothing yet.
        for _ in 0..<300 {
            answer = await Task.detached {
                MCPAppClient.call(request, socket: socket, lock: lock, dataDirectoryName: "Agent HUD Open", timeout: 5)
            }.value
            if case .reply = answer { break }
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(answer, .reply(.result(["revision": .integer(1)])))
        withExtendedLifetime(running) {}
    }

    func testTheServerTellsTheAgentWhereToTurnTheToolsOn() throws {
        let probe = MCPServerProbe(answer: .off)
        let line = #"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_hud","arguments":{}}}"#
        let response = try probe.response(to: line)
        XCTAssertEqual(response["result"].blockText, "Agent HUD's agent tools are off. Turn them on in Settings › Experimental.")
        XCTAssertEqual(response["result"]["isError"], .bool(true))
        probe.answer = .notRunning
        XCTAssertEqual(try probe.response(to: line)["result"].blockText, "Agent HUD is not running. Open Agent HUD and try again.",
                       "with no application at all the sentence is the one it always was")
    }
}
