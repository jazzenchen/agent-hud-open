import Foundation

/// Hook files that follow Claude Code's settings layout: `hooks.<Event>` is a list of matcher groups, each with a
/// `hooks` list of command handlers. Agent HUD owns one group whose only handler runs its completion command.
enum ClaudeStyleHooks {
    static func commands(in configuration: [String: ProviderJSON], event: String, source: CompletionHooks.Source) -> [String] {
        (configuration["hooks"]?[event].arrayValue ?? []).flatMap { $0["hooks"].arrayValue ?? [] }
            .compactMap { $0["command"].stringValue }.filter { CompletionHooks.ownsCommand($0, source: source) }
    }

    /// `timeout` is in the client's own unit: seconds for Claude Code's forks, milliseconds for Qwen Code.
    static func updating(_ configuration: [String: ProviderJSON], event: String, source: CompletionHooks.Source,
                         command: String?, timeout: Int = 5) throws -> [String: ProviderJSON] {
        var object = configuration
        guard object["hooks"] == nil || object["hooks"]?.objectValue != nil else { throw ProviderFailure.format }
        var hooks = object["hooks"]?.objectValue ?? [:]
        guard hooks[event] == nil || hooks[event]?.arrayValue != nil else { throw ProviderFailure.format }
        var groups = (hooks[event]?.arrayValue ?? []).compactMap { group -> ProviderJSON? in
            guard var fields = group.objectValue, let handlers = fields["hooks"]?.arrayValue else { return group }
            let kept = handlers.filter { !CompletionHooks.ownsCommand($0["command"].stringValue, source: source) }
            if kept.count == handlers.count { return group }
            if kept.isEmpty { return nil }
            fields["hooks"] = .array(kept)
            return .object(fields)
        }
        if let command {
            groups.append(.object(["hooks": .array([.object(["type": .string("command"), "command": .string(command), "timeout": .integer(Int64(timeout))])])]))
        }
        hooks[event] = groups.isEmpty ? nil : .array(groups)
        object["hooks"] = hooks.isEmpty && configuration["hooks"] == nil ? nil : .object(hooks)
        return object
    }
}
