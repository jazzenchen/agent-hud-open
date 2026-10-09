import Foundation

/// Whether a reading can be trusted, apart from its age: its read succeeded, the last read failed and the value shown is
/// the last good one, or the read succeeded but whose account it describes is not confirmed.
public enum ReadingStatus: Hashable, Sendable {
    case normal
    case readFailed(reason: String)
    case unverified(reason: String)

    /// Why the reading is held back; nil for a normal one.
    public var reason: String? {
        switch self {
        case .normal: nil
        case .readFailed(let reason), .unverified(let reason): reason
        }
    }

    public var isNormal: Bool { self == .normal }
}

/// What a provider says about a reading, at the scope where it read: a vendor, client home, account or plan pool, or balance.
public struct ReadingIssue: Hashable, Codable, Sendable {
    public enum Kind: String, Codable, Sendable {
        case readFailed, unverified
    }

    public let kind: Kind
    public let reason: String

    public init(_ kind: Kind, reason: String) {
        self.kind = kind
        self.reason = reason
    }

    public static func readFailed(_ reason: String) -> ReadingIssue { ReadingIssue(.readFailed, reason: reason) }
    public static func unverified(_ reason: String) -> ReadingIssue { ReadingIssue(.unverified, reason: reason) }

    public var status: ReadingStatus {
        switch kind {
        case .readFailed: .readFailed(reason: reason)
        case .unverified: .unverified(reason: reason)
        }
    }

    private enum CodingKeys: String, CodingKey { case kind, reason }

    /// A kind this version does not know, or a missing field, reads as a failed read, so that a saved report never fails
    /// to decode over one issue.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        kind = (try? container.decode(String.self, forKey: .kind)).flatMap(Kind.init(rawValue:)) ?? .readFailed
        reason = (try? container.decode(String.self, forKey: .reason)) ?? ""
    }
}

/// A reading to judge: a quota window's, an account's, or an API account's balance.
public enum ReadingSubject: Sendable {
    case window(AgentDescriptor)
    case account(AccountObservation)
    case balance(APIBilling)
}

/// Everything the surfaces weigh about one reading at one time. `status` does not depend on the time; the other facets
/// come from the reading's time and, for a window, its reset.
public struct ReadingAssessment: Hashable, Sendable {
    public private(set) var status: ReadingStatus
    /// A window of the account the client is signed in to, or of no known account. Balances always count as current.
    public let isCurrentAccount: Bool
    /// When the reading was taken; nil when there is none.
    public let observedAt: Date?
    /// The source reported a numeric value for this reading.
    public let hasValue: Bool
    /// At least `AlertPolicy.maximumReadingAge` old.
    public let isStale: Bool
    /// Taken after the time it is judged at.
    public let isFromFuture: Bool
    /// The window's reset time has passed, and no reading has confirmed the reset yet.
    public let isResetPending: Bool

    init(status: ReadingStatus, isCurrentAccount: Bool, observedAt: Date?, resetAt: Date? = nil, hasValue: Bool = true, now: Date) {
        self.status = status
        self.isCurrentAccount = isCurrentAccount
        self.observedAt = observedAt
        self.hasValue = hasValue
        isStale = observedAt.map { now.timeIntervalSince($0) >= AlertPolicy.maximumReadingAge } ?? false
        isFromFuture = observedAt.map { $0 > now } ?? false
        isResetPending = resetAt.map { $0 <= now } ?? false
    }

    /// The reading gives its window a status level: it has a value, is normal, of the current account, not taken in the future, and
    /// its reset has not passed. It keeps the level however old it grows, since collection reads a client again only
    /// when its work or a reset makes a new reading worth taking.
    public var showsLevel: Bool {
        hasValue && status.isNormal && isCurrentAccount && observedAt != nil && !isFromFuture && !isResetPending
    }

    /// The reading can confirm an event, such as a quota alert, added reset credits or a balance crossing: it shows a
    /// level and is younger than the maximum reading age.
    public var confirmsEvents: Bool { showsLevel && !isStale }

    /// The same reading after a pass in which every source failed to read: its read failed for `reason`.
    public func failing(_ reason: String) -> ReadingAssessment {
        var failed = self
        failed.status = .readFailed(reason: reason)
        return failed
    }

    /// What an account's header says of its reading: the current account while the reading is normal, else how long ago
    /// it was last read.
    public func accountLabel(now: Date) -> String {
        if isCurrentAccount, status.isNormal { return L10n.text("当前账户", "Current account") }
        let ago = Countdown.formatRough(max(0, now.timeIntervalSince(observedAt ?? now)))
        return L10n.text("上次读取 \(ago) 前", "Last read \(ago) ago")
    }
}

public extension UsageReport {
    /// A reading's status. A window and account answer to the account's issue, else its client home's, else its vendor's; a billing pool and
    /// its windows answer to the pool's account alone, since a vendor with several pools reads each on its own. A balance
    /// answers to its billing entry's issue. A notice about a client's local logs or hooks is no issue.
    func status(of subject: ReadingSubject) -> ReadingStatus {
        switch subject {
        case .window(let agent):
            let account = agent.account.flatMap { observation(accountID: $0.id) }
            if agent.billingPool != nil { return account?.ownStatus ?? .normal }
            return account.map { status(of: .account($0)) } ?? vendorStatus(agent.vendor)
        case .account(let observation):
            let own = observation.ownStatus
            if observation.account.isBillingPool || !own.isNormal { return own }
            let client = vendorStatus(ClientHome.sourceKey(provider: observation.account.provider, home: observation.home))
            return client.isNormal ? vendorStatus(observation.account.provider) : client
        case .balance(let billing):
            return billing.ownStatus
        }
    }

    /// The one judgement every surface asks of a reading at `now`.
    func assess(_ subject: ReadingSubject, now: Date) -> ReadingAssessment {
        let status = status(of: subject)
        switch subject {
        case .window(let agent):
            let snapshot = snapshot(for: agent.id)
            return ReadingAssessment(status: status, isCurrentAccount: isCurrent(agent), observedAt: snapshot?.updatedAt,
                                     resetAt: snapshot?.resetAt, hasValue: snapshot?.remainingPct != nil, now: now)
        case .account(let observation):
            return ReadingAssessment(status: status, isCurrentAccount: observation.isCurrent, observedAt: observation.observedAt, now: now)
        case .balance(let billing):
            return ReadingAssessment(status: status, isCurrentAccount: true, observedAt: billing.updatedAt, now: now)
        }
    }

    /// Only an explicit complete inventory from a current account with a sound reading can retire omitted windows.
    func confirmsCompleteInventory(_ account: AccountObservation) -> Bool {
        account.quotaWindowIDs != nil && account.isCurrent && status(of: .account(account)).isNormal
    }

    /// Complete inventories whose readings can replace earlier windows; health alone never asserts completeness.
    var completeQuotaWindowInventories: [String: Set<String>] {
        Set(accountObservations.map(\.account.id)).reduce(into: [:]) { inventory, id in
            guard let account = observation(accountID: id), confirmsCompleteInventory(account) else { return }
            inventory[id] = account.quotaWindowIDs
        }
    }

    /// A vendor's status: its issue with a quota or balance reading. A report that does not type its issues gives its
    /// notice about such a reading as a failed read, and one that does not tell such notices apart every source notice.
    internal func vendorStatus(_ vendor: String) -> ReadingStatus {
        if let readingIssues { return readingIssues[vendor]?.status ?? .normal }
        return (quotaNotices ?? sourceNotices)[vendor].map { .readFailed(reason: $0) } ?? .normal
    }

    /// The issues a report's providers gave, or for a report that does not type them, its notices about readings as
    /// failed reads.
    internal var typedReadingIssues: [String: ReadingIssue] {
        readingIssues ?? (quotaNotices ?? sourceNotices).mapValues(ReadingIssue.readFailed)
    }
}

extension AccountObservation {
    /// The account's own status; an observation saved without a typed issue gives its notice as a failed read.
    var ownStatus: ReadingStatus { readingIssue?.status ?? quotaNotice.map { .readFailed(reason: $0) } ?? .normal }
}

extension APIBilling {
    /// The balance's own status; a balance saved without a typed issue gives its notice as a failed read.
    var ownStatus: ReadingStatus { readingIssue?.status ?? notice.map { .readFailed(reason: $0) } ?? .normal }
}
