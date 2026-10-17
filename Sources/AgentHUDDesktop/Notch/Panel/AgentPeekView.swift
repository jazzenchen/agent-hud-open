import SwiftUI
import AgentHUDCore

/// What a dock that separates its agents says about one of them before anything is opened: its quota windows
/// and how many of its sessions are running.
struct AgentPeek: Equatable {
    struct Line: Equatable, Identifiable {
        let label: String
        let value: String
        let level: StatusLevel?
        var id: String { label }
    }

    /// How the peek is drawn: a small card floating beside the mark, or one line growing out of the Simple panel.
    enum Style: Equatable { case bubble, line }

    let vendor: String
    let lines: [Line]
    let running: Int
    var style: Style = .bubble
    /// Room the line keeps on the side that sits behind the Simple panel it grows from.
    var tuck: CGFloat = 0
    var edge: HUDEdge = .top
    /// The scale of the dock this peek belongs to (`IslandRootView.dockScale`). A peek is the dock answering a
    /// hover, so it grows with the dock's own marks; it is never smaller than at the default.
    var scale: CGFloat = 1

    /// A length of the bubble at the dock's scale.
    func scaled(_ value: CGFloat) -> CGFloat { (value * max(1, scale)).rounded() }

    static let bubbleWidth: CGFloat = 196
    private static let rowHeight: CGFloat = 18
    private static var labelFont: NSFont { .systemFont(ofSize: 12) }
    private static var valueFont: NSFont { .monospacedDigitSystemFont(ofSize: 12, weight: .semibold) }
    /// The line's lengths, all from the one scale: its text, which is every word a dock writes
    /// (`IslandRootView.dockText`), the gap inside a pair and between pairs, its end padding and its height, which is a
    /// compact reminder's.
    var lineFont: CGFloat { IslandRootView.dockText(scale: scale) }
    var linePairGap: CGFloat { (5 * max(1, scale)).rounded() }
    var lineSpacing: CGFloat { (12 * max(1, scale)).rounded() }
    var linePadding: CGFloat { (12 * max(1, scale)).rounded() }
    var lineHeight: CGFloat { (IslandRootView.dockCompactHeight * max(1, scale)).rounded() }

    /// The agent's windows in row order, each with the share already used, then its balance if it is billed by use.
    @MainActor
    static func make(vendor: String, store: UsageStore) -> AgentPeek {
        let key = AgentArtwork.markKey(vendor)
        let rows = store.rowGroups.filter { AgentArtwork.markKey($0.vendor) == key }.flatMap(\.rows)
        var seen = Set<String>()
        var lines = rows.compactMap { row -> Line? in
            guard row.isCurrentAccount, seen.insert(row.agent.shortName).inserted else { return nil }
            return Line(label: row.agent.shortName,
                        value: row.usedPct.map { "\(Int($0.rounded()))%" } ?? row.missingQuotaLabel,
                        level: row.level)
        }
        for billing in store.enabledBilling where AgentArtwork.markKey(billing.vendor) == key {
            lines += billing.balances.prefix(1).map {
                Line(label: L10n.text("余额", "Balance"), value: MoneyFormat.amount($0.total, currency: $0.currency),
                     level: store.view.level(of: billing))
            }
        }
        return AgentPeek(vendor: vendor, lines: lines, running: HoverPanelView.sessionRows(store, vendor: vendor).running.count)
    }

    private var runningText: String? {
        running > 0 ? L10n.text("\(running) 个运行中", "\(running) running") : nil
    }

    /// What the one line says when the agent has no window to report.
    private var fallback: String { runningText ?? vendor }

    /// The peek's size is worked out from its text, so the card is placed before anything is drawn.
    var size: CGSize {
        switch style {
        case .bubble:
            let rows = 1 + lines.count + (runningText == nil ? 0 : 1)
            return CGSize(width: scaled(Self.bubbleWidth), height: 2 * scaled(10) + scaled(Self.rowHeight) * CGFloat(rows))
        case .line:
            func width(_ text: String, _ font: NSFont) -> CGFloat { ceil((text as NSString).size(withAttributes: [.font: font]).width) }
            let label = NSFont.systemFont(ofSize: lineFont)
            let value = NSFont.monospacedDigitSystemFont(ofSize: lineFont, weight: .semibold)
            let cells = lines.isEmpty ? width(fallback, label)
                : lines.reduce(0) { $0 + width($1.label, label) + linePairGap + width($1.value, value) }
                    + lineSpacing * CGFloat(lines.count - 1)
            return CGSize(width: cells + linePadding * 2 + tuck, height: lineHeight)
        }
    }
}

struct AgentPeekView: View {
    let peek: AgentPeek
    @Environment(\.hudTheme) private var theme

    var body: some View {
        switch peek.style {
        case .bubble: bubble
        case .line: line
        }
    }

    private func tint(_ line: AgentPeek.Line) -> Color {
        line.level.map { theme.statusText($0) } ?? theme.secondary
    }

    private var bubble: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(peek.vendor).font(.ui(peek.scaled(13), .semibold)).frame(height: peek.scaled(18))
            ForEach(peek.lines) { line in
                HStack {
                    Text(line.label).foregroundStyle(theme.secondary)
                    Spacer(minLength: 8)
                    Text(line.value).font(.tabular(peek.scaled(12), .semibold)).foregroundStyle(tint(line))
                }.frame(height: peek.scaled(18))
            }
            if peek.running > 0 {
                Text(L10n.text("\(peek.running) 个运行中", "\(peek.running) running"))
                    .foregroundStyle(theme.secondary).frame(height: peek.scaled(18))
            }
        }
        .font(.ui(peek.scaled(12))).lineLimit(1)
        .foregroundStyle(theme.text)
        .padding(.horizontal, peek.scaled(12)).padding(.vertical, peek.scaled(10))
        .frame(width: peek.size.width, height: peek.size.height, alignment: .topLeading)
    }

    private var line: some View {
        HStack(spacing: peek.lineSpacing) {
            if peek.lines.isEmpty {
                Text(peek.running > 0 ? L10n.text("\(peek.running) 个运行中", "\(peek.running) running") : peek.vendor)
                    .foregroundStyle(theme.secondary)
            }
            ForEach(peek.lines) { line in
                HStack(spacing: peek.linePairGap) {
                    Text(line.label).foregroundStyle(theme.secondary)
                    Text(line.value).font(.tabular(peek.lineFont, .semibold)).foregroundStyle(tint(line))
                }
            }
        }
        .font(.ui(peek.lineFont)).lineLimit(1).fixedSize()
        .padding(.horizontal, peek.linePadding)
        // The side tucked behind the panel carries no text.
        .padding(tuckEdge, peek.tuck)
        .frame(width: peek.size.width, height: peek.size.height)
    }

    private var tuckEdge: Edge.Set {
        switch peek.edge {
        case .left: return .leading
        case .right: return .trailing
        case .top: return .top
        case .bottom: return .bottom
        }
    }
}
