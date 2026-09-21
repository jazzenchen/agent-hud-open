import AgentHUDSupport
import Foundation

/// Claude Code style `Stop` handler in `settings.json` of Qwen Code's home. Qwen counted hook timeouts in milliseconds
/// until 0.23 and in seconds since, except that a value of 1000 or more is still milliseconds, so the timeout is
/// written as milliseconds of at least 1000 to mean the same on every version.
enum QwenHookFormat: CompletionHookFormat {
    static let timeout = 5000

    static func configuration(home: URL) -> URL { QwenSessions.home(home).appendingPathComponent("settings.json") }

    /// `Stop` runs once a prompt's turn ends with no tool call pending; a cancelled or failed turn runs none. It can run
    /// twice for one prompt when another hook keeps the turn going, and `prompt_id` (0.23.4 and later) makes that one
    /// turn; older versions name no turn, so the callback time does.
    static func completion(_ payload: ProviderJSON, now: Date) -> CompletionHookEvent? {
        guard payload["hook_event_name"].stringValue == "Stop", let session = payload["session_id"].stringValue, !session.isEmpty else { return nil }
        let turn = payload["prompt_id"].stringValue.flatMap { $0.isEmpty ? nil : $0 } ?? "stop-\(RecordCoding.milliseconds(now))"
        return .init(session: session, turn: turn, workspace: payload["cwd"].stringValue)
    }

    static func commands(in configuration: [String: ProviderJSON]) -> [String] {
        ClaudeStyleHooks.commands(in: configuration, event: "Stop", source: .qwen)
    }

    static func updating(_ configuration: [String: ProviderJSON], command: String?) throws -> [String: ProviderJSON] {
        try ClaudeStyleHooks.updating(configuration, event: "Stop", source: .qwen, command: command, timeout: timeout)
    }
}
