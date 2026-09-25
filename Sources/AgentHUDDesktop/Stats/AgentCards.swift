import SwiftUI
import AgentHUDCore

/// A card per agent for the charted range: its share of the tokens and how they ran over the range, each kind, what they
/// would cost at API list prices and where that went, then its models and sessions. The agents this Mac has are picked in
/// a menu, until then the ones Settings shows, and appear in Settings' order.
struct AgentCards: View {
    let store: UsageStore
    let theme: Theme
    @State private var picking = false
    /// The local sources found, looked for again with each report.
    @State private var sources: [SourceStatus] = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Models and sessions named on a card at most.
    static let listed = 3

    var body: some View {
        let usage = store.agentUsage, shown = store.shownAgents, dimensions = store.tokenDimensions
        let byVendor = Dictionary(usage.map { ($0.vendor, $0) }, uniquingKeysWith: { first, _ in first })
        let total = usage.reduce(0) { $0 + dimensions.count($1.tokens) }
        let vendors = detected(usage)
        let cards = (vendors + shown.subtracting(vendors).sorted()).filter(shown.contains)
        let perRow = Dictionary(uniqueKeysWithValues: Self.rows(cards).flatMap { row in row.map { ($0, row.count) } })
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.text("各 Agent", "By agent")).font(.ui(13, .semibold))
                Spacer()
                Button { picking.toggle() } label: {
                    HStack(spacing: 4) {
                        Text(L10n.text("显示 \(cards.count) 个", "\(cards.count) shown"))
                        Image(systemName: "chevron.down").font(.ui(9, .semibold))
                    }
                }
                .buttonStyle(.plain)
                .font(.ui(11))
                .foregroundStyle(theme.secondary)
                .popover(isPresented: $picking, arrowEdge: .bottom) { picker(vendors, usage: byVendor) }
            }
            // Each card keeps its place by agent, so picking one fades it in or out and slides the rest into their rows.
            CardRows(spacing: 12) {
                ForEach(cards, id: \.self) { vendor in
                    AgentCard(vendor: vendor, usage: byVendor[vendor], total: total, perRow: perRow[vendor] ?? 1, store: store, theme: theme)
                        .id(Self.anchor(vendor))
                        .transition(.opacity)
                }
            }
        }
        .task(id: store.report?.generatedAt) { sources = SourceDetector.detect() }
        .onChange(of: store.selectedQuotaId, initial: true) { _, id in
            // A quota event points at its agent's card, which shows even if it was not picked.
            guard let id, let vendor = store.rowGroups.first(where: { $0.rows.contains { $0.id == id } })?.vendor,
                  !store.shownAgents.contains(vendor) else { return }
            store.pickedAgents = store.shownAgents.union([vendor])
        }
    }

    static func anchor(_ vendor: String) -> String { "agent-" + vendor }

    /// Three cards to a row, and two to each of the last two rows where three would leave one alone; a row's cards share
    /// its width.
    nonisolated static func rows<T>(_ cards: [T]) -> [[T]] {
        var sizes = Array(repeating: 3, count: cards.count / 3)
        switch cards.count % 3 {
        case 2: sizes.append(2)
        case 1 where sizes.isEmpty: sizes.append(1)
        case 1: sizes.removeLast(); sizes += [2, 2]
        default: break
        }
        var start = 0
        return sizes.map { size in
            defer { start += size }
            return Array(cards[start..<start + size])
        }
    }

    /// The agents this Mac has, in the order Settings lists them: a local source found, a quota window read, or tokens in
    /// the ledger. Supported agents it does not have are left out.
    private func detected(_ usage: [AgentUsage]) -> [String] {
        let used = Set(store.consumers.map(\.vendor))
        let groups = AgentSettingsGroup.make(sources: SourceDetector.resolve(sources, report: store.report),
                                             agents: store.settings.agents, report: store.report)
        let found = groups.filter { group in
            let installed = group.source.map { source in
                switch source.state {
                case .notDetected: false
                case .ready, .installed, .unavailable: true
                }
            } ?? false
            return installed || used.contains(group.id) || group.agents.contains(where: \.connected)
        }.map(\.id)
        return found + usage.map(\.vendor).filter { !found.contains($0) }
    }

    private func picker(_ vendors: [String], usage: [String: AgentUsage]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(vendors, id: \.self) { vendor in
                Toggle(isOn: Binding(get: { store.shownAgents.contains(vendor) }, set: { on in
                    var picked = store.shownAgents
                    if on { picked.insert(vendor) } else { picked.remove(vendor) }
                    pick(picked)
                })) {
                    HStack(spacing: 6) {
                        AgentLogo(vendor: vendor, size: 12)
                        Text(VendorCatalog.name(vendor))
                        Spacer(minLength: 16)
                        Text(usage[vendor].map { TokenFormat.short(store.tokenDimensions.count($0.tokens)) } ?? "—")
                            .font(.tabular(11)).foregroundStyle(theme.secondary)
                    }
                }
                .toggleStyle(.checkbox)
            }
            if store.pickedAgents != nil {
                Button(L10n.text("恢复为设置里的 agent", "Back to the agents in Settings")) { pick(nil) }
                    .buttonStyle(.link).font(.ui(11)).padding(.top, 2)
            }
        }
        .font(.ui(12))
        .padding(14)
        .frame(minWidth: 240)
    }

    private func pick(_ vendors: Set<String>?) {
        withAnimation(reduceMotion ? nil : .easeOut(duration: 0.15)) { store.pickedAgents = vendors }
    }
}

/// Cards in the rows `AgentCards.rows` makes of them: a row's cards share its width and take its tallest card's height.
private struct CardRows: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 900, heights = rowHeights(width: width, subviews: subviews)
        return CGSize(width: width, height: heights.reduce(0, +) + spacing * CGFloat(max(0, heights.count - 1)))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for (row, height) in zip(AgentCards.rows(Array(subviews.indices)), rowHeights(width: bounds.width, subviews: subviews)) {
            let width = cardWidth(bounds.width, row.count)
            for (column, index) in row.enumerated() {
                subviews[index].place(at: CGPoint(x: bounds.minX + CGFloat(column) * (width + spacing), y: y), anchor: .topLeading,
                                      proposal: ProposedViewSize(width: width, height: height))
            }
            y += height + spacing
        }
    }

    private func rowHeights(width: CGFloat, subviews: Subviews) -> [CGFloat] {
        AgentCards.rows(Array(subviews.indices)).map { row in
            row.map { subviews[$0].sizeThatFits(ProposedViewSize(width: cardWidth(width, row.count), height: nil)).height }.max() ?? 0
        }
    }

    private func cardWidth(_ width: CGFloat, _ count: Int) -> CGFloat {
        (width - spacing * CGFloat(count - 1)) / CGFloat(count)
    }
}

private struct AgentCard: View {
    let vendor: String
    let usage: AgentUsage?
    /// Every agent's tokens of the selected kinds in the range, for this one's share.
    let total: Int
    /// Cards in this one's row: a wider card lays its kinds out in more columns, and one alone in its row puts its
    /// sessions beside its models.
    let perRow: Int
    let store: UsageStore
    let theme: Theme
    @State private var showsBilling = false

    private var dimensions: TokenDimensions { store.tokenDimensions }
    /// The colour of the vendor's mark; grey for a monochrome one.
    private var accent: Color { AgentArtwork.accent(for: vendor).map(Color.init(nsColor:)) ?? theme.secondary }
    private var billing: APIBilling? { store.report?.billing.first { $0.vendor == vendor } }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let usage {
                let count = dimensions.count(usage.tokens)
                header(share: total > 0 ? share(count, of: total) : nil)
                HStack(alignment: .bottom, spacing: 10) {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(value(count)).font(.tabular(24, .semibold))
                        Text(dimensions.label).font(.ui(11)).foregroundStyle(theme.secondary)
                    }
                    Spacer(minLength: 6)
                    Sparkline(values: store.agentSeries(vendor), color: accent).frame(maxWidth: CGFloat(330 / perRow)).frame(height: 28)
                }
                kinds(usage)
                if let note = moneyNote(usage) { Text(note).font(.ui(11)).foregroundStyle(theme.secondary).lineLimit(1) }
                if let billing { balance(billing) }
                if perRow == 1 {
                    HStack(alignment: .top, spacing: 16) {
                        models(usage)
                        Rectangle().fill(theme.divider).frame(width: 1)
                        sessions(usage, count: count)
                    }
                } else {
                    models(usage)
                    Rectangle().fill(theme.divider).frame(height: 1)
                    sessions(usage, count: count)
                }
            } else {
                // Nothing in the range: the agent is named, and an API account still shows its balance, in whatever height
                // its row gives it.
                header(share: nil)
                VStack(spacing: 6) {
                    Text(L10n.text("此时段没有用量", "No usage in this range")).foregroundStyle(theme.secondary)
                    if let billing { balance(billing) }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .font(.ui(12))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(theme.status(.ok), lineWidth: 2).opacity(isPointedOut ? 1 : 0))
    }

    private func header(share: String?) -> some View {
        HStack(spacing: 7) {
            AgentLogo(vendor: vendor, size: 14)
                .frame(width: 26, height: 26)
                .background(RoundedRectangle(cornerRadius: 6).fill(accent.opacity(0.18)))
            Text(VendorCatalog.name(vendor)).font(.ui(13, .semibold)).lineLimit(1)
            Spacer(minLength: 4)
            if let share {
                Text(L10n.text("占 ", "") + share)
                    .font(.tabular(11)).foregroundStyle(theme.secondary)
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(Capsule().fill(theme.segmentBackground))
                    .help(L10n.text("占这段时间全部 agent 用量的比例", "Share of every agent's tokens in the range"))
            }
        }
    }

    /// Each kind the agent spent, with the price of all of them.
    private func kinds(_ usage: AgentUsage) -> some View {
        let kinds = usage.tokens
        let cells: [(String, String)] = [
            (L10n.text("输入", "Input"), value(kinds.input)), (L10n.text("输出", "Output"), value(kinds.output)),
            (L10n.text("推理", "Reasoning"), value(kinds.reasoning)), (L10n.text("缓存写", "Cache write"), value(kinds.cacheWrite)),
            (L10n.text("缓存读", "Cache read"), value(kinds.cacheRead)),
            (L10n.text("费用", "Cost"), usage.cost?.text ?? "—"),
        ]
        let columns = 6 / perRow
        return Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 6) {
            ForEach(0..<cells.count / columns, id: \.self) { row in
                GridRow {
                    ForEach(0..<columns, id: \.self) { column in
                        let cell = cells[row * columns + column]
                        HStack(spacing: 6) {
                            Text(cell.0).font(.ui(11)).foregroundStyle(theme.secondary).lineLimit(1)
                            Spacer(minLength: 0)
                            Text(cell.1).font(.tabular(13, .semibold)).lineLimit(1).minimumScaleFactor(0.8)
                        }
                        .frame(maxWidth: .infinity)
                        .help(row * columns + column == cells.count - 1 ? L10n.text("按厂商 API 公开价折合，含缓存读取", "At the vendors' API list prices, cache reads included") : "")
                    }
                }
            }
        }
    }

    private func value(_ tokens: Int) -> String { tokens > 0 ? TokenFormat.short(tokens) : "—" }

    /// A share that rounds to nothing but is not nothing reads as under one percent.
    private func share(_ part: Int, of whole: Int) -> String {
        let percent = Double(part) / Double(max(1, whole)) * 100
        return part > 0 && percent < 0.5 ? "<1%" : TokenFormat.percent(percent)
    }

    /// Where the price went: reading context back from the cache, and sending it again after the cache lapsed.
    private func moneyNote(_ usage: AgentUsage) -> String? {
        let note = [usage.cacheReadShare.map { L10n.text("缓存读占费用 ", "Cache reads ") + TokenFormat.percent($0 * 100) + L10n.text("", " of cost") },
                    usage.recachedCost.map { L10n.text("缓存重写 ", "Cache rewrites ") + "≈" + MoneyFormat.amount($0, currency: "USD") }]
            .compactMap { $0 }
        return note.isEmpty ? nil : note.joined(separator: " · ")
    }

    /// The models by share, the rest summed on the last row when there are more than fit.
    private func models(_ usage: AgentUsage) -> some View {
        let models = usage.models, counts = models.map { dimensions.count($0.tokens) }, sum = max(1, counts.reduce(0, +))
        let named = models.count > AgentCards.listed ? AgentCards.listed - 1 : models.count
        return VStack(alignment: .leading, spacing: 5) {
            GeometryReader { proxy in
                HStack(spacing: 1) {
                    ForEach(Array(models.enumerated()), id: \.offset) { index, model in
                        Rectangle().fill(color(model.agentId)).frame(width: max(0, proxy.size.width * CGFloat(counts[index]) / CGFloat(sum) - 1))
                    }
                }
            }
            .frame(height: 6)
            .background(Capsule().fill(theme.track))
            .clipShape(Capsule())
            .padding(.bottom, 2)
            ForEach(0..<named, id: \.self) { index in
                modelRow(color(models[index].agentId), store.consumerName(models[index].agentId), counts[index], of: sum)
                    .help(models[index].cost.map { $0.text + L10n.text(" 按 API 价", " at API prices") } ?? L10n.text("没有公开价", "No list price"))
            }
            if named < models.count {
                modelRow(theme.tertiary, L10n.text("其余 \(models.count - named) 个模型", "\(models.count - named) more models"),
                         counts[named...].reduce(0, +), of: sum)
                    .foregroundStyle(theme.secondary)
                    .help(models[named...].map { store.consumerName($0.agentId) }.joined(separator: L10n.text("、", ", ")))
            }
        }
    }

    private func modelRow(_ color: Color, _ name: String, _ tokens: Int, of sum: Int) -> some View {
        HStack(spacing: 6) {
            RoundedRectangle(cornerRadius: 2).fill(color).frame(width: 8, height: 8)
            Text(name).lineLimit(1)
            Spacer(minLength: 6)
            Text(value(tokens)).font(.tabular(12))
            Text(share(tokens, of: sum)).font(.tabular(11)).foregroundStyle(theme.secondary).frame(width: 34, alignment: .trailing)
        }
    }

    private func sessions(_ usage: AgentUsage, count: Int) -> some View {
        let entries = usage.sessions.compactMap { entry in store.sessions.first { $0.id == entry.id }.map { (entry, $0) } }
            .prefix(AgentCards.listed)
        return VStack(alignment: .leading, spacing: 7) {
            Text(L10n.text("用量最多的会话", "Top sessions")).font(.ui(11)).foregroundStyle(theme.secondary)
            if entries.isEmpty {
                Text(L10n.text("此时段无会话", "No sessions in this range")).foregroundStyle(theme.secondary)
            }
            ForEach(Array(entries), id: \.1.id) { entry, session in
                let tokens = dimensions.count(entry.tokens)
                Button { store.focusedSessionID = session.id } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 6) {
                            Text(session.task).lineLimit(1).truncationMode(.tail)
                            Spacer(minLength: 6)
                            Text(value(tokens)).font(.tabular(12))
                        }
                        GeometryReader { proxy in
                            Capsule().fill(accent).frame(width: proxy.size.width * CGFloat(tokens) / CGFloat(max(1, count)))
                        }
                        .frame(height: 2)
                        .background(Capsule().fill(theme.track))
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(session.task + "\n" + (entry.cost.map { $0.text + L10n.text(" 按 API 价 · ", " at API prices · ") } ?? "")
                      + L10n.text("打开会话页", "Open the session page"))
            }
        }
    }

    private func color(_ id: String) -> Color { AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(id)) }

    /// Whatever opened the window pointed at one of this agent's quota windows.
    private var isPointedOut: Bool {
        guard let id = store.selectedQuotaId else { return false }
        return store.rowGroups.first { $0.rows.contains { $0.id == id } }?.vendor == vendor
    }

    /// An API account's balance and its own estimate for the range, with the pricing details a click away.
    private func balance(_ billing: APIBilling) -> some View {
        HStack(spacing: 6) {
            Text(([billing.balances.isEmpty ? nil
                   : L10n.text("余额 ", "Balance ") + billing.balances.map { MoneyFormat.amount($0.total, currency: $0.currency) }.joined(separator: " · "),
                   billing.estimatedCost(currency: billing.currency, during: store.statsInterval)
                    .map { L10n.text("账户估算 ", "Account estimate ") + MoneyFormat.amount($0, currency: billing.currency, estimated: true) }]
                  as [String?]).compactMap { $0 }.joined(separator: " · "))
                .lineLimit(1)
            Button { showsBilling.toggle() } label: { Image(systemName: "info.circle") }
                .buttonStyle(.plain)
                .accessibilityLabel(L10n.text("余额与计价详情", "Balance and pricing details"))
                .popover(isPresented: $showsBilling) { APIBillingCard(billing: billing, store: store, theme: theme).padding(18).frame(width: 430) }
        }
        .font(.ui(11))
        .foregroundStyle(theme.secondary)
    }
}

/// An agent's tokens over the range, one bar per chart bucket.
private struct Sparkline: View {
    let values: [Int]
    let color: Color

    var body: some View {
        Canvas { context, size in
            guard !values.isEmpty, let peak = values.max(), peak > 0 else { return }
            let slot = size.width / CGFloat(values.count), bar = max(1, slot * 0.6)
            for (index, value) in values.enumerated() where value > 0 {
                let height = max(1.5, size.height * CGFloat(value) / CGFloat(peak))
                context.fill(Path(CGRect(x: CGFloat(index) * slot + (slot - bar) / 2, y: size.height - height, width: bar, height: height)),
                             with: .color(color.opacity(0.85)))
            }
        }
        .accessibilityHidden(true)
    }
}
