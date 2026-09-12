import XCTest
@testable import AgentHUDCore

final class ClaudeEngineTests: XCTestCase {
    /// Trimmed copy of a real `get_usage` response (Max plan).
    static let response = #"{"type":"control_response","response":{"subtype":"success","request_id":"agent-hud-usage","response":{"session":{"total_cost_usd":0,"total_api_duration_ms":0,"total_duration_ms":7314,"total_lines_added":0,"total_lines_removed":0,"model_usage":{}},"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":21,"resets_at":"2026-09-07T10:20:00.182540+00:00","limit_dollars":null},"seven_day":{"utilization":70,"resets_at":"2026-09-12T18:00:00.182559+00:00"},"seven_day_opus":null,"seven_day_sonnet":null,"extra_usage":{"is_enabled":false},"limits":[{"kind":"session","group":"session","percent":21}]},"behaviors":null}}}"#

    func testParsesRealResponse() throws {
        let parsed = try XCTUnwrap(ClaudeEngineUsage.parse(line: Self.response))
        XCTAssertTrue(parsed.rateLimitsAvailable)
        XCTAssertEqual(parsed.subscriptionType, "max")
        XCTAssertEqual(parsed.usage.fiveHour?.utilizationPct, 21)
        XCTAssertEqual(parsed.usage.fiveHour?.remainingPct, 79)
        XCTAssertEqual(parsed.usage.sevenDay?.utilizationPct, 70)
        XCTAssertNil(parsed.usage.sevenDayOpus)
        let resets = try XCTUnwrap(parsed.usage.fiveHour?.resetsAt)
        XCTAssertEqual(resets.timeIntervalSince1970, DateParsing.iso8601("2026-09-07T10:20:00Z")!.timeIntervalSince1970, accuracy: 1, "microsecond offsets parse")
    }

    func testModelScopedWeeklyLimits() throws {
        let json = #"{"five_hour":{"utilization":21},"seven_day":{"utilization":70},"seven_day_opus":null,"limits":[{"kind":"session","group":"session","percent":21},{"kind":"weekly_all","group":"weekly","percent":70},{"kind":"weekly_scoped","group":"weekly","percent":44,"resets_at":"2026-09-12T17:59:59.652963+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null}}]}"#
        let usage = try ClaudeUsage.parse(Data(json.utf8))
        XCTAssertEqual(usage.modelWeekly["fable"]?.utilizationPct, 44)
        XCTAssertEqual(usage.weekly(for: "claude-fable")?.remainingPct, 56)
        XCTAssertEqual(usage.weekly(for: "claude-sonnet")?.utilizationPct, 70, "families without a scoped window use the shared one")
        XCTAssertNotNil(usage.modelWeekly["fable"]?.resetsAt)
    }

    func testIgnoresOtherStreamLines() throws {
        XCTAssertNil(try ClaudeEngineUsage.parse(line: #"{"type":"system","subtype":"hook_started"}"#))
        XCTAssertNil(try ClaudeEngineUsage.parse(line: "not json"))
    }

    func testErrorResponseThrows() {
        let line = #"{"type":"control_response","response":{"subtype":"error","request_id":"x","error":"get_usage is not supported in this context"}}"#
        XCTAssertThrowsError(try ClaudeEngineUsage.parse(line: line)) { error in
            XCTAssertEqual(error as? ClaudeDataError, .engineFailed("get_usage is not supported in this context"))
        }
    }

    func testUnavailableLimits() throws {
        let line = #"{"type":"control_response","response":{"subtype":"success","request_id":"x","response":{"subscription_type":null,"rate_limits_available":false,"rate_limits":null}}}"#
        let parsed = try XCTUnwrap(ClaudeEngineUsage.parse(line: line))
        XCTAssertFalse(parsed.rateLimitsAvailable)
        XCTAssertNil(parsed.usage.fiveHour)
    }

    func testLocatorPrefersNewestVersion() throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-home-\(UUID().uuidString)", isDirectory: true)
        let versions = home.appendingPathComponent(".local/share/claude/versions", isDirectory: true)
        try FileManager.default.createDirectory(at: versions, withIntermediateDirectories: true)
        for name in ["2.1.9", "2.1.245", "2.1.100"] {
            let url = versions.appendingPathComponent(name)
            try "#!/bin/sh\n".write(to: url, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
        }
        XCTAssertEqual(ClaudeEngineLocator.find(home: home)?.lastPathComponent, "2.1.245")
        let bin = home.appendingPathComponent(".local/bin/claude")
        try FileManager.default.createDirectory(at: bin.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: bin, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: bin.path)
        XCTAssertEqual(ClaudeEngineLocator.find(home: home), bin, "the user-facing symlink wins when present")
        XCTAssertNil(ClaudeEngineLocator.find(home: home.appendingPathComponent("missing")))
    }

    func testClientRunsAFakeEngine() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-engine-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let fake = dir.appendingPathComponent("claude")
        let script = """
        #!/bin/bash
        read -r line
        echo '{"type":"system","subtype":"init"}'
        echo '\(Self.response)'
        sleep 5
        """
        try script.write(to: fake, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fake.path)
        let client = ClaudeEngineUsageClient(executable: fake, workingDirectory: dir, timeout: 10)
        let started = Date()
        let usage = try await client.fetch()
        XCTAssertEqual(usage.usage.fiveHour?.remainingPct, 79)
        XCTAssertLessThan(Date().timeIntervalSince(started), 4, "returns as soon as the response line arrives")
    }

    func testClientTimesOut() async {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-engine-\(UUID().uuidString)", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let fake = dir.appendingPathComponent("claude")
        try? "#!/bin/bash\nsleep 30\n".write(to: fake, atomically: true, encoding: .utf8)
        try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fake.path)
        let client = ClaudeEngineUsageClient(executable: fake, workingDirectory: dir, timeout: 1)
        do {
            _ = try await client.fetch()
            XCTFail("expected timeout")
        } catch let error as ClaudeDataError {
            if case .engineFailed = error {} else { XCTFail("unexpected \(error)") }
        } catch {
            XCTFail("unexpected \(error)")
        }
    }

    func testProviderKeepsExpiredReadingUntilEngineConfirmsReset() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-reset-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let fake = dir.appendingPathComponent("claude")
        try "#!/bin/bash\nread -r line\necho '\(Self.response)'\n".write(to: fake, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fake.path)
        let now = try XCTUnwrap(DateParsing.iso8601("2026-09-08T00:00:00Z"))
        let provider = ClaudeCodeProvider(engine: .init(executable: fake, workingDirectory: dir),
                                         transcripts: .init(roots: []), history: .init(fileURL: nil), clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 2)
        let quota = try XCTUnwrap(report.snapshot(for: ClaudeUsage.sessionRowId))
        XCTAssertEqual(quota.remainingPct, 79, "A passed deadline must not fabricate full quota")
        XCTAssertLessThan(try XCTUnwrap(quota.resetAt), now)
        let cached = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 2)
        XCTAssertEqual(cached.snapshot(for: ClaudeUsage.sessionRowId), quota)
    }
}

final class ISO8601FastTests: XCTestCase {
    func testMatchesFoundationParser() {
        let samples = [
            "2026-09-07T05:41:44.123Z",
            "2026-09-07T05:41:44Z",
            "2026-09-07T10:20:00.182540+00:00",
            "2024-02-29T23:59:59.999Z",
            "2026-01-01T00:00:00+08:00",
            "1999-12-31T12:00:00-05:30",
        ]
        for sample in samples {
            let fast = ISO8601Fast.parse(sample)
            let slow = DateParsing.iso8601(sample)
            XCTAssertNotNil(fast, sample)
            XCTAssertEqual(fast?.timeIntervalSince1970 ?? -1, slow?.timeIntervalSince1970 ?? -2, accuracy: 0.0005, sample)
        }
        XCTAssertNil(ISO8601Fast.parse("2026-13-01T00:00:00Z"))
        XCTAssertNil(ISO8601Fast.parse("nonsense"))
        XCTAssertEqual(ISO8601Fast.daysFromCivil(year: 1970, month: 1, day: 1), 0)
        XCTAssertEqual(ISO8601Fast.daysFromCivil(year: 2000, month: 3, day: 1), 11017)
    }
}
