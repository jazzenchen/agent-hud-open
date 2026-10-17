import Foundation
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// The tool channel opens and closes with the switch in Settings → Experimental; a demo with `--demo-tools` serves it
/// whatever the switch says, and one without never does.
@MainActor
final class AgentToolsChannelTests: XCTestCase {
    /// A short path, since a socket's address has about a hundred bytes.
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("tools-" + UUID().uuidString.prefix(8))
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private func settings() throws -> SettingsStore {
        let suite = "app.agenthud.tests.agent-tools.\(UUID().uuidString)"
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        return SettingsStore(defaults: try XCTUnwrap(UserDefaults(suiteName: suite)))
    }

    private func until(_ reached: () -> Bool) async {
        let deadline = Date().addingTimeInterval(5)
        while !reached(), Date() < deadline { try? await Task.sleep(for: .milliseconds(10)) }
    }

    /// A channel that follows the switch, beside the one under test: once it has taken a change, the other has had the
    /// same change handed to it, so what that one did not do it will not do.
    private func control(_ settings: SettingsStore) throws -> MCPAppChannel {
        let channel = MCPAppChannel(directory: try directory()) { _ in .result([:]) }
        let tools = AgentToolsChannel(channel: channel, settings: settings, mode: .whileSwitchedOn)
        addTeardownBlock { await tools.stop() }
        tools.start()
        return channel
    }

    /// What a client's call gets, made as the `--mcp` process makes it, off the main actor the channel answers on.
    private func call(_ path: String) async -> MCPAppClient.Answer {
        await Task.detached { MCPAppClient.call(MCPToolRequest(tool: MCPTools.getHUD), socket: path, timeout: 5) }.value
    }

    func testTheChannelOpensAndClosesWithTheSwitch() async throws {
        let settings = try settings()
        let channel = MCPAppChannel(directory: try directory()) { _ in .result(["revision": .integer(2)]) }
        let tools = AgentToolsChannel(channel: channel, settings: settings, mode: .whileSwitchedOn)
        addTeardownBlock { await tools.stop() }
        tools.start()
        XCTAssertFalse(channel.isListening, "off until a person turns it on")
        XCTAssertFalse(FileManager.default.fileExists(atPath: channel.path))
        let closed = await call(channel.path)
        XCTAssertEqual(closed, .notRunning)

        settings.update { $0.hudTools = true }
        await until { channel.isListening }
        XCTAssertTrue(channel.isListening)
        var answer = MCPAppClient.Answer.notRunning
        for _ in 0..<300 where answer == .notRunning {
            answer = await call(channel.path)
            if answer == .notRunning { try? await Task.sleep(for: .milliseconds(10)) }
        }
        XCTAssertEqual(answer, .reply(.result(["revision": .integer(2)])))

        settings.update { $0.hudTools = false }
        await until { !channel.isListening }
        XCTAssertFalse(channel.isListening)
        XCTAssertFalse(FileManager.default.fileExists(atPath: channel.path), "the socket goes with the switch")
        let after = await call(channel.path)
        XCTAssertEqual(after, .notRunning)

        settings.update { $0.hudTools = true }
        await until { channel.isListening }
        XCTAssertTrue(channel.isListening, "and comes back with it")
        tools.stop()
        XCTAssertFalse(channel.isListening)
        let control = try control(settings)
        settings.update { $0.hudTools = false }
        await until { !control.isListening }
        settings.update { $0.hudTools = true }
        await until { control.isListening }
        XCTAssertTrue(control.isListening)
        XCTAssertFalse(channel.isListening, "a channel stopped as the application quits stays closed")
    }

    func testADemoWithToolsServesWhateverTheSwitchSays() async throws {
        let settings = try settings()
        let channel = MCPAppChannel(directory: try directory()) { _ in .result([:]) }
        let tools = AgentToolsChannel(channel: channel, settings: settings, mode: .always)
        addTeardownBlock { await tools.stop() }
        tools.start()
        XCTAssertFalse(settings.settings.hudTools)
        XCTAssertTrue(channel.isListening)
        let control = try control(settings)
        settings.update { $0.hudTools = true }
        await until { control.isListening }
        settings.update { $0.hudTools = false }
        await until { !control.isListening }
        XCTAssertFalse(control.isListening)
        XCTAssertTrue(channel.isListening)
    }

    func testADemoWithoutToolsNeverServes() async throws {
        let settings = try settings()
        settings.update { $0.hudTools = true }
        let channel = MCPAppChannel(directory: try directory()) { _ in .result([:]) }
        let tools = AgentToolsChannel(channel: channel, settings: settings, mode: .never)
        addTeardownBlock { await tools.stop() }
        tools.start()
        XCTAssertFalse(channel.isListening, "a demo leaves the socket to the real copy, whatever its own switch says")
        let control = try control(settings)
        settings.update { $0.hudTools = false }
        await until { !control.isListening }
        settings.update { $0.hudTools = true }
        await until { control.isListening }
        XCTAssertTrue(control.isListening)
        XCTAssertFalse(channel.isListening, "nor after the switch moves")
    }
}
