import XCTest
@testable import AgentHUDCore

final class SubscriptionTests: XCTestCase {
    func testPlanBadgesUseProviderSpecificNames() {
        for (id, plan, expected) in [
            ("claude-code", "max_5x", "Max x5"),
            ("claude-code", "max_20x", "Max x20"),
            ("claude-code", "max", "Max"),
            ("claude-code", "pro", "Pro"),
            ("codex-cli", "prolite", "Pro x5"),
            ("codex-cli", "pro", "Pro x20"),
            ("codex-cli", "plus", "Plus"),
            ("codex-cli", "enterprise", "Enterprise"),
        ] {
            let source = SourceStatus(id: id, name: "Agent", detail: "", state: .ready(plan: plan))
            XCTAssertEqual(source.planLabel, expected)
            XCTAssertEqual(source.statusLabel, L10n.text("已就绪", "Ready"), "the plan belongs beside the agent name")
        }
    }

    func testMissingSubscriptionsDoNotShowABadge() {
        let states: [SourceStatus.State] = [.ready(plan: nil), .ready(plan: ""), .installed, .notDetected, .unavailable, .needsAuthorization]
        for state in states {
            XCTAssertNil(SourceStatus(id: "deepseek", name: "DeepSeek", detail: "", state: state).planLabel)
        }
    }

    func testClaudeMaxUsesTheAccountTier() throws {
        for (tier, expected) in [("default_claude_max_5x", "max_5x"), ("default_claude_max_20x", "max_20x")] {
            XCTAssertEqual(ClaudeSubscription.plan(type: "max", profileData: try profile(tier: tier)), expected)
        }
    }

    func testClaudeMissingOrUnknownMetadataDoesNotInventAMultiplier() throws {
        let profiles: [Data?] = [nil, Data("invalid".utf8), Data("{}".utf8), try profile(tier: "future_tier"),
                                try profile(tier: "default_claude_max_20x", organization: "claude_team")]
        for data in profiles {
            XCTAssertEqual(ClaudeSubscription.plan(type: "max", profileData: data), "max")
        }
        let max = try profile(tier: "default_claude_max_20x")
        XCTAssertNil(ClaudeSubscription.plan(type: nil, profileData: max), "a cached profile cannot create a ready subscription")
        XCTAssertEqual(ClaudeSubscription.plan(type: "pro", profileData: max), "pro", "the engine remains authoritative for the current plan")
    }

    func testClaudeUserTierTakesPrecedenceOverOrganizationTier() throws {
        let data = try profile(tier: "default_claude_max_20x", userTier: "default_claude_max_5x")
        XCTAssertEqual(ClaudeSubscription.plan(type: "max", profileData: data), "max_5x")
        let unknown = try profile(tier: "default_claude_max_20x", userTier: "future_tier")
        XCTAssertEqual(ClaudeSubscription.plan(type: "max", profileData: unknown), "max")
    }

    func testClaudeProviderCarriesTheDetailedPlanIntoItsReport() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-subscription-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let profileURL = directory.appendingPathComponent(".claude.json")
        try profile(tier: "default_claude_max_20x").write(to: profileURL)
        let executable = directory.appendingPathComponent("claude")
        let response = #"{"type":"control_response","response":{"subtype":"success","response":{"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":21,"resets_at":null}}}}}"#
        try "#!/bin/sh\nread -r request\necho '\(response)'\n".write(to: executable, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        let provider = ClaudeCodeProvider(
            engine: .init(executable: executable, workingDirectory: directory),
            transcripts: .init(roots: []), history: .init(fileURL: nil), accountProfileURL: profileURL
        )
        let report = try await provider.fetchUsage(agents: [], historyHours: 1)
        XCTAssertEqual(report.subscriptionType, "max")
        XCTAssertEqual(report.subscriptions["Claude"], "max_20x")
        XCTAssertEqual(report.snapshot(for: ClaudeUsage.sessionRowId)?.remainingPct, 79)
    }

    private func profile(tier: String, organization: String = "claude_max", userTier: String? = nil) throws -> Data {
        var account = ["organizationType": organization, "organizationRateLimitTier": tier]
        account["userRateLimitTier"] = userTier
        return try JSONSerialization.data(withJSONObject: ["oauthAccount": account])
    }
}
