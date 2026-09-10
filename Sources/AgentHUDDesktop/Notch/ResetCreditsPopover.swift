import SwiftUI
import AgentHUDCore

struct ResetCreditsDetails: View {
    let resets: CodexResetCredits
    private let theme = Theme.island

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.text("额度重置", "Usage resets")).font(.ui(12, .semibold))
                Spacer()
                Text(L10n.text("\(resets.availableCount) 次可用", "\(resets.availableCount) available"))
                    .font(.tabular(11, .semibold)).foregroundStyle(theme.status(.ok))
            }
            Text(L10n.text("到期时间 · 本地时区", "Expires at · local time"))
                .font(.ui(10)).foregroundStyle(theme.secondary)
            VStack(spacing: 8) {
                ForEach(Array(resets.creditsByExpiry.enumerated()), id: \.element.id) { index, credit in
                    HStack {
                        Text(L10n.text("重置 \(index + 1)", "Reset \(index + 1)"))
                            .foregroundStyle(theme.secondary)
                        Spacer()
                        Text(Self.expiryLabel(credit)).font(.tabular(12))
                    }
                    .accessibilityElement(children: .combine)
                }
            }
            let missing = resets.availableCount - (resets.credits?.count ?? 0)
            if missing > 0 {
                Text(Self.missingLabel(missing)).font(.ui(11)).foregroundStyle(theme.secondary)
            }
        }
        .font(.ui(11))
        .foregroundStyle(theme.text)
        .padding(12)
        .frame(width: 310)
        .background(RoundedRectangle(cornerRadius: 10).fill(theme.windowBackground))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(theme.cardBorder))
        .fixedSize(horizontal: false, vertical: true)
    }

    static func expiryLabel(_ credit: CodexResetCredits.Credit) -> String {
        guard let date = credit.expirationDate else { return L10n.text("有效期未提供", "Expiry unavailable") }
        return date.formatted(Date.FormatStyle()
            .year().month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits)
            .locale(Locale(identifier: L10n.resolved == .zhHans ? "zh_CN" : "en_GB")))
    }

    static func missingLabel(_ count: Int) -> String {
        L10n.text("\(count) 次重置的有效期未提供", "Expiry unavailable for \(count) resets")
    }

    static func accessibilityText(_ resets: CodexResetCredits) -> String {
        var lines = [L10n.text("\(resets.availableCount) 次可用", "\(resets.availableCount) available")]
        lines += resets.creditsByExpiry.enumerated().map { index, credit in
            L10n.text("重置 \(index + 1) · ", "Reset \(index + 1) · ") + expiryLabel(credit)
        }
        let missing = resets.availableCount - (resets.credits?.count ?? 0)
        if missing > 0 { lines.append(missingLabel(missing)) }
        return lines.joined(separator: "\n")
    }
}
