import SwiftUI
import AgentHUDCore

struct UsageChartsCard: View {
    let store: UsageStore
    let theme: Theme

    var body: some View {
        TokenConsumptionChart(store: store, theme: theme, context: .stats)
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
    }
}

/// The same stacked token chart in statistics and the expanded island.
struct TokenConsumptionChart: View {
    enum Context { case stats, island }

    let store: UsageStore
    let theme: Theme
    let context: Context

    var body: some View {
        let columns = store.tokenColumns
        let legendConsumers = store.consumers.enumerated().filter { index, _ in
            columns.contains { $0.tokens[index] > 0 }
        }.map(\.element)
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(L10n.text("Token 消耗", "Tokens"))
                    .font(.ui(13, .semibold))
                Spacer()
                if context == .island {
                    Text(store.report == nil ? "\(store.tokenBucketSize.label) · —"
                         : "\(store.tokenBucketSize.label) · \(TokenFormat.short(columns.reduce(0) { $0 + $1.total })) tok")
                        .font(.tabular(12))
                        .foregroundStyle(theme.secondary)
                } else {
                    Text(TokenFormat.short(columns.reduce(0) { $0 + $1.total }))
                        .font(.tabular(20, .semibold))
                        .help(L10n.text("所选时间与维度的 Token 总量", "Total tokens for the selected range and dimensions"))
                }
            }
            if !legendConsumers.isEmpty {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 145), alignment: .leading)], alignment: .leading, spacing: 6) {
                    ForEach(legendConsumers) { consumer in
                        legendLabel(consumer)
                    }
                }
                .font(.ui(11))
                .foregroundStyle(theme.secondary)
            }
            TokenBarsChart(columns: columns, interval: store.statsInterval, colors: barColors,
                           consumers: store.consumers, theme: theme, isLoading: store.report == nil)
                .id([store.statsRange.hours, store.tokenBucketSize.rawValue, store.tokenDimensions.rawValue])
                .frame(height: context == .island ? 72 : 100)
                .padding(.horizontal, context == .stats ? 12 : 6)
                .padding(.top, 12)
                .zIndex(1)

            HStack {
                let labels = ChartData.axisLabels(range: store.statsRange, now: store.dataDate)
                ForEach(Array(labels.enumerated()), id: \.offset) { index, label in
                    if index > 0 { Spacer() }
                    Text(label)
                }
            }
            .font(.ui(10))
            .foregroundStyle(theme.secondary)
            .padding(.horizontal, context == .stats ? 12 : 6)
        }
    }

    private func legendLabel(_ consumer: AgentDescriptor) -> some View {
        HStack(spacing: 5) {
            Circle().fill(AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(consumer.id))).frame(width: 7, height: 7)
            AgentLogo(vendor: consumer.vendor, size: 12)
            Text(L10n.modelLabel(consumer.model))
                .lineLimit(1)
        }
        .accessibilityLabel(consumer.displayName)
        .help(consumer.displayName)
    }

    private var barColors: [Color] {
        store.consumers.map { AgentPalette.swiftUIColor(index: store.consumerPaletteIndex($0.id)) }
    }
}

/// Each column is a single stack whose height is the sum of its model segments.
struct TokenBarsChart: View {
    let columns: [TokenColumn]
    let interval: DateInterval
    let colors: [Color]
    let consumers: [AgentDescriptor]
    let theme: Theme
    var isLoading = false
    @State private var hoveredID: Date?

    private var inspectedColumn: TokenColumn? {
        hoveredID.flatMap { id in columns.first { $0.id == id } }
    }

    /// Render complete buckets at both ends; their counts still cover only the selected range.
    private var plotInterval: DateInterval {
        guard let first = columns.first, let last = columns.last else { return interval }
        return DateInterval(start: first.interval.start, end: last.interval.end)
    }

    var body: some View {
        let peak = columns.map(\.total).max() ?? 0
        let scale = max(1, peak)
        let inspectedColumn = inspectedColumn
        GeometryReader { proxy in
            let usable = proxy.size.height - 2
            ZStack(alignment: .bottomLeading) {
                Rectangle().fill(theme.divider).frame(height: 1)
                if let column = inspectedColumn {
                    let bounds = xBounds(column, width: proxy.size.width)
                    Rectangle().fill(theme.text.opacity(0.08))
                        .frame(width: bounds.upperBound - bounds.lowerBound, height: usable)
                        .offset(x: bounds.lowerBound)
                    Rectangle().fill(theme.secondary.opacity(0.5))
                        .frame(width: 1, height: usable)
                        .offset(x: (bounds.lowerBound + bounds.upperBound) / 2)
                }
                // Draw dense ranges in one surface instead of laying out a view for every segment.
                Canvas { context, size in
                    for column in columns where column.total > 0 {
                        let bounds = xBounds(column, width: size.width)
                        let gap = min(2, (bounds.upperBound - bounds.lowerBound) * 0.2)
                        let x = bounds.lowerBound + gap / 2
                        let width = max(0, bounds.upperBound - bounds.lowerBound - gap)
                        let height = CGFloat(column.total) / CGFloat(scale) * usable
                        let stack = CGRect(x: x, y: size.height - 1 - height, width: width, height: height)
                        var stackContext = context
                        stackContext.clip(to: Path(roundedRect: stack, cornerRadius: 2))
                        stackContext.opacity = inspectedColumn == nil || inspectedColumn?.id == column.id ? 1 : 0.55
                        var bottom = size.height - 1
                        for (index, value) in column.tokens.enumerated() where value > 0 {
                            let segmentHeight = CGFloat(value) / CGFloat(scale) * usable
                            bottom -= segmentHeight
                            let segment = CGRect(x: x, y: bottom, width: width, height: segmentHeight)
                            let color = colors.indices.contains(index) ? colors[index] : theme.secondary
                            stackContext.fill(Path(segment), with: .color(color))
                        }
                    }
                }
                if peak > 0 || !isLoading {
                    Text(TokenFormat.short(peak))
                        .font(.ui(10))
                        .foregroundStyle(theme.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                        .offset(y: -12)
                }
                if peak == 0 && !isLoading {
                    Text(L10n.text("此时间窗口内没有 Token 消耗", "No token usage in this range"))
                        .font(.ui(11))
                        .foregroundStyle(theme.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .contentShape(Rectangle())
            .onContinuousHover { phase in
                switch phase {
                case .active(let point): hoveredID = isLoading && peak == 0 ? nil : column(at: point.x, width: proxy.size.width)?.id
                case .ended: hoveredID = nil
                }
            }
            .overlay(alignment: .bottomLeading) {
                if let column = inspectedColumn {
                    let width = min(280, proxy.size.width)
                    let bounds = xBounds(column, width: proxy.size.width)
                    let midpoint = (bounds.lowerBound + bounds.upperBound) / 2
                    let preferred = midpoint < proxy.size.width / 2 ? midpoint + 12 : midpoint - width - 12
                    detail(column)
                        .frame(width: width)
                        .fixedSize(horizontal: false, vertical: true)
                        .offset(x: max(0, min(preferred, proxy.size.width - width)), y: -6)
                        .allowsHitTesting(false)
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(L10n.text("Token 消耗图表", "Token consumption chart"))
        .accessibilityValue(inspectedColumn.map(description) ?? L10n.text("悬停查看时间段", "Hover to inspect a period"))
    }

    private func xBounds(_ column: TokenColumn, width: CGFloat) -> ClosedRange<CGFloat> {
        let left = column.interval.start.timeIntervalSince(plotInterval.start) / plotInterval.duration * width
        let right = column.interval.end.timeIntervalSince(plotInterval.start) / plotInterval.duration * width
        return left...right
    }

    private func column(at x: CGFloat, width: CGFloat) -> TokenColumn? {
        guard width > 0 else { return nil }
        let date = plotInterval.start.addingTimeInterval(max(0, min(1, x / width)) * plotInterval.duration)
        return ChartData.tokenColumn(at: date, in: columns)
    }

    private func detail(_ column: TokenColumn) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(period(column)).font(.ui(10)).foregroundStyle(theme.secondary)
            Text("\(column.total.formatted()) tokens").font(.tabular(14, .semibold))
            ForEach(Array(consumers.enumerated()), id: \.element.id) { index, consumer in
                if column.tokens.indices.contains(index), column.tokens[index] > 0 {
                    HStack(spacing: 5) {
                        Circle().fill(colors[index]).frame(width: 6, height: 6)
                        AgentLogo(vendor: consumer.vendor, size: 12)
                        Text(L10n.modelLabel(consumer.model)).lineLimit(1)
                        Spacer(minLength: 6)
                        Text(column.tokens[index].formatted()).font(.tabular(11))
                    }
                    .font(.ui(11))
                }
            }
            if column.total == 0 {
                Text(L10n.text("此时段没有 Token 消耗", "No token usage in this period"))
                    .font(.ui(11)).foregroundStyle(theme.secondary)
            }
            if column.interval.start < interval.start || column.interval.end > interval.end {
                Text(L10n.text("仅统计所选范围内的部分时段", "Partial period within the selected range"))
                    .font(.ui(10)).foregroundStyle(theme.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .foregroundStyle(theme.text)
        .background(RoundedRectangle(cornerRadius: 8).fill(theme.windowBackground))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(theme.cardBorder))
        .shadow(color: .black.opacity(0.2), radius: 6, y: 2)
    }

    private func period(_ column: TokenColumn) -> String {
        "\(ChartData.weekdayTime(max(column.interval.start, interval.start))) – \(ChartData.weekdayTime(min(column.interval.end, interval.end)))"
    }

    private func description(_ column: TokenColumn) -> String {
        let total = L10n.text("合计 \(column.total.formatted()) tokens", "Total \(column.total.formatted()) tokens")
        let details = zip(consumers, column.tokens).filter { $0.1 > 0 }.map { "\($0.0.displayName): \($0.1.formatted())" }
        return ([period(column), total] + details).joined(separator: "\n")
    }
}
