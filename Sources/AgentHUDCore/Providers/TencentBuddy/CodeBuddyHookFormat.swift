import AgentHUDSupport
import Foundation

/// Claude Code style `Stop` handler in `settings.json` of CodeBuddy Code's home; `Stop` takes no matcher and `timeout`
/// is in seconds.
enum CodeBuddyHookFormat: CompletionHookFormat {
    static func configuration(home: URL) -> URL { CodeBuddySessions.home(home).appendingPathComponent("settings.json") }

    /// `Stop` names no turn: the transcript's final assistant message identifies it, else the callback time does.
    /// `StopFailure` and other events are not completions.
    static func completion(_ payload: ProviderJSON, now: Date) -> CompletionHookEvent? {
        guard payload["hook_event_name"].stringValue == "Stop", let session = payload["session_id"].stringValue, !session.isEmpty else { return nil }
        let message = payload["transcript_path"].stringValue.flatMap { TencentBuddySessions.lastAssistantMessage(URL(fileURLWithPath: $0), session: session) }
        return .init(session: session, turn: message?.id ?? "stop-\(RecordCoding.milliseconds(now))",
                     workspace: payload["cwd"].stringValue, model: message?.model)
    }

    static func commands(in configuration: [String: ProviderJSON]) -> [String] {
        ClaudeStyleHooks.commands(in: configuration, event: "Stop", source: .codebuddy)
    }

    static func updating(_ configuration: [String: ProviderJSON], command: String?) throws -> [String: ProviderJSON] {
        try ClaudeStyleHooks.updating(configuration, event: "Stop", source: .codebuddy, command: command)
    }
}
