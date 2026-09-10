import Foundation

/// The engine reports only "max" through get_usage; its account profile supplies the specific tier.
enum ClaudeSubscription {
    static var accountProfileURL: URL {
        let directory = ProcessInfo.processInfo.environment["CLAUDE_CONFIG_DIR"]
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? FileManager.default.homeDirectoryForCurrentUser
        return directory.appendingPathComponent(".claude.json")
    }

    static func plan(type: String?, profileData: Data?) -> String? {
        guard type == "max", let profileData,
              let profile = try? JSONDecoder().decode(Profile.self, from: profileData),
              let account = profile.oauthAccount, account.organizationType == "claude_max"
        else { return type }
        switch account.userRateLimitTier ?? account.organizationRateLimitTier {
        case "default_claude_max_5x": return "max_5x"
        case "default_claude_max_20x": return "max_20x"
        default: return type
        }
    }

    private struct Profile: Decodable {
        let oauthAccount: Account?

        struct Account: Decodable {
            let organizationType: String?
            let organizationRateLimitTier: String?
            let userRateLimitTier: String?
        }
    }
}
