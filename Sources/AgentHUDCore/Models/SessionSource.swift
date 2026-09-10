import Foundation

/// Display grouping for provider-owned client metadata, independent of invocation mode.
public struct SessionSource: Hashable, Sendable {
    public let vendor: String?
    private let client: String?

    public init(vendor: String?, client: String?) {
        self.vendor = vendor
        switch vendor {
        case "Codex":
            switch client {
            case "CLI · exec": self.client = "CLI"
            case "Codex": self.client = nil
            default: self.client = client
            }
        case "Claude":
            // Builds before entrypoint detection, and synced peers still on them, carry the plain product name.
            self.client = client == ClaudeEntrypoint.defaultLabel ? nil : client
        default:
            self.client = client
        }
    }

    public var name: String {
        switch (vendor, client) {
        case ("Codex", "Desktop"): return "Codex Desktop"
        case ("Codex", "CLI"): return "Codex CLI"
        case ("Codex", "IDE"): return "Codex IDE extension"
        case ("Claude", nil): return ClaudeEntrypoint.defaultLabel
        default: return client ?? vendor ?? L10n.text("未知来源", "Unknown source")
        }
    }

    /// Vendor implied by an agent id when no descriptor exists for it: a Claude session whose model never answered
    /// ("claude-model:Unknown"), or a synced session for a model this Mac has not seen.
    public static func vendor(impliedBy agentId: String) -> String? {
        let id = agentId.lowercased()
        if id.hasPrefix("claude") { return "Claude" }
        if id.hasPrefix("codex") { return "Codex" }
        if id.hasPrefix("deepseek") { return "DeepSeek" }
        if id.hasPrefix("chatgpt") { return "ChatGPT" }
        if id.hasPrefix("antigravity") { return "Antigravity" }
        if id.hasPrefix("cursor") { return "Cursor" }
        if id.hasPrefix("grok") { return "Grok" }
        for source in OpenAgentSource.allCases where id.hasPrefix(source.rawValue + "-model:") { return source.name }
        return nil
    }
}
