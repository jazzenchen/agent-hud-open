import AgentHUDSupport
import Foundation

/// An explicitly identified agent turn. Timestamps belong to source events, never a cache read.
public struct SessionTurn: Codable, Hashable, Sendable, Identifiable {
    public enum State: String, Codable, Sendable { case running, completed, ended }
    public let provider: String
    public let sessionID: String
    public let turnID: String
    public let state: State
    public let startedAtMs: Int64?
    public let observedAtMs: Int64
    public var id: String { RecordCoding.hash([provider, sessionID, turnID]) }

    public init(provider: String, sessionID: String, turnID: String, state: State,
                startedAtMs: Int64?, observedAtMs: Int64) {
        self.provider = provider; self.sessionID = sessionID; self.turnID = turnID
        self.state = state; self.startedAtMs = startedAtMs; self.observedAtMs = observedAtMs
    }
}
