import SwiftUI
import AgentHUDCore

/// Today, the last seven days and the last thirty side by side: the selected kinds' tokens and what they would cost at API
/// list prices, how much of the prompts came from the cache, and the models that spent the most.
struct UsagePeriodsCard: View {
    let store: UsageStore
    let theme: Theme
    /// Models named in each period; the rest are counted together.
    private static let listed = 3

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.text("近期用量", "Recent usage")).font(.ui(13, .semibold))
                Spacer()
                Text(store.tokenDimensions.label).font(.ui(11)).foregroundStyle(theme.secondary)
            }
            HStack(alignment: .top, spacing: 0) {
                ForEach(Array(UsagePeriods.Period.allCases.enumerated()), id: \.element) { index, period in
                    if index > 0 { Rectangle().fill(theme.divider).frame(width: 1).padding(.horizontal, 14) }
                    column(period).frame(maxWidth: .infinity, alignment: .topLeading)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }

    private func column(_ period: UsagePeriods.Period) -> some View {
        let tokens = store.periodTokens(period), dimensions = store.tokenDimensions
        let counts = tokens.mapValues(dimensions.count), total = counts.values.reduce(0, +)
        let ranked = store.consumers.filter { (counts[$0.id] ?? 0) > 0 }.sorted { counts[$0.id]! > counts[$1.id]! }
        let cost = ModelCatalog.cost(of: tokens.mapValues(dimensions.masking))
        let hits = tokens.values.reduce(TokenKinds(), +).cacheHitRate
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(period.label).font(.ui(12, .semibold))
                Spacer(minLength: 6)
                Text(total > 0 ? TokenFormat.short(total) : "—").font(.tabular(15, .semibold))
            }
            HStack(spacing: 4) {
                if let cost {
                    Text("≈" + MoneyFormat.amount(cost.amount, currency: "USD"))
                        .help(L10n.text("所选种类按厂商 API 公开价折合", "The selected kinds at the vendors' API list prices")
                              + (cost.unpriced.isEmpty ? "" : "\n" + L10n.text("没有公开价、未计入：", "Not counted, no list price: ")
                                 + cost.unpriced.map(store.consumerName).joined(separator: L10n.text("、", ", "))))
                }
                if cost != nil, hits != nil { Text("·") }
                if let hits {
                    Text(L10n.text("缓存命中 ", "Cache hits ") + TokenFormat.percent(hits * 100))
                        .help(L10n.text("缓存读取占全部提示的比例", "Cache reads as a share of every prompt"))
                }
            }
            .font(.tabular(11))
            .foregroundStyle(theme.secondary)
            .lineLimit(1)
            if total > 0 {
                shareBar(ranked, counts: counts, total: total)
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(ranked.prefix(Self.listed)) { consumer in
                        HStack(spacing: 5) {
                            Circle().fill(color(consumer.id)).frame(width: 6, height: 6)
                            AgentLogo(vendor: consumer.vendor, size: 11)
                            Text(L10n.modelLabel(consumer.model)).lineLimit(1)
                            Spacer(minLength: 4)
                            Text(TokenFormat.short(counts[consumer.id]!)).font(.tabular(11))
                        }
                        .help(consumer.displayName)
                    }
                    if ranked.count > Self.listed {
                        let rest = ranked.dropFirst(Self.listed)
                        HStack {
                            Text(L10n.text("其余 \(rest.count) 个", "\(rest.count) more"))
                            Spacer(minLength: 4)
                            Text(TokenFormat.short(rest.reduce(0) { $0 + counts[$1.id]! })).font(.tabular(11))
                        }
                        .padding(.leading, 11)
                    }
                }
                .font(.ui(11))
                .foregroundStyle(theme.secondary)
            }
        }
    }

    private func shareBar(_ ranked: [AgentDescriptor], counts: [String: Int], total: Int) -> some View {
        GeometryReader { proxy in
            HStack(spacing: 0) {
                ForEach(ranked) { consumer in
                    Rectangle().fill(color(consumer.id)).frame(width: proxy.size.width * CGFloat(counts[consumer.id]!) / CGFloat(total))
                }
            }
        }
        .frame(height: 5)
        .clipShape(RoundedRectangle(cornerRadius: 3))
    }

    private func color(_ id: String) -> Color { AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(id)) }
}
