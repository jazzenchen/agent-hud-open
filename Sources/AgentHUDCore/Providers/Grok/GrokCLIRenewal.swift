import Foundation
import os

/// The CLI's access token lasts six hours and only the CLI renews it, when it runs, so on a Mac where Grok has not run
/// since, the billing service rejects the stored token and no quota can be read. Running `grok models` lets the CLI
/// renew the token under its own lock and write it back. The refresh token is never used here: a rotated one the CLI
/// did not store would sign it out.
enum GrokCLIRenewal {
    /// The pause after an attempt; each failed renewal doubles it, up to `longestPause`, and a renewal resets it.
    static let retryInterval: TimeInterval = 900
    static let longestPause: TimeInterval = 4 * 3600
    static let timeout: TimeInterval = 30
    private static let schedule = OSAllocatedUnfairLock(initialState: Schedule())

    /// When the CLI may be started again, so a signed-out CLI or a Mac without network does not start it on every
    /// account refresh, nor every `retryInterval` for as long as the app runs.
    struct Schedule: Equatable {
        var last: Date?
        var pause = GrokCLIRenewal.retryInterval

        /// Whether an attempt may start at `now`; one that may is recorded as the last.
        mutating func begin(at now: Date) -> Bool {
            if let last, now.timeIntervalSince(last) < pause { return false }
            last = now
            return true
        }

        mutating func finish(renewed: Bool) {
            pause = renewed ? GrokCLIRenewal.retryInterval : min(pause * 2, GrokCLIRenewal.longestPause)
        }
    }

    /// Only the Grok CLI as its installer places it, in `$GROK_HOME/bin` or the default home's. The link the installer
    /// adds in `~/.local/bin`, a directory other installers share, and `PATH` or Homebrew are not searched, so no other
    /// program named `grok` is started.
    static func candidates(home: URL, grokHome: URL) -> [URL] {
        let installed = grokHome.appendingPathComponent("bin/grok"), standard = home.appendingPathComponent(".grok/bin/grok")
        return installed.standardizedFileURL == standard.standardizedFileURL ? [installed] : [installed, standard]
    }

    /// Every stored login has expired, and one can still be renewed by the CLI.
    static func isRenewable(_ logins: [ProviderJSON], now: Date) -> Bool {
        !logins.isEmpty
            && logins.contains { $0["refresh_token"].stringValue?.isEmpty == false }
            && logins.allSatisfy { DateParsing.internet($0["expires_at"].stringValue).map { $0 <= now } ?? true }
    }

    /// Whether the CLI ran, exited cleanly and left a login that has not expired; false without running it while
    /// `Schedule` holds it back.
    static func renew(grokHome: URL, now: Date) async -> Bool {
        guard schedule.withLock({ $0.begin(at: now) }) else { return false }
        let renewed = await run(grokHome: grokHome, now: now)
        schedule.withLock { $0.finish(renewed: renewed) }
        return renewed
    }

    private static func run(grokHome: URL, now: Date) async -> Bool {
        guard let executable = Executables.first(candidates(home: FileManager.default.homeDirectoryForCurrentUser, grokHome: grokHome)),
              let output = try? await ChildProcess.run(executable, ["models"], environment: ProcessInfo.processInfo.environment,
                                                       timeout: timeout, stdoutLimit: 256 * 1024),
              output.status == 0,
              let json = try? ProviderFiles.json(grokHome.appendingPathComponent("auth.json")) else { return false }
        return GrokClient.loginEntries(json).contains { DateParsing.internet($0["expires_at"].stringValue).map { $0 > now } == true }
    }
}
