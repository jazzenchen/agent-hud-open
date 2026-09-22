import AgentHUDSupport
import Darwin
import Foundation
import XCTest
@testable import AgentHUDCore

/// A client asking whether a tool may run, and the answer travelling back to it.
final class PermissionHookTests: XCTestCase, @unchecked Sendable {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private func payload(session: String = "s", tool: String = "Bash",
                         input: [String: Any] = ["command": "rm -rf node_modules",
                                                 "description": "Remove node_modules"]) -> [String: Any] {
        ["session_id": session, "hook_event_name": "PermissionRequest", "cwd": "/Users/me/agent-hud",
         "tool_name": tool, "tool_input": input]
    }

    func testTheHookIsInstalledBesideWhateverElseTheSettingsHold() throws {
        let home = try directory()
        let settings = home.appendingPathComponent(".claude/settings.json")
        try FileManager.default.createDirectory(at: settings.deletingLastPathComponent(), withIntermediateDirectories: true)
        let existing: [String: Any] = ["model": "opus",
                                       "hooks": ["PermissionRequest": [["hooks": [["type": "command", "command": "say hi"]]]]]]
        try JSONSerialization.data(withJSONObject: existing).write(to: settings)

        let executable = URL(fileURLWithPath: "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD")
        XCTAssertFalse(PermissionHooks.isActive(.claude, home: home))
        try PermissionHooks.configure(.claude, enabled: true, executable: executable, home: home)
        XCTAssertTrue(PermissionHooks.isActive(.claude, home: home))

        let updated = try XCTUnwrap(try ProviderJSON.read(Data(contentsOf: settings)).objectValue)
        XCTAssertEqual(updated["model"]?.stringValue, "opus", "nothing else in the file is touched")
        let groups = updated["hooks"]?["PermissionRequest"].arrayValue ?? []
        let commands = groups.flatMap { $0["hooks"].arrayValue ?? [] }.compactMap { $0["command"].stringValue }
        XCTAssertEqual(commands.count, 2)
        XCTAssertTrue(commands.contains("say hi"))
        let ours = try XCTUnwrap(groups.first { group in
            (group["hooks"].arrayValue ?? []).contains { $0["command"].stringValue?.hasSuffix(" --permission-hook claude") == true }
        })
        XCTAssertEqual(ours["matcher"].stringValue, "", "every tool the client would ask about")
        XCTAssertEqual(ours["hooks"].arrayValue?.first?["timeout"].numberValue, 86_400,
                       "the client must keep waiting while the request sits on the HUD")

        try PermissionHooks.configure(.claude, enabled: false, executable: executable, home: home)
        XCTAssertFalse(PermissionHooks.isActive(.claude, home: home))
        let removed = try XCTUnwrap(try ProviderJSON.read(Data(contentsOf: settings)).objectValue)
        XCTAssertEqual((removed["hooks"]?["PermissionRequest"].arrayValue ?? []).count, 1, "the other handler stays")
    }

    func testEachForkIsFoundInItsOwnHome() throws {
        let home = try directory()
        for source in PermissionHooks.Source.allCases {
            XCTAssertFalse(source.isInstalled(home: home), "a machine without the client keeps its home untouched")
        }
        try FileManager.default.createDirectory(at: home.appendingPathComponent(".qoder"), withIntermediateDirectories: true)
        XCTAssertTrue(PermissionHooks.Source.qoder.isInstalled(home: home))
        XCTAssertFalse(PermissionHooks.Source.qoderCN.isInstalled(home: home), "the CN build has its own home")

        try PermissionHooks.configure(.qoder, enabled: true, executable: URL(fileURLWithPath: "/tmp/hud"), home: home)
        let settings = try ProviderJSON.read(Data(contentsOf: home.appendingPathComponent(".qoder/settings.json")))
        let commands = (settings["hooks"]["PermissionRequest"].arrayValue ?? [])
            .flatMap { $0["hooks"].arrayValue ?? [] }.compactMap { $0["command"].stringValue }
        XCTAssertEqual(commands, ["'/tmp/hud' --permission-hook qoderCN".replacingOccurrences(of: "qoderCN", with: "qoder")],
                       "the fork ships Claude Code's schema unchanged, so one handler serves it")
    }

    func testAnUnreadableSettingsLayoutIsNeverRewritten() throws {
        let home = try directory()
        let settings = home.appendingPathComponent(".claude/settings.json")
        try FileManager.default.createDirectory(at: settings.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(#"{"hooks": "everything off"}"#.utf8).write(to: settings)
        XCTAssertThrowsError(try PermissionHooks.configure(.claude, enabled: true,
            executable: URL(fileURLWithPath: "/tmp/hud"), home: home))
        XCTAssertEqual(try Data(contentsOf: settings), Data(#"{"hooks": "everything off"}"#.utf8))
    }

    func testCodexCLIAndDesktopShareAnIdempotentHookWithoutChangingConfiguration() throws {
        let home = try directory()
        let root = home.appendingPathComponent(".codex")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        XCTAssertTrue(PermissionHooks.Source.codex.isInstalled(home: home))
        let config = root.appendingPathComponent("config.toml")
        let originalConfig = Data("[features]\nhooks = false\n".utf8)
        try originalConfig.write(to: config)
        let hooks = root.appendingPathComponent("hooks.json")
        let original: [String: Any] = ["description": "My hooks", "hooks": [
            "Stop": [["hooks": [["type": "command", "command": "echo done"]]]],
            "PermissionRequest": [["matcher": "Bash", "hooks": [["type": "command", "command": "echo check"]]]],
        ]]
        try JSONSerialization.data(withJSONObject: original).write(to: hooks)
        let executable = URL(fileURLWithPath: "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD")
        try PermissionHooks.configure(.codex, enabled: true, executable: executable, home: home)
        let installed = try Data(contentsOf: hooks)
        try PermissionHooks.configure(.codex, enabled: true, executable: executable, home: home)
        XCTAssertEqual(try Data(contentsOf: hooks), installed, "repeated startup must not change hook trust hashes")
        let object = try XCTUnwrap(try ProviderJSON.read(installed).objectValue)
        XCTAssertEqual(PermissionHooks.commands(in: object, source: .codex),
                       ["'\(executable.path)' --permission-hook codex"], "one handler serves both Codex clients")
        XCTAssertTrue(PermissionHooks.isActive(.codex, home: home))
        XCTAssertEqual(try Data(contentsOf: config), originalConfig, "a disabled hook feature stays disabled")
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("settings.json").path))

        XCTAssertThrowsError(try PermissionHooks.configure(.codex, enabled: true,
            executable: URL(fileURLWithPath: "/tmp/other-hud"), home: home))
        XCTAssertEqual(try Data(contentsOf: hooks), installed, "another installation cannot take over the handler")
        try PermissionHooks.configure(.codex, enabled: false, executable: executable, home: home)
        XCTAssertEqual(try ProviderJSON.read(Data(contentsOf: hooks)),
                       try ProviderJSON.read(JSONSerialization.data(withJSONObject: original)), "other hooks are preserved")
    }

    func testCodexHomeUsesTheSameResolverAsItsUsageProvider() throws {
        let home = try directory()
        let custom = home.appendingPathComponent("custom-codex", isDirectory: true)
        XCTAssertEqual(CodexLocator.dataDirectory(home: home, environment: ["CODEX_HOME": custom.path]), custom)
        for environment in [[:], ["CODEX_HOME": ""]] {
            XCTAssertEqual(CodexLocator.dataDirectory(home: home, environment: environment),
                           home.appendingPathComponent(".codex", isDirectory: true))
        }
        XCTAssertEqual(PermissionHooks.Source.codex.configuration(home: home),
                       CodexLocator.dataDirectory(home: home).appendingPathComponent("hooks.json"))
    }

    func testCodexPatchAndShellRequestsShowTheOperationAndOnlySupportedAnswers() throws {
        let patch = "*** Begin Patch\n*** Update File: README.md\n@@\n-old\n+new\n*** End Patch"
        var body = payload(tool: "apply_patch", input: ["command": patch])
        body["turn_id"] = "turn-1"
        let rule: JSONValue = .object(["type": .string("addRules"), "behavior": .string("allow"),
                                      "rules": .array([.object(["toolName": .string("Bash")])])])
        body["permission_suggestions"] = try JSONSerialization.jsonObject(with: RecordCoding.encoder().encode([rule]))
        let request = try XCTUnwrap(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: body),
                                                               source: .codex, id: "patch", now: now))
        XCTAssertEqual(request.vendor, "Codex")
        XCTAssertEqual(request.sessionID, "s")
        XCTAssertEqual(request.detail, patch)
        XCTAssertEqual(request.badge, "EDIT")
        XCTAssertEqual(request.symbol, "pencil")
        XCTAssertNil(request.alwaysAllow, "Codex does not support permission-rule updates, even if a payload offers one")
        XCTAssertTrue(PermissionDecision.allowAlways(rule).response(for: .codex).isEmpty)
        let claudeResponse = try ProviderJSON.read(PermissionDecision.allowAlways(rule).response(for: .claude))
        XCTAssertEqual(claudeResponse["hookSpecificOutput"]["decision"]["updatedPermissions"].arrayValue,
                       [rule], "Claude keeps its rule updates")

        let shell = try XCTUnwrap(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload()),
                                                             source: .codex, id: "shell", now: now))
        XCTAssertEqual(shell.summary, "Remove node_modules")
        XCTAssertEqual(shell.detail, "rm -rf node_modules")
        for decision in [PermissionDecision.allow, .deny] {
            let response = try ProviderJSON.read(decision.response(for: .codex))
            XCTAssertEqual(response["hookSpecificOutput"]["hookEventName"].stringValue, "PermissionRequest")
            XCTAssertEqual(response["hookSpecificOutput"]["decision"].objectValue,
                           ["behavior": .string(decision == .allow ? "allow" : "deny")])
        }
    }

    func testCodeBuddyAndWorkBuddyAnswerOnceWithoutRuleUpdates() throws {
        let home = try directory()
        let executable = URL(fileURLWithPath: "/tmp/hud")
        for (source, folder) in [(PermissionHooks.Source.codebuddy, ".codebuddy"), (.workbuddy, ".workbuddy")] {
            try FileManager.default.createDirectory(at: home.appendingPathComponent(folder), withIntermediateDirectories: true)
            XCTAssertFalse(source.isInstalled(home: home), "a settings folder alone can be the IDE extension's")
            try FileManager.default.createDirectory(at: home.appendingPathComponent("\(folder)/projects"), withIntermediateDirectories: true)
            XCTAssertTrue(source.isInstalled(home: home))
            try PermissionHooks.configure(source, enabled: true, executable: executable, home: home)
            let settings = try ProviderJSON.read(Data(contentsOf: home.appendingPathComponent("\(folder)/settings.json")))
            let handler = settings["hooks"]["PermissionRequest"].arrayValue?.first?["hooks"].arrayValue?.first
            XCTAssertEqual(handler?["command"].stringValue, "'/tmp/hud' --permission-hook \(source.rawValue)")
            XCTAssertEqual(handler?["timeout"].numberValue, 86_400, "CodeBuddy counts hook timeouts in seconds")
        }

        let rule: JSONValue = .object(["type": .string("addRules"), "behavior": .string("allow"),
                                      "rules": .array([.object(["toolName": .string("MultiEdit")])])])
        var body = payload(tool: "MultiEdit", input: ["file_path": "/Users/me/agent-hud/README.md",
                                                      "edits": [["old_string": "old", "new_string": "new"],
                                                                ["old_string": "a", "new_string": "b"]]])
        body["permission_suggestions"] = try JSONSerialization.jsonObject(with: RecordCoding.encoder().encode([rule]))
        let request = try XCTUnwrap(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: body),
                                                               source: .workbuddy, id: "edit", now: now))
        XCTAssertEqual(request.vendor, "WorkBuddy")
        XCTAssertEqual(request.summary, "README.md")
        XCTAssertEqual(request.badge, "EDIT")
        XCTAssertEqual(request.path, "/Users/me/agent-hud/README.md")
        XCTAssertEqual(request.removed, "old", "a multi-edit is recognized by its first change")
        XCTAssertEqual(request.added, "new")
        XCTAssertNil(request.alwaysAllow, "the engine offers rules but never applies one sent back")
        XCTAssertTrue(PermissionDecision.allowAlways(rule).response(for: .codebuddy).isEmpty)
        XCTAssertEqual(try ProviderJSON.read(PermissionDecision.allow.response(for: .codebuddy))["hookSpecificOutput"]["decision"].objectValue,
                       ["behavior": .string("allow")])
    }

    func testZCodeNestsItsHookAndTurnsHooksOnOnlyWhenUnset() throws {
        let home = try directory()
        let root = home.appendingPathComponent(".zcode/cli")
        XCTAssertFalse(PermissionHooks.Source.zcode.isInstalled(home: home))
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        XCTAssertTrue(PermissionHooks.Source.zcode.isInstalled(home: home))
        let config = root.appendingPathComponent("config.json")
        let original: [String: Any] = ["$schema": "https://zcode.z.ai/config.json",
                                       "mcp": ["servers": ["linear": ["env": ["TOKEN": "secret"]]]],
                                       "hooks": ["timeoutMs": 30000, "events": [
                                           "Stop": [["hooks": [["type": "command", "command": "echo done"]]]]]]]
        try JSONSerialization.data(withJSONObject: original).write(to: config)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: config.path)

        let executable = URL(fileURLWithPath: "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD")
        try PermissionHooks.configure(.zcode, enabled: true, executable: executable, home: home)
        XCTAssertTrue(PermissionHooks.isActive(.zcode, home: home))
        let installed = try ProviderJSON.read(Data(contentsOf: config))
        XCTAssertEqual(installed["mcp"]["servers"]["linear"]["env"]["TOKEN"].stringValue, "secret", "the rest of the file is kept")
        XCTAssertEqual(installed["hooks"]["timeoutMs"].numberValue, 30000)
        XCTAssertEqual(installed["hooks"]["enabled"], .bool(true), "ZCode runs no hook until hooks are switched on")
        XCTAssertNil(installed["hooks"]["PermissionRequest"].arrayValue, "ZCode reads its events one level down")
        let group = try XCTUnwrap(installed["hooks"]["events"]["PermissionRequest"].arrayValue?.first?.objectValue)
        XCTAssertEqual(Set(group.keys), ["hooks"], "ZCode rejects an empty matcher and any key it does not know")
        XCTAssertEqual(group["hooks"]?.arrayValue?.first?["timeout"].numberValue, 86_400)
        XCTAssertEqual(installed["hooks"]["events"]["Stop"].arrayValue?.count, 1)
        let mode = try FileManager.default.attributesOfItem(atPath: config.path)[.posixPermissions] as? NSNumber
        XCTAssertEqual(mode?.intValue, 0o600, "a file that holds server credentials stays private")

        try PermissionHooks.configure(.zcode, enabled: false, executable: executable, home: home)
        let removed = try ProviderJSON.read(Data(contentsOf: config))
        XCTAssertNil(removed["hooks"]["events"]["PermissionRequest"].arrayValue)
        XCTAssertEqual(removed["hooks"]["events"]["Stop"].arrayValue?.count, 1)

        var off = original
        off["hooks"] = ["enabled": false]
        try JSONSerialization.data(withJSONObject: off).write(to: config)
        try PermissionHooks.configure(.zcode, enabled: true, executable: executable, home: home)
        XCTAssertEqual(try ProviderJSON.read(Data(contentsOf: config))["hooks"]["enabled"], .bool(false),
                       "hooks the user switched off stay off")

        try Data(#"{"hooks": {"events": []}}"#.utf8).write(to: config)
        XCTAssertThrowsError(try PermissionHooks.configure(.zcode, enabled: true, executable: executable, home: home),
                             "a layout ZCode would reject is never rewritten")
    }

    func testZCodeQuestionsAndPlansStayInItsOwnDialog() throws {
        for tool in ["AskUserQuestion", "ExitPlanMode"] {
            XCTAssertNil(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload(tool: tool, input: [:])),
                                                     source: .zcode, id: tool, now: now))
        }
        let shell = try XCTUnwrap(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload()),
                                                             source: .zcode, id: "shell", now: now))
        XCTAssertEqual(shell.vendor, "ZCode")
        XCTAssertEqual(shell.detail, "rm -rf node_modules")
        XCTAssertNil(shell.alwaysAllow)
    }

    func testQwenWaitsADayInMillisecondsAndItsToolsReadLikeClaudeCodes() throws {
        let home = try directory()
        try FileManager.default.createDirectory(at: home.appendingPathComponent(".qwen"), withIntermediateDirectories: true)
        XCTAssertTrue(PermissionHooks.Source.qwen.isInstalled(home: home))
        try PermissionHooks.configure(.qwen, enabled: true, executable: URL(fileURLWithPath: "/tmp/hud"), home: home)
        let settings = try ProviderJSON.read(Data(contentsOf: home.appendingPathComponent(".qwen/settings.json")))
        let handler = settings["hooks"]["PermissionRequest"].arrayValue?.first?["hooks"].arrayValue?.first
        XCTAssertEqual(handler?["command"].stringValue, "'/tmp/hud' --permission-hook qwen")
        XCTAssertEqual(handler?["timeout"].numberValue, 86_400_000, "Qwen reads 1000 or more as milliseconds on every version")

        let shell = try XCTUnwrap(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload(tool: "run_shell_command")),
                                                             source: .qwen, id: "shell", now: now))
        XCTAssertEqual(shell.vendor, "Qwen")
        XCTAssertEqual(shell.toolName, "run_shell_command", "the client's own name is kept")
        XCTAssertEqual([shell.badge, shell.symbol, shell.summary], ["BASH", "terminal.fill", "Remove node_modules"])
        XCTAssertEqual(shell.detail, "rm -rf node_modules")

        let edit = try XCTUnwrap(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload(tool: "edit", input: [
            "file_path": "/Users/me/agent-hud/README.md", "old_string": "old", "new_string": "new"])), source: .qwen, id: "edit", now: now))
        XCTAssertEqual([edit.badge, edit.summary, edit.removed, edit.added], ["EDIT", "README.md", "old", "new"])
        XCTAssertEqual(edit.path, "/Users/me/agent-hud/README.md")
        XCTAssertNil(edit.alwaysAllow)

        for tool in ["ask_user_question", "exit_plan_mode"] {
            XCTAssertNil(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload(tool: tool, input: [:])),
                                                     source: .qwen, id: tool, now: now), "Qwen ignores an allow for \(tool)")
        }
    }

    func testTheRequestSaysWhatTheCallIsAbout() throws {
        let bash = try XCTUnwrap(try PermissionRequest.parse(
            JSONSerialization.data(withJSONObject: payload()), source: .claude, id: "1", now: now))
        XCTAssertEqual(bash.summary, "Remove node_modules", "the tool's own description reads better than its command")
        XCTAssertEqual(bash.detail, "rm -rf node_modules", "and the command itself is what is being approved")
        XCTAssertEqual(bash.project, "agent-hud", "the folder is how two sessions are told apart")

        let edit = try XCTUnwrap(try PermissionRequest.parse(
            JSONSerialization.data(withJSONObject: payload(tool: "Edit", input: ["file_path": "/Users/me/agent-hud/README.md"])),
            source: .claude, id: "2", now: now))
        XCTAssertEqual(edit.summary, "README.md")
        XCTAssertEqual(edit.detail, "/Users/me/agent-hud/README.md")

        let mcp = try XCTUnwrap(try PermissionRequest.parse(
            JSONSerialization.data(withJSONObject: payload(tool: "mcp__linear__create_issue", input: [:])),
            source: .claude, id: "3", now: now))
        XCTAssertEqual(mcp.summary, "linear · create_issue")

        XCTAssertNil(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: ["tool_name": "Bash"]),
                                                 source: .claude, id: "4", now: now),
                     "a request that names no session cannot be shown beside one")
    }

    private let questions: [[String: Any]] = [
        ["question": "Push the 40 commits now?", "header": "Push",
         "options": [["label": "Push", "description": "And update the PR"], ["label": "Wait"]], "multiSelect": false],
        ["question": "Which pages report it?", "header": "Pages",
         "options": [["label": "Channels"], ["label": "Documents"]], "multiSelect": true],
    ]

    func testAClaudeQuestionIsAnsweredWithTheUsersChoicesInsideItsOwnInput() throws {
        let data = try JSONSerialization.data(withJSONObject: payload(tool: "AskUserQuestion",
                                                                      input: ["questions": questions, "metadata": ["source": "x"]]))
        let request = try XCTUnwrap(try PermissionRequest.parse(data, source: .claude, id: "q", now: now))
        XCTAssertTrue(request.isQuestion)
        XCTAssertEqual(request.questions.map(\.question), ["Push the 40 commits now?", "Which pages report it?"])
        XCTAssertEqual(request.questions[0].options, [.init(label: "Push", description: "And update the PR"), .init(label: "Wait")])
        XCTAssertEqual(request.questions.map(\.multiSelect), [false, true])
        XCTAssertEqual([request.badge, request.summary], ["ASK", "Push the 40 commits now?"])

        let answers = ["Push the 40 commits now?": "Push", "Which pages report it?": "Channels, Documents"]
        let decision = try ProviderJSON.read(PermissionDecision.answer(answers).response(for: request))["hookSpecificOutput"]["decision"]
        XCTAssertEqual(decision["behavior"].stringValue, "allow")
        XCTAssertEqual(decision["updatedInput"]["questions"], try ProviderJSON.read(JSONSerialization.data(withJSONObject: questions)),
                       "Claude Code reads the questions back beside their answers")
        XCTAssertEqual(decision["updatedInput"]["metadata"]["source"].stringValue, "x", "the rest of the call is left as it was")
        XCTAssertEqual(decision["updatedInput"]["answers"], .object(answers.mapValues { .string($0) }))
        XCTAssertTrue(PermissionDecision.answer(answers).response(for: .claude).isEmpty, "answers need the question they answer")
        let skipped = try ProviderJSON.read(PermissionDecision.answer([:]).response(for: request))["hookSpecificOutput"]["decision"]
        XCTAssertEqual(skipped["behavior"].stringValue, "allow", "skipping every question is not a refusal")
        XCTAssertEqual(skipped["updatedInput"]["answers"], .object([:]), "Claude Code reads it as the questions left open")
        XCTAssertTrue(PermissionDecision.leave.response(for: request).isEmpty, "putting a card away says nothing")

        let unreadable = [questions[0], ["question": "Anything else?", "options": []]]
        XCTAssertNil(try PermissionRequest.parse(JSONSerialization.data(withJSONObject: payload(tool: "AskUserQuestion", input: ["questions": unreadable])),
                                                 source: .claude, id: "u", now: now),
                     "a question the HUD cannot offer answers for is left whole to Claude Code's own dialog")
        for source in [PermissionHooks.Source.qoder, .codebuddy, .workbuddy] {
            XCTAssertNil(try PermissionRequest.parse(data, source: source, id: "f", now: now), "\(source) is not known to read answers back")
        }
    }

    func testAPlanIsShownToBeApprovedInClaudeCodeAndLeftToTheForksOwnDialog() throws {
        let data = try JSONSerialization.data(withJSONObject: payload(tool: "ExitPlanMode", input: [
            "plan": "## Ship the question card\n\n1. Parse the questions\n2. Send the answers back"]))
        let plan = try XCTUnwrap(try PermissionRequest.parse(data, source: .claude, id: "p", now: now))
        XCTAssertTrue(plan.isPlan)
        XCTAssertFalse(plan.isQuestion)
        XCTAssertEqual([plan.badge, plan.summary], ["PLAN", "Ship the question card"], "a plan is named by its title")
        XCTAssertEqual(plan.detail?.hasPrefix("## Ship the question card\n\n1. Parse"), true, "and its opening lines are there to read")
        for source in [PermissionHooks.Source.qoder, .codebuddy, .workbuddy] {
            XCTAssertNil(try PermissionRequest.parse(data, source: source, id: "f", now: now), "\(source) keeps its plans in its own dialog")
        }
    }

    func testACallAnsweredInTheClientShowsAsSettledInItsSessionRecord() throws {
        func line(_ blocks: [[String: Any]]) throws -> Data {
            try JSONSerialization.data(withJSONObject: ["type": "assistant", "message": ["role": "assistant", "content": blocks]])
        }
        func use(_ id: String, _ tool: String = "Bash", _ command: String = "git push") throws -> Data {
            try line([["type": "tool_use", "id": id, "name": tool, "input": ["command": command]]])
        }
        func result(_ id: String) throws -> Data { try line([["type": "tool_result", "tool_use_id": id, "content": "ok"]]) }

        var record = PermissionTranscript(tool: "Bash", input: .object(["command": .string("git push")]))
        // What was already written: the same command approved an hour ago, then this call, still waiting.
        for earlier in [try use("old"), try result("old"), try use("other", "Bash", "ls"), try use("now")] {
            _ = record.read(earlier)
        }
        XCTAssertEqual(record.calls, ["now"], "an earlier call with its result is not the one waiting")
        XCTAssertFalse(record.read(try result("other")), "another call's result settles nothing")
        XCTAssertTrue(record.read(try result("now")))

        // A question is written only once answered, together with its result.
        let question: JSONValue = .object(["questions": .array([.object(["question": .string("Push?")])])])
        var asked = PermissionTranscript(tool: "AskUserQuestion", input: question)
        XCTAssertFalse(asked.read(try line([["type": "tool_use", "id": "q", "name": "AskUserQuestion", "input": ["questions": [["question": "Push?"]]]]])))
        XCTAssertTrue(asked.read(try result("q")))
    }

    func testTheHookLetsGoOnceTheRecordShowsTheCallSettled() throws {
        let transcript = try directory().appendingPathComponent("session.jsonl")
        let use = #"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"make"}}]}}"#
        try Data((use + "\n").utf8).write(to: transcript)
        let payload: JSONValue = .object(["transcript_path": .string(transcript.path), "tool_name": .string("Bash"),
                                          "tool_input": .object(["command": .string("make")])])
        let settled = expectation(description: "the call is settled")
        let watch = try XCTUnwrap(PermissionTranscriptWatch(payload: payload) { settled.fulfill() })

        let handle = try FileHandle(forWritingTo: transcript)
        handle.seekToEndOfFile()
        handle.write(Data(#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"done"}]}}"#.utf8))
        handle.write(Data("\n".utf8))
        try handle.close()
        wait(for: [settled], timeout: 5)
        watch.stop()
        XCTAssertNil(PermissionTranscriptWatch(payload: .object(["tool_name": .string("Bash")])) {}, "no record, nothing to follow")
    }

    func testTheAnswerIsTheOneTheClientReads() throws {
        let allow = try XCTUnwrap(try ProviderJSON.read(PermissionDecision.allow.response(for: .claude)).objectValue)
        let output = try XCTUnwrap(allow["hookSpecificOutput"]?.objectValue)
        XCTAssertEqual(output["hookEventName"]?.stringValue, "PermissionRequest")
        XCTAssertEqual(output["decision"]?["behavior"].stringValue, "allow")
        XCTAssertEqual(try ProviderJSON.read(PermissionDecision.deny.response(for: .claude))["hookSpecificOutput"]["decision"]["behavior"].stringValue,
                       "deny")
        XCTAssertTrue(PermissionDecision.noDecision.isEmpty, "saying nothing leaves the client's own prompt alone")
    }

    // MARK: The channel

    /// Connects the way the hook does and returns the descriptor, or nil when nothing is listening.
    private func connect(to path: String) -> Int32? {
        let descriptor = socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else { return nil }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let size = MemoryLayout.size(ofValue: address.sun_path)
        _ = withUnsafeMutablePointer(to: &address.sun_path) { field in
            field.withMemoryRebound(to: CChar.self, capacity: size) { strlcpy($0, path, size) }
        }
        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { close(descriptor); return nil }
        return descriptor
    }

    /// The listener comes up on the main queue, so the first connection waits for it the way a hook would retry.
    private func ask(_ path: String, _ body: [String: Any], source: PermissionHooks.Source = .claude) async throws -> Int32 {
        var opened = connect(to: path)
        for _ in 0..<200 where opened == nil {
            try await Task.sleep(for: .milliseconds(10))
            opened = connect(to: path)
        }
        let descriptor = try XCTUnwrap(opened, "the HUD is listening at \(path): \(String(cString: strerror(errno)))")
        var payload = body
        payload[PermissionHookClient.sourceKey] = source.rawValue
        let data = try JSONSerialization.data(withJSONObject: payload)
        _ = data.withUnsafeBytes { Darwin.send(descriptor, $0.baseAddress, $0.count, 0) }
        shutdown(descriptor, SHUT_WR)
        return descriptor
    }

    /// Waits for the requests on the HUD to become `count`, so the test follows the channel rather than a delay.
    @MainActor private func waitForPending(_ count: Int, _ message: String) async throws {
        for _ in 0..<200 where PermissionRequests.shared.pending.count != count {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(PermissionRequests.shared.pending.count, count, message)
    }

    func testTheWaitForAnAnswerIsTenMinutesUnlessChosen() throws {
        XCTAssertEqual(Settings().approvalWaitMinutes, 10)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: Data(#"{"approvalWaitMinutes":7}"#.utf8)).approvalWaitMinutes, 10,
                       "only the offered choices are kept")
        let chosen = Settings().with { $0.approvalWaitMinutes = 30 }
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: JSONEncoder().encode(chosen)).approvalWaitMinutes, 30)
    }

    @MainActor
    func testAnUnansweredRequestGoesBackToTheClientWhenTheWaitRunsOut() async throws {
        let path = try directory().appendingPathComponent("permission.sock").path
        let requests = PermissionRequests.shared
        requests.start(path: path)
        addTeardownBlock { Task { @MainActor in requests.stop(); requests.holdTime = 600 } }

        requests.holdTime = 0.3
        let client = try await ask(path, payload())
        try await waitForPending(1, "the request reaches the HUD")
        try await waitForPending(0, "nobody answered in time")
        var buffer = [UInt8](repeating: 0, count: 64)
        XCTAssertEqual(recv(client, &buffer, buffer.count, 0), 0, "no answer: the client asks in its own prompt")
        close(client)

        // A shorter wait chosen while a request is already on the HUD applies to it too.
        requests.holdTime = 600
        let waiting = try await ask(path, payload(session: "t"))
        try await waitForPending(1, "the second request reaches the HUD")
        requests.holdTime = 0.2
        try await waitForPending(0, "the new wait applies to a request already waiting")
        XCTAssertEqual(recv(waiting, &buffer, buffer.count, 0), 0)
        close(waiting)
    }

    @MainActor
    func testAClientWaitsOnTheChannelUntilItIsAnsweredOrGivesUp() async throws {
        let path = try directory().appendingPathComponent("permission.sock").path
        let requests = PermissionRequests.shared
        requests.start(path: path)
        addTeardownBlock { Task { @MainActor in requests.stop() } }

        let client = try await ask(path, payload())
        try await waitForPending(1, "the request reaches the HUD")
        let request = try XCTUnwrap(requests.pending.first)
        XCTAssertEqual(request.summary, "Remove node_modules")
        XCTAssertEqual(request.sessionID, "s")

        requests.resolve(request.id, .allow)
        var buffer = [UInt8](repeating: 0, count: 4096)
        let read = recv(client, &buffer, buffer.count, 0)
        close(client)
        XCTAssertGreaterThan(read, 0, "the client is waiting for exactly this")
        XCTAssertEqual(try ProviderJSON.read(Data(buffer[..<max(0, read)]))["hookSpecificOutput"]["decision"]["behavior"].stringValue,
                       "allow")
        XCTAssertTrue(requests.pending.isEmpty, "an answered request leaves the HUD")

        // A client that gives up — answered in its own terminal, timed out, killed — takes its request with it.
        let abandoned = try await ask(path, payload(session: "t"))
        try await waitForPending(1, "the second request reaches the HUD")
        close(abandoned)
        try await waitForPending(0, "a request nobody is waiting for is no longer a question")

        // Both Codex clients use the same source; answering one must leave the other client waiting.
        let cli = try await ask(path, payload(session: "codex-cli"), source: .codex)
        let desktop = try await ask(path, payload(session: "codex-desktop", tool: "apply_patch",
                                                 input: ["command": "*** Begin Patch\n*** End Patch"]), source: .codex)
        try await waitForPending(2, "CLI and Desktop share the queue")
        let cliRequest = try XCTUnwrap(requests.pending.first { $0.sessionID == "codex-cli" })
        let desktopRequest = try XCTUnwrap(requests.pending.first { $0.sessionID == "codex-desktop" })
        XCTAssertEqual(cliRequest.vendor, "Codex")
        XCTAssertEqual(desktopRequest.vendor, "Codex")
        requests.resolve(desktopRequest.id, .deny)
        let denied = recv(desktop, &buffer, buffer.count, 0)
        close(desktop)
        XCTAssertGreaterThan(denied, 0)
        XCTAssertEqual(try ProviderJSON.read(Data(buffer[..<max(0, denied)]))["hookSpecificOutput"]["decision"]["behavior"].stringValue, "deny")
        XCTAssertEqual(requests.pending.map(\.id), [cliRequest.id])
        requests.resolve(cliRequest.id, .allow)
        let allowed = recv(cli, &buffer, buffer.count, 0)
        close(cli)
        XCTAssertGreaterThan(allowed, 0)
        XCTAssertEqual(try ProviderJSON.read(Data(buffer[..<max(0, allowed)]))["hookSpecificOutput"]["decision"]["behavior"].stringValue, "allow")

        let cancelled = try await ask(path, payload(session: "codex-cancelled"), source: .codex)
        try await waitForPending(1, "Codex can withdraw a waiting request")
        close(cancelled)
        try await waitForPending(0, "cancellation removes the Codex card")
        let fallback = try await ask(path, payload(session: "codex-fallback"), source: .codex)
        try await waitForPending(1, "the HUD can close while Codex waits")
        requests.stop()
        XCTAssertEqual(recv(fallback, &buffer, buffer.count, 0), 0, "no answer leaves Codex's native approval flow in charge")
        close(fallback)
        XCTAssertTrue(requests.pending.isEmpty)
    }
}
