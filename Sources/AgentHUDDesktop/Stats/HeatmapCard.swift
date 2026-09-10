import SwiftUI
import AgentHUDCore

/// 7 × 24 usage heatmap for the last week.
struct HeatmapCard: View {
    let grid: ActivityGrid
    let consumers: [AgentDescriptor]
    let theme: Theme
    @State private var hoveredCell: Cell?

    private struct Cell: Equatable {
        let day: Int
        let hour: Int
    }

    private static let ticks: [Int: String] = [0: "0", 6: "6", 12: "12", 18: "18", 23: "23"]
    private static let cellHeight: CGFloat = 14
    private static let rowSpacing: CGFloat = 4
    private static let columnSpacing: CGFloat = 3
    private static let labelWidth: CGFloat = 28
    private static let labelGap: CGFloat = 10

    var body: some View {
        let days = L10n.weekdayNamesMondayFirst
        let totals = grid.tokens
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(L10n.text("使用时段热图 · 近 7 天", "Activity heatmap · last 7 days")).font(.ui(13, .semibold))
            }
            VStack(alignment: .leading, spacing: Self.rowSpacing) {
                HStack(spacing: Self.labelGap) {
                    Color.clear.frame(width: Self.labelWidth, height: 12)
                    HStack(spacing: Self.columnSpacing) {
                        ForEach(0..<24, id: \.self) { hour in
                            Text(Self.ticks[hour] ?? "")
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
                GeometryReader { proxy in
                    VStack(alignment: .leading, spacing: Self.rowSpacing) {
                        ForEach(Array(grid.rows.prefix(7).enumerated()), id: \.offset) { index, row in
                            HStack(spacing: Self.labelGap) {
                                Text(days[index]).frame(width: Self.labelWidth, alignment: .leading)
                                HStack(spacing: Self.columnSpacing) {
                                    ForEach(Array(row.prefix(24).enumerated()), id: \.offset) { hour, value in
                                        let cell = Cell(day: index, hour: hour)
                                        RoundedRectangle(cornerRadius: 3)
                                            .fill(Self.color(for: value))
                                            .frame(maxWidth: .infinity)
                                            .frame(height: Self.cellHeight)
                                            .overlay {
                                                if hoveredCell == cell {
                                                    RoundedRectangle(cornerRadius: 3).stroke(theme.text.opacity(0.6), lineWidth: 1)
                                                }
                                            }
                                            .onHover { hovering in
                                                if hovering { hoveredCell = cell }
                                                else if hoveredCell == cell { hoveredCell = nil }
                                            }
                                            .accessibilityLabel("\(period(cell)), \(totals[index][hour].formatted()) tokens")
                                    }
                                }
                            }
                        }
                    }
                    .overlay(alignment: .topLeading) {
                        if let cell = hoveredCell {
                            let labelSpace = Self.labelWidth + Self.labelGap
                            let cellWidth = (proxy.size.width - labelSpace - 23 * Self.columnSpacing) / 24
                            let center = labelSpace + CGFloat(cell.hour) * (cellWidth + Self.columnSpacing) + cellWidth / 2
                            let width = min(300, proxy.size.width)
                            HeatmapModelDetails(period: period(cell), tokensByModel: grid.tokensByModel[cell.day][cell.hour],
                                                consumers: consumers, theme: theme)
                                .frame(width: width, alignment: .leading)
                                .fixedSize(horizontal: false, vertical: true)
                                .alignmentGuide(.top) { $0[.bottom] + 8 }
                                .offset(x: max(0, min(center - width / 2, proxy.size.width - width)),
                                        y: CGFloat(cell.day) * (Self.cellHeight + Self.rowSpacing))
                                .allowsHitTesting(false)
                        }
                    }
                }
                .frame(height: 7 * Self.cellHeight + 6 * Self.rowSpacing)
                .zIndex(1)
            }
            .font(.ui(10))
            .foregroundStyle(theme.secondary)
            HStack(spacing: 4) {
                Spacer()
                Text(L10n.text("少", "Less"))
                ForEach([0.0, 0.05, 0.2, 0.5, 1.0], id: \.self) { value in
                    RoundedRectangle(cornerRadius: 2).fill(Self.color(for: value)).frame(width: 12, height: 10)
                }
                Text(L10n.text("多", "More"))
            }
            .font(.ui(10)).foregroundStyle(theme.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
        .help(L10n.text("所有 agent 加总 · 越亮消耗越多", "All agents · brighter = more tokens"))
    }

    private func period(_ cell: Cell) -> String {
        "\(L10n.weekdayNamesMondayFirst[cell.day]) \(String(format: "%02d:00–%02d:00", cell.hour, cell.hour + 1))"
    }

    static func color(for value: Double) -> Color {
        if value == 0 { return Color(.sRGB, red: 128 / 255, green: 128 / 255, blue: 136 / 255, opacity: 0.15) }
        let intensity = sqrt(value)
        return Color(.sRGB, red: 0.08 + 0.62 * intensity, green: 0.28 + 0.69 * intensity, blue: 0.17 + 0.53 * intensity)
    }
}

struct HeatmapModelDetails: View {
    let period: String
    let tokensByModel: [String: Int]
    let consumers: [AgentDescriptor]
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(period).font(.ui(10)).foregroundStyle(theme.secondary)
            Text("\(tokensByModel.values.reduce(0, +).formatted()) tokens").font(.tabular(14, .semibold))
            ForEach(tokensByModel.sorted { a, b in a.value == b.value ? a.key < b.key : a.value > b.value }, id: \.key) { model in
                let index = consumers.firstIndex { $0.id == model.key }
                let consumer = index.map { consumers[$0] }
                HStack(spacing: 5) {
                    Circle().fill(index.map { AgentPalette.swiftUIColor(index: $0) } ?? theme.secondary).frame(width: 6, height: 6)
                    if let consumer { AgentLogo(vendor: consumer.vendor, size: 12) }
                    Text(consumer.map { L10n.modelLabel($0.model) } ?? model.key)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 6)
                    Text(model.value.formatted()).font(.tabular(11)).fixedSize()
                }
                .font(.ui(11))
            }
            Text(L10n.text("所有 agent · 近 7 天", "All agents · last 7 days"))
                .font(.ui(10)).foregroundStyle(theme.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .foregroundStyle(theme.text)
        .background(RoundedRectangle(cornerRadius: 8).fill(theme.windowBackground))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(theme.cardBorder))
        .shadow(color: .black.opacity(0.2), radius: 6, y: 2)
    }

}
