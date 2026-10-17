import Foundation
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// A Mac a test makes up: which clients' commands it has, which configurations hold the server, and every command
/// run and text copied. A command run changes the configuration as the real client would, unless told to fail.
final class FakeClientMac: @unchecked Sendable {
    private let lock = NSLock()
    private var _servers: Set<MCPClient>
    private var _runs: [(URL, [String])] = []
    private var _copied: [String] = []
    var installed: [MCPClient: URL]
    var executable: URL?
    /// What the next command reports; it changes nothing unless it succeeds.
    var result = MCPCommandResult(status: 0)
    /// Whether a successful command changes the configuration.
    var takesEffect = true
    /// A command waits while this is true.
    var holding = false

    init(installed: [MCPClient: URL], servers: Set<MCPClient> = [],
         executable: URL? = URL(fileURLWithPath: "/Applications/Agent HUD Open.app/Contents/MacOS/Agent HUD Open")) {
        self.installed = installed
        _servers = servers
        self.executable = executable
    }

    var servers: Set<MCPClient> { lock.withLock { _servers } }
    var runs: [(URL, [String])] { lock.withLock { _runs } }
    var copied: [String] { lock.withLock { _copied } }

    @MainActor
    var environment: MCPClientSetup.Environment {
        MCPClientSetup.Environment(
            executable: executable,
            locate: { [self] in installed[$0] },
            hasServer: { [self] in servers.contains($0) },
            run: { [self] url, arguments in
                while lock.withLock({ holding }) { try? await Task.sleep(for: .milliseconds(5)) }
                return lock.withLock {
                    _runs.append((url, arguments))
                    guard result.succeeded, takesEffect,
                          let client = installed.first(where: { $0.value == url })?.key else { return result }
                    if arguments.contains("add") { _servers.insert(client) } else { _servers.remove(client) }
                    return result
                }
            },
            copy: { [self] text in lock.withLock { _copied.append(text) } })
    }

    func release() { lock.withLock { holding = false } }

    /// What a person's own run of a command in a terminal leaves in the configurations.
    func setServers(_ servers: Set<MCPClient>) { lock.withLock { _servers = servers } }
}

/// The clients row on Settings → Experimental: which clients it says have the server, what picking one in its menu
/// runs, and what it says after.
@MainActor
final class MCPClientSetupTests: XCTestCase {
    private let claude = URL(fileURLWithPath: "/Users/me/.local/bin/claude")
    private let codex = URL(fileURLWithPath: "/Applications/Codex.app/Contents/Resources/codex-cli/bin/codex")
    private let executable = "/Applications/Agent HUD Open.app/Contents/MacOS/Agent HUD Open"

    override func setUp() {
        super.setUp()
        L10n.setLanguage(.en)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    private func setup(_ mac: FakeClientMac) -> MCPClientSetup {
        let setup = MCPClientSetup(environment: mac.environment)
        setup.refresh()
        return setup
    }

    func testTheRowNamesEveryClientThatHasTheServerAndSaysWhatToDoNext() {
        let neither = setup(FakeClientMac(installed: [.codex: codex, .claudeCode: claude]))
        XCTAssertEqual(neither.clients, [.claudeCode, .codex], "the menu's order, whatever order they were found in")
        XCTAssertEqual(neither.title, "Not added to a client yet")
        XCTAssertEqual(neither.detail(toolsOn: true), "Pick a client from the menu to add it.")
        XCTAssertEqual(neither.detail(toolsOn: false), "Turn on the switch above, then pick a client to add it to.")

        let codexHasIt = setup(FakeClientMac(installed: [.codex: codex, .claudeCode: claude], servers: [.codex]))
        XCTAssertEqual(codexHasIt.title, "Added to Codex")
        XCTAssertEqual(codexHasIt.detail(toolsOn: true), "Start a new Codex session and ask it to change the HUD.")
        XCTAssertEqual(codexHasIt.detail(toolsOn: false), "Turn on the switch above so agents can use it.")

        let both = setup(FakeClientMac(installed: [.codex: codex, .claudeCode: claude], servers: [.codex, .claudeCode]))
        XCTAssertEqual(both.title, "Added to Claude Code and Codex")
        XCTAssertEqual(both.detail(toolsOn: true), "Start a new session and ask it to change the HUD.")

        let none = setup(FakeClientMac(installed: [:]))
        XCTAssertEqual(none.clients, [])
        XCTAssertEqual(none.detail(toolsOn: true), "No Claude Code or Codex on this Mac. Copy the settings for another client.")

        let mac = FakeClientMac(installed: [.claudeCode: claude, .codex: codex], servers: [.codex])
        let gone = setup(mac)
        mac.installed = [.claudeCode: claude]
        gone.refresh()
        XCTAssertEqual(gone.clients, [.claudeCode], "a client that is gone leaves the menu")
        XCTAssertEqual(gone.title, "Not added to a client yet")
    }

    func testPickingAClientRunsItsOwnCommandAndTheRowReadsItsConfigurationAfter() async {
        let mac = FakeClientMac(installed: [.claudeCode: claude, .codex: codex])
        let setup = setup(mac)
        XCTAssertEqual(setup.action(for: .claudeCode), .add(.claudeCode))

        await setup.perform(.add(.claudeCode))
        XCTAssertEqual(mac.runs.map(\.0), [claude])
        XCTAssertEqual(mac.runs.last?.1, ["mcp", "add", "--scope", "user", "agent-hud", "--", executable, "--mcp"])
        XCTAssertEqual(setup.title, "Added to Claude Code")
        XCTAssertEqual(setup.action(for: .claudeCode), .remove(.claudeCode), "checked now, so picking it again removes it")

        await setup.perform(.add(.codex))
        XCTAssertEqual(mac.runs.last?.0, codex)
        XCTAssertEqual(mac.runs.last?.1, ["mcp", "add", "agent-hud", "--", executable, "--mcp"])
        XCTAssertEqual(setup.title, "Added to Claude Code and Codex")

        await setup.perform(.remove(.claudeCode))
        XCTAssertEqual(mac.runs.last?.1, ["mcp", "remove", "--scope", "user", "agent-hud"])
        await setup.perform(.remove(.codex))
        XCTAssertEqual(mac.runs.last?.1, ["mcp", "remove", "agent-hud"])
        XCTAssertEqual(setup.title, "Not added to a client yet")

        await setup.perform(.remove(.codex))
        await setup.perform(.add(.claudeCode))
        await setup.perform(.add(.claudeCode))
        XCTAssertEqual(mac.runs.count, 5, "one command a pick, nothing on its own, and nothing the client's state rules out")
        XCTAssertTrue(mac.copied.isEmpty)
    }

    func testAFailureNamesTheClientAndOffersTheCommandForATerminal() async {
        let mac = FakeClientMac(installed: [.claudeCode: claude, .codex: codex], servers: [.codex])
        let setup = setup(mac)
        XCTAssertNil(setup.failedCommandLine, "no command to copy until one fails")
        mac.result = MCPCommandResult(status: 1, output: "\nerror: user config is locked\n    at write (cli.js:1:1)\n")
        await setup.perform(.add(.claudeCode))
        XCTAssertEqual(setup.detail(toolsOn: true), "Couldn't add to Claude Code: error: user config is locked")
        XCTAssertEqual(setup.title, "Added to Codex", "nothing was added")
        XCTAssertEqual(setup.failedCommandLine, "claude mcp add --scope user agent-hud -- '\(executable)' --mcp")

        mac.result = MCPCommandResult(status: nil)
        await setup.perform(.remove(.codex))
        XCTAssertEqual(setup.detail(toolsOn: true), "Couldn't remove from Codex: It did not finish in 15 seconds")
        XCTAssertEqual(setup.failedCommandLine, "codex mcp remove agent-hud")

        mac.result = MCPCommandResult(status: 0)
        mac.takesEffect = false
        await setup.perform(.add(.claudeCode))
        XCTAssertEqual(setup.detail(toolsOn: true), "Couldn't add to Claude Code: Claude Code's settings did not change",
                       "a command that says it worked is held to the configuration")

        setup.copyFailedCommand()
        XCTAssertEqual(mac.copied, ["claude mcp add --scope user agent-hud -- '\(executable)' --mcp"])
        XCTAssertEqual(setup.detail(toolsOn: true), "Command copied. Paste it into Terminal.")
        XCTAssertEqual(mac.runs.count, 3, "copying runs nothing")

        // The person runs it in a terminal and comes back: the row reads the configuration again.
        setup.refresh()
        XCTAssertEqual(setup.detail(toolsOn: true), "Command copied. Paste it into Terminal.", "nothing changed yet")
        mac.setServers([.codex, .claudeCode])
        setup.refresh()
        XCTAssertEqual(setup.title, "Added to Claude Code and Codex")
        XCTAssertEqual(setup.detail(toolsOn: true), "Start a new session and ask it to change the HUD.")
        XCTAssertNil(setup.failedCommandLine)
    }

    func testWhileACommandRunsTheRowWaitsForIt() async {
        let mac = FakeClientMac(installed: [.claudeCode: claude])
        mac.holding = true
        let setup = setup(mac)
        let picked = Task { await setup.perform(.add(.claudeCode)) }
        for _ in 0..<500 where setup.running == nil { try? await Task.sleep(for: .milliseconds(5)) }
        XCTAssertEqual(setup.running, .add(.claudeCode))
        XCTAssertEqual(setup.detail(toolsOn: true), "Adding to Claude Code…")
        XCTAssertEqual(setup.controls(toolsOn: true), .init(menu: false, add: false, remove: false, copySettings: false))
        await setup.perform(.add(.claudeCode))
        await setup.perform(.copySettings)
        mac.release()
        await picked.value
        XCTAssertEqual(mac.runs.count, 1, "a second pick while one runs does nothing")
        XCTAssertTrue(mac.copied.isEmpty)
        XCTAssertEqual(setup.title, "Added to Claude Code")
    }

    func testCopySettingsGivesTheServersSettingsAsJSON() async throws {
        let mac = FakeClientMac(installed: [.claudeCode: claude])
        let setup = setup(mac)
        await setup.perform(.copySettings)
        XCTAssertEqual(mac.copied, [MCPClient.settingsJSON(executable: executable)])
        let copied = try JSONSerialization.jsonObject(with: Data(mac.copied[0].utf8)) as? [String: [String: [String: Any]]]
        XCTAssertEqual(copied?["mcpServers"]?["agent-hud"]?["command"] as? String, executable)
        XCTAssertEqual(copied?["mcpServers"]?["agent-hud"]?["args"] as? [String], ["--mcp"])
        XCTAssertEqual(setup.detail(toolsOn: true), "Settings copied. Paste them into your client's MCP settings.")
        XCTAssertNil(setup.failedCommandLine)
        XCTAssertTrue(mac.runs.isEmpty)
    }

    func testAnApplicationRunningFromATemporaryPlaceIsNotAdded() async {
        for path in ["/private/var/folders/x1/abc/T/AppTranslocation/0F1E/d/Agent HUD Open.app/Contents/MacOS/Agent HUD Open",
                     "/Volumes/Agent HUD Open/Agent HUD Open.app/Contents/MacOS/Agent HUD Open"] {
            let mac = FakeClientMac(installed: [.claudeCode: claude, .codex: codex], servers: [.codex],
                                    executable: URL(fileURLWithPath: path))
            let setup = setup(mac)
            XCTAssertNil(setup.action(for: .claudeCode), path)
            XCTAssertEqual(setup.detail(toolsOn: true), "Move Agent HUD to Applications first.")
            XCTAssertFalse(setup.canPick(.claudeCode, toolsOn: true))
            XCTAssertFalse(setup.controls(toolsOn: true).copySettings, "nor are its settings copied")
            await setup.perform(.add(.claudeCode))
            await setup.perform(.copySettings)
            XCTAssertTrue(mac.runs.isEmpty, path)
            XCTAssertTrue(mac.copied.isEmpty, path)

            XCTAssertEqual(setup.action(for: .codex), .remove(.codex), "what was added before can still be removed")
            XCTAssertTrue(setup.canPick(.codex, toolsOn: true))
            await setup.perform(.remove(.codex))
            XCTAssertEqual(mac.runs.map(\.1), [["mcp", "remove", "agent-hud"]])
        }
    }

    /// While the switch is off the row is dimmed and only a checked client, with the menu it needs, can be picked.
    func testTheRowFollowsTheSwitchExceptRemove() {
        let neither = setup(FakeClientMac(installed: [.claudeCode: claude, .codex: codex]))
        XCTAssertEqual(neither.controls(toolsOn: true), .init(menu: true, add: true, remove: true, copySettings: true))
        XCTAssertEqual(neither.controls(toolsOn: false), .init(menu: false, add: false, remove: true, copySettings: false))
        XCTAssertTrue(neither.canPick(.claudeCode, toolsOn: true))
        XCTAssertFalse(neither.canPick(.claudeCode, toolsOn: false))

        let codexHasIt = setup(FakeClientMac(installed: [.claudeCode: claude, .codex: codex], servers: [.codex]))
        XCTAssertEqual(codexHasIt.controls(toolsOn: false), .init(menu: true, add: false, remove: true, copySettings: false),
                       "the menu stays so Codex can be unchecked; adding waits for the switch")
        XCTAssertTrue(codexHasIt.canPick(.codex, toolsOn: false))
        XCTAssertFalse(codexHasIt.canPick(.claudeCode, toolsOn: false))
    }
}
