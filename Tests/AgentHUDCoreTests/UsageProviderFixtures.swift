import AgentHUDCore

extension UsageProvider {
    /// Parser/account fixtures need both results; responsiveness tests exercise the two operations separately.
    func fetchAccountAndLocalUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        await refreshAccountUsage(historyHours: historyHours)
        return try await fetchUsage(agents: agents, historyHours: historyHours)
    }
}
