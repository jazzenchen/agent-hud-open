import SwiftUI
import AgentHUDCore

/// One compact metric tile per agent, with each plan's own window selection.
struct MetricCards: View {
    let store: UsageStore
    let theme: Theme

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 12, alignment: .top)], spacing: 12) {
            ForEach(store.rowGroups, id: \.vendor) { group in
                AgentQuotaTile(store: store, vendor: group.vendor, rows: group.rows, theme: theme)
                    .id(Self.anchor(group.vendor))
            }
            ForEach(store.report?.billing ?? []) { billing in
                APIBillingSummary(billing: billing, store: store, theme: theme)
            }
        }
    }
}

extension MetricCards {
    static func anchor(_ vendor: String) -> String { "quota-" + vendor }
}

private struct AgentQuotaTile: View {
    let store: UsageStore
    let vendor: String
    let rows: [AgentRow]
    let theme: Theme
    @State private var selectedRowID: String?

    private var row: AgentRow? {
        rows.first { $0.id == selectedRowID } ?? rows.first { $0.remainingPct != nil } ?? rows.first
    }

    var body: some View {
        let insights = row.flatMap { store.report?.insightsByAgent[$0.id] }
        let forecast = row.flatMap { store.quotaForecastHint(for: $0.id) }
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                AgentLogo(vendor: rows.first?.agent.vendor ?? vendor, size: 14)
                Text(vendor).font(.ui(12, .semibold)).lineLimit(1)
                Spacer(minLength: 4)
                SelectionMenu(
                    title: vendor + " " + L10n.text("额度窗口", "quota window"),
                    options: rows.map { row in
                        let account = Set(rows.compactMap(\.agent.account?.id)).count > 1 ? row.account.map { $0.displayName + " · " } : nil
                        return SegmentOption(value: row.id, label: (account ?? "") + L10n.modelLabel(row.agent.model))
                    },
                    selection: Binding(get: { row?.id ?? "" }, set: { selectedRowID = $0 }),
                    theme: theme
                )
            }
            .frame(height: 22)
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 5) {
                    Text(L10n.text("近期速率", "Burn rate"))
                        .font(.ui(10)).foregroundStyle(theme.secondary)
                    if let rate = insights?.burnRatePctPerHour {
                        (Text(String(format: "%.1f", rate)).font(.tabular(18, .semibold))
                         + Text(" % / h").font(.ui(10)).foregroundColor(theme.secondary))
                    } else {
                        Text("—").font(.tabular(18, .semibold))
                    }
                    Text(forecast ?? L10n.text("记录不足", "Insufficient data"))
                        .font(.ui(10)).foregroundStyle(theme.secondary)
                        .lineLimit(1).help(forecast ?? "")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Rectangle().fill(theme.divider).frame(width: 1)
                VStack(alignment: .leading, spacing: 5) {
                    Text(L10n.text("本周触顶", "Weekly caps"))
                        .font(.ui(10)).foregroundStyle(theme.secondary)
                    Text(insights.map { String($0.weeklyCapHits) } ?? "—")
                        .font(.tabular(18, .semibold))
                    Text(insights.map { $0.weeklyCapHits > 0
                         ? L10n.text("等待 ", "Waited ") + Countdown.format($0.weeklyWaitTotal)
                         : L10n.text("本周尚未触顶", "No cap hit this week") }
                         ?? L10n.text("记录不足", "Insufficient data"))
                        .font(.ui(10)).foregroundStyle(theme.secondary).lineLimit(1)
                        .help(insights.map(capHitsDescription) ?? "")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, minHeight: 86, alignment: .topLeading)
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(theme.status(.ok), lineWidth: 2).opacity(isPointedOut ? 1 : 0))
        .onChange(of: store.selectedQuotaId, initial: true) { _, id in
            if let id, rows.contains(where: { $0.id == id }) { selectedRowID = id }
        }
    }

    /// Whatever opened the window asked for one of this tile's windows.
    private var isPointedOut: Bool { store.selectedQuotaId.map { id in rows.contains { $0.id == id } } ?? false }

    private func capHitsDescription(_ insights: UsageInsights) -> String {
        guard insights.weeklyCapHits > 0 else { return L10n.text("本周尚未触顶", "No cap hit this week") }
        let total = Countdown.format(insights.weeklyWaitTotal)
        let longest = Countdown.format(insights.weeklyWaitLongest)
        var text = L10n.text("累计等待 \(total) · 最长一次 \(longest)", "Waited \(total) in total · longest \(longest)")
        if let at = insights.weeklyWaitLongestAt { text += "（\(ChartData.weekdayTime(at))）" }
        return text
    }
}
