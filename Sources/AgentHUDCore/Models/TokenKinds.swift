import Foundation

/// The five ways a model call spends tokens. Logs count cache writes inside input and reasoning inside output; here
/// input and output leave those parts out, and a log that does not tell them apart puts everything in input and output.
public enum TokenKind: Int, CaseIterable, Hashable, Sendable {
    /// Display order: what a call adds, from the base of a stack up, then the context it reads back from the cache.
    case cacheWrite, input, reasoning, output, cacheRead

    public var label: String {
        switch self {
        case .cacheWrite: "Cache write"
        case .input: "Input"
        case .reasoning: "Reasoning"
        case .output: "Output"
        case .cacheRead: "Cache read"
        }
    }

    public var dimension: TokenDimensions {
        switch self {
        case .cacheWrite: .cacheWrite
        case .input: .input
        case .reasoning: .reasoning
        case .output: .output
        case .cacheRead: .cacheRead
        }
    }
}

/// Token counts by kind.
public struct TokenKinds: Hashable, Codable, Sendable {
    public var cacheWrite: Int
    public var input: Int
    public var reasoning: Int
    public var output: Int
    public var cacheRead: Int

    public init(cacheWrite: Int = 0, input: Int = 0, reasoning: Int = 0, output: Int = 0, cacheRead: Int = 0) {
        self.cacheWrite = cacheWrite; self.input = input; self.reasoning = reasoning; self.output = output; self.cacheRead = cacheRead
    }

    /// From a log's counts, where input includes its cache writes and output its reasoning.
    public init(tokensIn: Int, tokensOut: Int, cacheRead: Int, cacheWrite: Int = 0, reasoning: Int = 0) {
        let write = min(max(0, cacheWrite), max(0, tokensIn)), thought = min(max(0, reasoning), max(0, tokensOut))
        self.init(cacheWrite: write, input: tokensIn - write, reasoning: thought, output: tokensOut - thought, cacheRead: cacheRead)
    }

    public subscript(kind: TokenKind) -> Int {
        switch kind {
        case .cacheWrite: cacheWrite
        case .input: input
        case .reasoning: reasoning
        case .output: output
        case .cacheRead: cacheRead
        }
    }

    public var total: Int { cacheWrite + input + reasoning + output + cacheRead }
    /// What the calls added rather than read back from the cache.
    public var new: Int { total - cacheRead }
    public var isEmpty: Bool { total == 0 }

    /// The share of the prompts read back from the cache: cache reads against fresh input, cache writes and cache reads.
    /// nil where nothing was cached, since a log that does not count its cache says nothing about hits.
    public var cacheHitRate: Double? {
        guard cacheRead + cacheWrite > 0 else { return nil }
        return Double(cacheRead) / Double(input + cacheWrite + cacheRead)
    }

    public static func + (lhs: Self, rhs: Self) -> Self {
        Self(cacheWrite: lhs.cacheWrite + rhs.cacheWrite, input: lhs.input + rhs.input, reasoning: lhs.reasoning + rhs.reasoning,
             output: lhs.output + rhs.output, cacheRead: lhs.cacheRead + rhs.cacheRead)
    }

    public static func += (lhs: inout Self, rhs: Self) { lhs = lhs + rhs }

    public static func - (lhs: Self, rhs: Self) -> Self {
        Self(cacheWrite: lhs.cacheWrite - rhs.cacheWrite, input: lhs.input - rhs.input, reasoning: lhs.reasoning - rhs.reasoning,
             output: lhs.output - rhs.output, cacheRead: lhs.cacheRead - rhs.cacheRead)
    }
}
