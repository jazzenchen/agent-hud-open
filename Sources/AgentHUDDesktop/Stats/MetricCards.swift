import SwiftUI
import AgentHUDCore

/// One compact metric tile per agent, with each subscription's own window selection.
struct MetricCards: View {
    let store: UsageStore
    let theme: Theme

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 12, alignment: .top)], spacing: 12) {
            ForEach(store.rowGroups, id: \.vendor) { group in
                AgentQuotaTile(store: store, vendor: group.vendor, rows: group.rows, theme: theme)
            }
            ForEach(store.report?.billing ?? []) { billing in
                APIBillingSummary(billing: billing, store: store, theme: theme)
            }
        }
    }
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
                    options: rows.map { SegmentOption(value: $0.id, label: L10n.modelLabel($0.agent.model)) },
                    selection: Binding(get: { row?.id ?? "" }, set: { selectedRowID = $0 }),
                    theme: theme
                )
            }
            .frame(height: 22)
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 5) {
                    Text(L10n.text("本周期速率", "Burn rate"))
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
    }

    private func capHitsDescription(_ insights: UsageInsights) -> String {
        guard insights.weeklyCapHits > 0 else { return L10n.text("本周尚未触顶", "No cap hit this week") }
        let total = Countdown.format(insights.weeklyWaitTotal)
        let longest = Countdown.format(insights.weeklyWaitLongest)
        var text = L10n.text("累计等待 \(total) · 最长一次 \(longest)", "Waited \(total) in total · longest \(longest)")
        if let at = insights.weeklyWaitLongestAt { text += "（\(ChartData.weekdayTime(at))）" }
        return text
    }
}

struct WeeklyTokenShareCard: View {
    let store: UsageStore
    let theme: Theme

    var body: some View {
        let shares = store.weeklyTokenShare
        let consumers = store.consumers
        let totalShare = consumers.reduce(0.0) { $0 + (shares[$1.id] ?? 0) }
        let entries = consumers.compactMap { consumer -> (agent: AgentDescriptor, color: Color, share: Double)? in
            guard totalShare > 0, let share = shares[consumer.id] else { return nil }
            return (consumer, AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(consumer.id)), share / totalShare)
        }
        return VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(L10n.text("本周消耗占比", "Share of this week's tokens")).font(.ui(13, .semibold))
                Spacer()
                Text(store.tokenDimensions.label).font(.ui(11)).foregroundStyle(theme.secondary)
            }
            GeometryReader { proxy in
                HStack(spacing: 0) {
                    ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                        Rectangle().fill(entry.color).frame(width: proxy.size.width * entry.share)
                    }
                }
            }
            .frame(height: 6)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .padding(.top, 2)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), alignment: .leading)], alignment: .leading, spacing: 6) {
                ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                    HStack(spacing: 5) {
                        Circle().fill(entry.color).frame(width: 6, height: 6)
                        AgentLogo(vendor: entry.agent.vendor, size: 12)
                        Text(L10n.modelLabel(entry.agent.model)).lineLimit(1)
                        Spacer(minLength: 4)
                        Text("\(Int((entry.share * 100).rounded()))%")
                    }
                    .font(.ui(11)).foregroundStyle(theme.secondary)
                    .help(entry.agent.displayName)
                    .padding(.trailing, 12)
                }
            }
            .padding(.top, 2)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }
}
