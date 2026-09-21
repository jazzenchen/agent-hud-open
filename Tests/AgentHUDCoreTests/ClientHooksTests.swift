import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// Switching client hooks off takes this installation's handlers out of the clients' files and adds nothing back.
final class ClientHooksTests: XCTestCase, @unchecked Sendable {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private func commands(_ url: URL) throws -> [String] {
        let hooks = try ProviderJSON.read(Data(contentsOf: url))["hooks"]
        return (hooks.objectValue ?? [:]).values.flatMap { $0.arrayValue ?? [] }
            .flatMap { $0["hooks"].arrayValue ?? [] }.compactMap { $0["command"].stringValue }.sorted()
    }

    func testTheSwitchIsOnUntilTurnedOff() throws {
        XCTAssertTrue(Settings().clientHooks)
        XCTAssertTrue(try JSONDecoder().decode(Settings.self, from: Data(#"{"glowRange":12}"#.utf8)).clientHooks)
        let off = Settings().with { $0.clientHooks = false }
        XCTAssertFalse(try JSONDecoder().decode(Settings.self, from: JSONEncoder().encode(off)).clientHooks)
    }

    func testTurningHooksOffRemovesOurHandlersAndLeavesTheRest() throws {
        let home = try directory()
        for folder in [".claude/projects", ".qwen/projects"] {
            try FileManager.default.createDirectory(at: home.appendingPathComponent(folder), withIntermediateDirectories: true)
        }
        let claude = home.appendingPathComponent(".claude/settings.json")
        try JSONSerialization.data(withJSONObject: ["model": "opus", "hooks": [
            "PreToolUse": [["matcher": "Bash", "hooks": [["type": "command", "command": "~/bin/lint"]]]]]]).write(to: claude)

        let executable = URL(fileURLWithPath: "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD")
        SessionObservers.configure(executable: executable, enabled: true, home: home)
        let qwen = home.appendingPathComponent(".qwen/settings.json")
        XCTAssertEqual(try commands(claude).count, 3, "approval and notification hooks beside the user's own")
        XCTAssertEqual(try commands(qwen).count, 2, "approval and stop hooks")

        SessionObservers.configure(executable: executable, enabled: false, home: home)
        XCTAssertEqual(try commands(claude), ["~/bin/lint"])
        XCTAssertEqual(try ProviderJSON.read(Data(contentsOf: claude))["model"].stringValue, "opus")
        XCTAssertEqual(try commands(qwen), [])
    }

    func testRemovingNeverCreatesAFileTheClientDidNotHave() throws {
        let home = try directory(), executable = URL(fileURLWithPath: "/tmp/hud")
        for source in CompletionHooks.Source.allCases {
            try CompletionHooks.configure(source, enabled: false, executable: executable, home: home)
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.configuration(home: home).path), source.rawValue)
        }
        for source in PermissionHooks.Source.allCases {
            try PermissionHooks.configure(source, enabled: false, executable: executable, home: home)
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.configuration(home: home).path), source.rawValue)
        }
    }
}
