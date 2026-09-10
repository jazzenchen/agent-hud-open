import Foundation

/// Family and version used for display labels and quota-family matching.
/// Token consumers retain their original model ids, including different versions.
public struct ClaudeModelInfo: Hashable, Sendable {
    public static let families = ["opus", "sonnet", "haiku", "fable", "mythos"]

    public let family: String
    public let version: String

    public init(family: String, version: String) {
        self.family = family
        self.version = version
    }

    /// "claude-opus" — stable per family so settings, thresholds and history survive version bumps.
    public var agentId: String { "claude-\(family.lowercased())" }

    /// "Opus 4.5", "Fable 5.1", "Sonnet 5".
    public var displayName: String { version.isEmpty ? family : "\(family) \(version)" }

    public var isSubagentModel: Bool { family.lowercased() == "haiku" }

    /// Handles `claude-opus-4-5-20251101`, `claude-fable-5-1`, `claude-opus-5`, `claude-3-5-haiku-20241022`,
    /// `us.anthropic.claude-3-5-sonnet-20241022-v2:0`. Returns nil for `<synthetic>` and non-Claude ids.
    public static func parse(_ modelId: String) -> ClaudeModelInfo? {
        let tokens = modelId.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
        guard let familyIndex = tokens.firstIndex(where: { families.contains($0) }) else { return nil }
        let family = tokens[familyIndex].prefix(1).uppercased() + tokens[familyIndex].dropFirst()
        var numbers: [String] = []
        // Modern ids put the version after the family.
        for token in tokens[(familyIndex + 1)...] {
            guard token.allSatisfy(\.isNumber), token.count < 8 else { break }
            numbers.append(token)
        }
        // Legacy ids (claude-3-5-haiku) put it before.
        if numbers.isEmpty, familyIndex > 0 {
            var index = familyIndex - 1
            while index >= 0, tokens[index].allSatisfy(\.isNumber), tokens[index].count < 8 {
                numbers.insert(tokens[index], at: 0)
                index -= 1
            }
        }
        return ClaudeModelInfo(family: family, version: numbers.prefix(2).joined(separator: "."))
    }
}

/// An actual model observed in local data; different versions remain separate consumers.
public struct DiscoveredModel: Hashable, Sendable {
    public let modelId: String
    public let lastSeen: Date

    public init(modelId: String, lastSeen: Date) {
        self.modelId = modelId
        self.lastSeen = lastSeen
    }

    public var descriptor: AgentDescriptor {
        AgentDescriptor(
            id: "claude-model:\(modelId)",
            vendor: "Claude",
            model: ClaudeModelInfo.parse(modelId)?.displayName ?? modelId,
            source: L10n.sourceClaudeCode,
            enabled: true
        )
    }
}

public enum ClaudeModelDiscovery {
    /// Every model with usage, including subagent models and models outside the known Claude families.
    public static func discover(_ observations: [(modelId: String, seenAt: Date)]) -> [DiscoveredModel] {
        var latest: [String: DiscoveredModel] = [:]
        for observation in observations {
            guard observation.modelId != "<synthetic>", !observation.modelId.isEmpty else { continue }
            if let existing = latest[observation.modelId], existing.lastSeen >= observation.seenAt { continue }
            latest[observation.modelId] = DiscoveredModel(modelId: observation.modelId, lastSeen: observation.seenAt)
        }
        return latest.values.sorted { $0.lastSeen == $1.lastSeen ? $0.modelId < $1.modelId : $0.lastSeen > $1.lastSeen }
    }
}
