import SwiftUI
import AgentHUDCore

/// API-funded accounts show money directly; there is no percentage quota or reset period.
struct APIBillingCard: View {
    let billing: APIBilling
    let store: UsageStore
    let theme: Theme
    var compact = false

    var body: some View {
        VStack(alignment: .leading, spacing: compact ? 8 : 12) {
            HStack(spacing: compact ? 6 : 7) {
                AgentLogo(vendor: billing.vendor, size: 14)
                    .frame(width: compact ? IslandRowLayout.inset * 2 + IslandRowLayout.markerWidth : 14)
                Text(billing.displayName)
                    .font(compact ? IslandRowLayout.headingFont : .ui(12, .semibold))
                    .foregroundStyle(theme.text)
                Spacer()
            }
            .padding(.vertical, compact ? IslandRowLayout.headingVerticalPadding : 0)
            if billing.balances.isEmpty {
                balanceLine(currency: billing.currency, balance: nil)
            } else {
                ForEach(billing.balances) { balance in
                    balanceLine(currency: balance.currency, balance: balance)
                    if !compact {
                        Text(L10n.text("充值余额 ", "Topped up ") + MoneyFormat.amount(balance.toppedUp, currency: balance.currency)
                             + L10n.text(" · 赠送余额 ", " · Granted ") + MoneyFormat.amount(balance.granted, currency: balance.currency))
                            .font(.ui(11)).foregroundStyle(theme.secondary)
                    }
                }
            }
            if !compact {
                HStack(alignment: .top) {
                    Text(L10n.text("费用仅估算本机 Harness 已记录的请求，包含缓存和峰谷价格。", "Costs estimate requests recorded by local Harness, including cache and peak pricing."))
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 8)
                    Link(L10n.text("计价说明", "Pricing"), destination: DeepSeekPricing.sourceURL)
                }
                .font(.ui(11)).foregroundStyle(theme.secondary)
                .help(L10n.text("官方价格核对日期：", "Official prices checked: ") + DeepSeekPricing.checkedOn)
            }
        }
    }

    private func balanceLine(currency: String, balance: AccountBalance?) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: compact ? 24 : nil) {
            VStack(alignment: .leading, spacing: 3) {
                Text(L10n.text("账户余额", "Account balance")).font(.ui(11)).foregroundStyle(theme.secondary)
                Text(balance.map { MoneyFormat.amount($0.total, currency: currency) } ?? "—")
                    .font(.tabular(compact ? 15 : 22, .semibold))
                    .foregroundStyle(balance.flatMap { store.balanceLevel($0, billing: billing) }.map { theme.statusText($0) } ?? theme.text)
                    .help(billing.isAvailable == false ? L10n.text("余额不足，API 暂不可用", "Insufficient balance; API unavailable") : L10n.text("来自 DeepSeek 官方余额接口", "From the official DeepSeek balance API"))
            }
            .frame(maxWidth: compact ? .infinity : nil, alignment: .leading)
            if !compact { Spacer() }
            VStack(alignment: compact ? .leading : .trailing, spacing: 3) {
                Text(L10n.text("费用估算 · ", "Est. cost · ") + store.statsRange.recentLabel)
                    .font(.ui(11)).foregroundStyle(theme.secondary)
                Text(billing.estimatedCost(currency: currency, during: store.statsInterval)
                    .map { MoneyFormat.amount($0, currency: currency, estimated: true) } ?? L10n.text("暂无价格", "Unpriced"))
                    .font(.tabular(compact ? 15 : 20, .semibold))
                    .foregroundStyle(theme.text)
            }
            .frame(maxWidth: compact ? .infinity : nil, alignment: compact ? .leading : .trailing)
        }
        .padding(.leading, compact ? IslandRowLayout.textInset : 0)
        .padding(.trailing, compact ? IslandRowLayout.inset : 0)
    }
}
