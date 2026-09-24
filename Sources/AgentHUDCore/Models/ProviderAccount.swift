import AgentHUDSupport
import Foundation

/// Whose quota a reading describes. Every quota row belongs to one account, and the account id is the storage key
/// for its readings, history and display settings. Identities are hashes of provider-owned ids, never credentials.
public struct ProviderAccount: Hashable, Codable, Sendable, Identifiable {
    /// `account`: the provider named the user or workspace. `credential`: only a local login record did.
    /// `unresolved`: a signed-in client without identity, kept separate per client home.
    public enum Evidence: String, Codable, Sendable { case account, credential, unresolved }

    public let id: String
    public let provider: String
    public let evidence: Evidence

    /// `user` and `workspace` are hashes; an empty value means the provider has no such level.
    /// Evidence is not part of the id, so confirming an identity later keeps the same key.
    public init(provider: String, user: String, workspace: String, evidence: Evidence) {
        id = "account:" + RecordCoding.hash([provider, user, workspace])
        self.provider = provider
        self.evidence = evidence
    }

    /// Billing pools already carry an account-scoped id; their rows keep it.
    public init(pool: BillingPool) {
        id = pool.id
        provider = pool.provider
        evidence = switch pool.evidence {
        case .account: .account
        case .credential: .credential
        case .unresolved: .unresolved
        }
    }

    /// Hashes provider-owned user and workspace ids. Nil when the provider returned neither.
    public static func identified(provider: String, user: String?, workspace: String?,
                                  evidence: Evidence = .account) -> ProviderAccount? {
        let user = user?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let workspace = workspace?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard !user.isEmpty || !workspace.isEmpty else { return nil }
        return ProviderAccount(provider: provider, user: user.isEmpty ? "" : RecordCoding.hash([user]),
                               workspace: workspace.isEmpty ? "" : RecordCoding.hash([workspace]), evidence: evidence)
    }

    /// A client that reports quota without naming the account. Different homes never share such an account.
    public static func unresolved(provider: String, home: String) -> ProviderAccount {
        ProviderAccount(provider: provider, user: "", workspace: "home:" + home, evidence: .unresolved)
    }

    /// Row id of one provider window within this account, e.g. `account:<hash>/codex`.
    public func windowID(_ window: String) -> String { id + "/" + window }
}

/// One account as seen from one client home. A provider's list in `UsageReport.accounts` is authoritative:
/// accounts it no longer reports stay as last readings until they have not been seen for the history retention.
public struct AccountObservation: Hashable, Codable, Sendable, Identifiable {
    public let account: ProviderAccount
    /// `ClientHome.key` of the directory the client was read from; empty for the client's default home.
    public let home: String
    /// An email or name the provider already returned, shown only to this Mac's user.
    public let label: String?
    public let plan: String?
    public let observedAt: Date
    /// The client is currently signed in to this account. Other accounts show their last reading only.
    public let isCurrent: Bool
    /// A failed quota read keeps the last observation, but cannot confirm quota events.
    public let quotaNotice: String?
    /// Codex credits belong to this account, even when several clients are signed in.
    public let resetCredits: CodexResetCredits?
    /// Keys this account was filed under while its identity was incomplete, such as a Codex workspace read without its
    /// email. Last readings under them are this account's own and leave once it is read.
    public let aliases: [String]?

    public init(account: ProviderAccount, home: String = "", label: String? = nil, plan: String? = nil,
                observedAt: Date, isCurrent: Bool = true, quotaNotice: String? = nil, resetCredits: CodexResetCredits? = nil,
                aliases: [String]? = nil) {
        self.account = account
        self.home = home
        self.label = label.flatMap { $0.isEmpty ? nil : $0 }
        self.plan = plan.flatMap { $0.isEmpty ? nil : $0 }
        self.observedAt = observedAt
        self.isCurrent = isCurrent
        self.quotaNotice = quotaNotice
        self.resetCredits = resetCredits
        self.aliases = aliases.flatMap { $0.isEmpty ? nil : $0 }
    }

    public var id: String { account.id + "@" + home }

    public func with(isCurrent: Bool) -> AccountObservation {
        AccountObservation(account: account, home: home, label: label, plan: plan, observedAt: observedAt,
                           isCurrent: isCurrent, quotaNotice: quotaNotice, resetCredits: resetCredits, aliases: aliases)
    }

    /// The account's email or name, else a short form of its id.
    public var displayName: String {
        label ?? L10n.text("账户 ", "Account ") + String(account.id.split(separator: ":").last?.prefix(6) ?? "")
    }

    /// Plan badge text in the provider's own tier names.
    public var planLabel: String? {
        let source = account.provider == "Claude" ? "claude-code" : account.provider == "Codex" ? "codex-cli" : account.provider
        return SourceStatus(id: source, name: account.provider, detail: "", state: .ready(plan: plan)).planLabel
    }

    public func statusLabel(now: Date) -> String {
        if isCurrent, quotaNotice == nil { return L10n.text("当前账户", "Current account") }
        let ago = Countdown.formatRough(max(0, now.timeIntervalSince(observedAt)))
        return L10n.text("上次读取 \(ago) 前", "Last read \(ago) ago")
    }
}

/// The client home dimension: one client can run from several data directories with different sign-ins.
public enum ClientHome {
    /// Empty for the client's default directory, otherwise a hash of the resolved path.
    public static func key(_ directory: URL, defaultDirectory: URL) -> String {
        let path = directory.standardizedFileURL.resolvingSymlinksInPath().path
        return path == defaultDirectory.standardizedFileURL.resolvingSymlinksInPath().path ? "" : RecordCoding.hash([path])
    }
}

public extension UsageReport {
    func quotaNotice(for agent: AgentDescriptor) -> String? {
        if let id = agent.account?.id, let account = observation(accountID: id) { return account.quotaNotice ?? sourceNotices[agent.vendor] }
        return sourceNotices[agent.vendor]
    }

    /// Older reports carry one unscoped credit balance. It is safe only with a single current account.
    func resetCredits(for accountID: String) -> CodexResetCredits? {
        if let account = observation(accountID: accountID), let credits = account.resetCredits { return credits }
        let current = Set((accounts?["Codex"] ?? []).filter(\.isCurrent).map(\.account.id))
        return current.count <= 1 && (current.isEmpty || current.contains(accountID)) ? codexResetCredits : nil
    }
    var accountObservations: [AccountObservation] { (accounts ?? [:]).values.flatMap { $0 } }

    /// The newest observation of an account, preferring homes where it is current.
    func observation(accountID: String) -> AccountObservation? {
        accountObservations.filter { $0.account.id == accountID }
            .max { ($0.isCurrent ? 1 : 0, $0.observedAt) < ($1.isCurrent ? 1 : 0, $1.observedAt) }
    }

    /// Rows without an account (demo data, placeholders) count as current.
    func isCurrent(_ agent: AgentDescriptor) -> Bool {
        guard let id = agent.account?.id, let observations = accounts?[agent.account!.provider] else { return true }
        let matches = observations.filter { $0.account.id == id }
        return matches.isEmpty || matches.contains(where: \.isCurrent)
    }
}
