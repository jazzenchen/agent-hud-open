import SwiftUI
import AgentHUDCore

/// Wallet money belongs to the account beside its quota windows. Its amounts never imply a subscription percentage.
struct AccountWalletRow: View {
    let wallet: AccountWallet
    private let theme = Theme.island

    var body: some View {
        let labels = AccountWalletLabels(wallet: wallet)
        HStack(spacing: IslandRowLayout.spacing) {
            Image(systemName: wallet.kind == .prepaid ? "creditcard" : "dollarsign.circle")
                .font(.ui(10))
                .foregroundStyle(theme.tertiary)
                .frame(width: IslandRowLayout.markerWidth)
            Text(labels.name)
                .font(.ui(12, .medium))
                .foregroundStyle(theme.secondary)
                .frame(width: IslandRowLayout.nameWidth, alignment: .leading)
            Spacer(minLength: 8)
            Text(labels.amounts)
                .font(.tabular(12, .semibold))
                .foregroundStyle(theme.text)
        }
        .lineLimit(1)
        .padding(.horizontal, IslandRowLayout.inset)
        .padding(.vertical, 2)
        .help(wallet.observedAt.map { at in
            let date = at.formatted(Date.FormatStyle().month(.abbreviated).day()
                .hour().minute().second().locale(L10n.dateLocale))
            return L10n.text("读取时间：\(date)", "Read at: \(date)")
        } ?? "")
        .accessibilityElement(children: .combine)
    }
}

/// Keep a reported zero distinct from a field the source omitted.
struct AccountWalletLabels {
    let wallet: AccountWallet

    var name: String {
        switch wallet.kind {
        case .prepaid: L10n.text("预付余额", "Prepaid balance")
        case .onDemand: L10n.text("按需付费", "On-demand")
        }
    }

    var amounts: String {
        switch wallet.kind {
        case .prepaid: amount(wallet.balance)
        case .onDemand:
            L10n.text("已用 \(amount(wallet.used)) · 上限 \(amount(wallet.limit))",
                      "Used \(amount(wallet.used)) · Limit \(amount(wallet.limit))")
        }
    }

    private func amount(_ value: Decimal?) -> String {
        value.map { MoneyFormat.amount($0, currency: wallet.currency) } ?? "N/A"
    }
}
