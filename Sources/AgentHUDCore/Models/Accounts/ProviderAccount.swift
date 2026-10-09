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

    /// A billing pool's account, which keeps its pool's id.
    public var isBillingPool: Bool { id.hasPrefix("pool:") }
}

/// One account as seen from one client home. A provider's list in `UsageReport.accounts` is authoritative:
/// accounts it no longer reports stay as last readings until they have not been seen for the history retention.
public struct AccountObservation: Hashable, Codable, Sendable, Identifiable {
    public let account: ProviderAccount
    /// `ClientHome.key` of the directory the client was read from; empty for the client's default home.
    public let home: String
    /// The client that owns the account's reading; its provider determines the subscription group and account identity.
    public let client: String
    /// An email or name the provider already returned, shown only to this Mac's user.
    public let label: String?
    public let plan: String?
    public let observedAt: Date
    /// The client is currently signed in to this account. Other accounts show their last reading only.
    public let isCurrent: Bool
    /// The reason of `readingIssue`, written beside it for readers of the notice text; an observation saved without a
    /// typed issue counts it as a failed read.
    public let quotaNotice: String?
    /// A failed quota read keeps the last observation, but cannot confirm quota events; neither can a reading whose
    /// account the provider could not confirm.
    public let readingIssue: ReadingIssue?
    /// Codex credits belong to this account, even when several clients are signed in.
    public let resetCredits: CodexResetCredits?
    /// Keys this account was filed under while its identity was incomplete, such as a Codex workspace read without its
    /// email. Last readings under them are this account's own and leave once it is read.
    public let aliases: [String]?
    /// The provider explicitly listed every enabled quota window for this account, using account-scoped row ids.
    /// Nil is an incremental or failed reading; an empty set explicitly confirms that no windows remain.
    public let quotaWindowIDs: Set<String>?
    /// Reported account money, with each wallet's own reading time. Nil when this source reports no wallet values.
    public let wallets: [AccountWallet]?
    /// Informational provenance of a successful reading, shown in source details rather than as a warning.
    public let sourceInfo: String?

    public init(account: ProviderAccount, home: String = "", client: String? = nil, label: String? = nil, plan: String? = nil,
                observedAt: Date, isCurrent: Bool = true, quotaNotice: String? = nil, readingIssue: ReadingIssue? = nil,
                resetCredits: CodexResetCredits? = nil, aliases: [String]? = nil, quotaWindowIDs: Set<String>? = nil,
                wallets: [AccountWallet]? = nil, sourceInfo: String? = nil) {
        self.account = account
        self.home = home
        self.client = client ?? (account.provider == "Codex" && home.hasPrefix("pi:") ? "Pi" : account.provider)
        self.label = label.flatMap { $0.isEmpty ? nil : $0 }
        self.plan = plan.flatMap { $0.isEmpty ? nil : $0 }
        self.observedAt = observedAt
        self.isCurrent = isCurrent
        self.quotaNotice = quotaNotice
        self.readingIssue = readingIssue
        self.resetCredits = resetCredits
        self.aliases = aliases.flatMap { $0.isEmpty ? nil : $0 }
        self.quotaWindowIDs = quotaWindowIDs
        self.wallets = wallets.flatMap { $0.isEmpty ? nil : $0 }
        self.sourceInfo = sourceInfo
    }

    public var id: String { account.id + "@" + home }

    public func with(isCurrent: Bool) -> AccountObservation {
        AccountObservation(account: account, home: home, client: client, label: label, plan: plan, observedAt: observedAt,
                           isCurrent: isCurrent, quotaNotice: quotaNotice, readingIssue: readingIssue, resetCredits: resetCredits,
                           aliases: aliases, quotaWindowIDs: quotaWindowIDs, wallets: wallets, sourceInfo: sourceInfo)
    }

    private enum CodingKeys: String, CodingKey {
        case account, home, client, label, plan, observedAt, isCurrent, quotaNotice, readingIssue, resetCredits, aliases, quotaWindowIDs
        case wallets, sourceInfo
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(account: try values.decode(ProviderAccount.self, forKey: .account),
            home: try values.decode(String.self, forKey: .home), client: try values.decodeIfPresent(String.self, forKey: .client),
            label: try values.decodeIfPresent(String.self, forKey: .label), plan: try values.decodeIfPresent(String.self, forKey: .plan),
            observedAt: try values.decode(Date.self, forKey: .observedAt), isCurrent: try values.decode(Bool.self, forKey: .isCurrent),
            quotaNotice: try values.decodeIfPresent(String.self, forKey: .quotaNotice),
            readingIssue: try values.decodeIfPresent(ReadingIssue.self, forKey: .readingIssue),
            resetCredits: try values.decodeIfPresent(CodexResetCredits.self, forKey: .resetCredits),
            aliases: try values.decodeIfPresent([String].self, forKey: .aliases),
            quotaWindowIDs: try values.decodeIfPresent(Set<String>.self, forKey: .quotaWindowIDs),
            wallets: try values.decodeIfPresent([AccountWallet].self, forKey: .wallets),
            sourceInfo: try values.decodeIfPresent(String.self, forKey: .sourceInfo))
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

    /// The header label of this reading judged on its own, without the report it came in.
    public func statusLabel(now: Date) -> String {
        ReadingAssessment(status: ownStatus, isCurrentAccount: isCurrent, observedAt: observedAt, now: now).accountLabel(now: now)
    }
}

public extension UsageReport {
    /// The reason of the window's `status(of:)`, kept for hosts that read the notice text.
    func quotaNotice(for agent: AgentDescriptor) -> String? { status(of: .window(agent)).reason }

    /// The reason of a vendor's status: its notice about a quota or balance reading; a notice about its local logs or
    /// hooks is not one. Kept for hosts that read the notice text.
    func quotaNotice(vendor: String) -> String? { vendorStatus(vendor).reason }

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
