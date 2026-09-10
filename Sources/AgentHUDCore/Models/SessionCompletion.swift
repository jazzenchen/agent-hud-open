import AgentHUDSupport
import Foundation

/// An explicit, successful end of one turn. Inactivity is never a completion.
public struct SessionCompletion: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let sessionID: String
    public let vendor: String
    public let task: String
    public let model: String
    public let startedAt: Date?
    public let completedAt: Date

    public init(sessionID: String, vendor: String, turnID: String, task: String, model: String,
                startedAt: Date?, completedAt: Date) {
        id = RecordCoding.hash([vendor, sessionID, turnID])
        self.sessionID = sessionID; self.vendor = vendor; self.task = task; self.model = model
        self.startedAt = startedAt; self.completedAt = completedAt
    }
}
