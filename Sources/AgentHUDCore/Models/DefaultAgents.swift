import Foundation

/// Initial rows before clients report their available usage windows.
public enum DefaultAgents {
    public static let list: [AgentDescriptor] = [
        AgentDescriptor(id: "codex", vendor: "Codex", model: "Desktop / CLI", source: L10n.sourceCodexAppServer, enabled: true, connected: false),
        AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness", source: L10n.sourceDeepSeekSessions, enabled: false, connected: false),
    ]
}
