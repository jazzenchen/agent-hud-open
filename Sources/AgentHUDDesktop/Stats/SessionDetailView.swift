import AppKit
import SwiftUI
import AgentHUDCore

/// One session in the statistics window: what it is, where its tokens went turn by turn, and what the agent last said.
struct SessionDetailView: View {
    let session: LiveSession
    let store: UsageStore
    let theme: Theme
    @State private var inspected: Int?

    var body: some View {
        let usage = store.sessionUsage(session)
        VStack(alignment: .leading, spacing: 12) {
            header
            figures(usage)
            if let usage {
                tokens(usage)
                kinds(usage)
                if usage.models.count > 1 { models(usage) }
            }
            if let message = store.sessionMessage(session) { lastMessage(message) }
        }
    }

    // MARK: Header

    private var header: some View {
        let source = store.sessionSource(session)
        let dot = store.isSessionWaiting(session) ? theme.status(.warning)
            : store.isSessionLive(session) ? AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(session.agentId)) : theme.dotEnded
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Circle().fill(dot).frame(width: 8, height: 8)
                if let vendor = source.vendor { AgentLogo(vendor: vendor, size: 14) }
                Text(store.consumerName(session.agentId)).font(.ui(12, .semibold))
                Text(source.name).font(.ui(12)).foregroundStyle(theme.secondary)
                Spacer(minLength: 8)
                Text(store.sessionStatusLabel(session)).font(.tabular(12)).foregroundStyle(theme.secondary)
            }
            Text(session.task)
                .font(.ui(16, .semibold))
                .lineLimit(3)
                .textSelection(.enabled)
            HStack(spacing: 12) {
                Text(details).font(.ui(11)).foregroundStyle(theme.secondary).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 8)
                if let path = session.transcriptPath {
                    Button(L10n.text("在 Finder 中显示日志", "Reveal log in Finder")) {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                    }
                    .buttonStyle(.link)
                    .font(.ui(11))
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }

    /// Project, start time and how long the session has run.
    private var details: String {
        [session.accountWide ? L10n.text("账户 · 跨设备", "Account · across devices") : session.displayPath,
         L10n.text("开始于 ", "Started ") + ChartData.weekdayTime(session.startedAt),
         L10n.text("时长 ", "Duration ") + Countdown.format(session.duration(now: store.now))]
            .compactMap { $0 }.joined(separator: " · ")
    }

    // MARK: Figures

    private func figures(_ usage: SessionUsage?) -> some View {
        let total = store.sessionTokens(session)
        var items: [(title: String, value: String, note: String, help: String)] = [
            (L10n.text("Token", "Tokens"), total.isEmpty ? "—" : TokenFormat.short(total.total),
             usage?.subagents.map { L10n.text("含子 agent ", "Sub-agents ") + TokenFormat.short($0.kinds.total) }
                ?? L10n.text("不含缓存读取 ", "Excl. cache reads ") + TokenFormat.short(total.new),
             L10n.text("五类合计，含缓存读取", "All five kinds, cache reads included")),
        ]
        if let cost {
            items.append((L10n.text("费用", "Cost"), cost.value, cost.note, cost.help))
        }
        if let share = session.pctOfWindow {
            items.append((L10n.text("额度占比", "Quota share"), TokenFormat.percent1(share), L10n.text("占当前窗口", "Of the current window"),
                          L10n.text("本会话用掉的当前额度窗口", "What this session used of the current quota window")))
        }
        items.append((L10n.text("轮次", "Turns"), usage.flatMap { $0.turnCount > 0 ? $0.turnCount.formatted() : nil } ?? "—",
                      usage.map { L10n.text("\($0.calls) 次调用", $0.calls == 1 ? "1 call" : "\($0.calls.formatted()) calls") } ?? "",
                      L10n.text("每个 prompt 开始一轮", "Each prompt starts a turn")))
        items.append((L10n.text("上下文", "Context"), context(usage), usage?.contextWindow.map { L10n.text("共 ", "Of ") + TokenFormat.short($0) }
                        ?? L10n.text("最近一次调用", "Latest call"),
                      L10n.text("最近一次模型调用的输入：新输入、缓存写入与缓存读取", "The latest model call's input: fresh, cache writes and cache reads")))
        items.append((L10n.text("缓存命中", "Cache hits"), total.cacheHitRate.map { TokenFormat.percent($0 * 100) } ?? "—",
                      L10n.text("提示读自缓存", "Of prompts, from cache"),
                      L10n.text("缓存读取占全部提示的比例：新输入、缓存写入与缓存读取", "Cache reads as a share of every prompt: fresh input, cache writes and cache reads")))
        return HStack(alignment: .top, spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                VStack(spacing: 4) {
                    Text(item.title).font(.ui(10)).foregroundStyle(theme.secondary).lineLimit(1)
                    Text(item.value).font(.tabular(17, .semibold)).lineLimit(1).minimumScaleFactor(0.7)
                    Text(item.note).font(.ui(10)).foregroundStyle(theme.secondary).lineLimit(1).minimumScaleFactor(0.8)
                }
                .frame(maxWidth: .infinity)
                .help(item.help)
                if index < items.count - 1 { Rectangle().fill(theme.divider).frame(width: 1, height: 34).padding(.top, 6) }
            }
        }
        .card(theme, padding: EdgeInsets(top: 10, leading: 8, bottom: 10, trailing: 8))
    }

    private func context(_ usage: SessionUsage?) -> String {
        if let fill = usage?.contextFill { return TokenFormat.percent(fill * 100) }
        return usage?.contextTokens.map(TokenFormat.short) ?? "—"
    }

    /// Priced accounts show their estimated cost; other sessions what their calls would cost at the API's list price.
    private var cost: (value: String, note: String, help: String)? {
        switch store.sessionMoney(session) {
        case .listPrice?:
            return (store.sessionMoney(session)!.text, L10n.text("按 API 价", "At API prices"),
                    L10n.text("同样的调用按厂商 API 公开价计算的费用，不是实际扣费", "What the same calls cost at the vendor's API list price; not a charge"))
        case let money?:
            return (money.text, L10n.text("本会话", "This session"), L10n.text("按账户价格估算的本会话费用", "This session's cost at the account's prices"))
        case nil:
            return nil
        }
    }

    // MARK: Tokens

    private func tokens(_ usage: SessionUsage) -> some View {
        let byTurn = !usage.turns.isEmpty
        let bars = byTurn ? SessionBar.turns(usage) : SessionBar.periods(usage)
        let shown = SessionBarsChart.stacked.filter { kind in bars.contains { $0.kinds[kind] > 0 } }
        let hasContext = bars.contains { $0.context != nil }
        return VStack(alignment: .leading, spacing: 8) {
            Text(byTurn ? L10n.text("每轮 Token", "Tokens per turn") : L10n.text("Token 消耗", "Tokens over time")).font(.ui(13, .semibold))
            HStack(spacing: 12) {
                ForEach(shown, id: \.self) { kind in
                    HStack(spacing: 5) {
                        RoundedRectangle(cornerRadius: 2).fill(theme.kind(kind)).frame(width: 7, height: 7)
                        Text(kind.label)
                    }
                }
                Spacer(minLength: 8)
                Text(L10n.text("悬停查看每一根柱子", "Hover a bar for details"))
            }
            .font(.ui(10))
            .foregroundStyle(theme.secondary)
            SessionBarsChart(bars: bars, axis: byTurn ? .turn : .time, running: byTurn && store.isSessionLive(session), theme: theme,
                             inspected: $inspected)
            if hasContext {
                HStack(alignment: .firstTextBaseline) {
                    Text(L10n.text("上下文窗口", "Context window")).font(.ui(12, .semibold))
                    Spacer()
                    Text(L10n.text("缓存读取 + 新输入", "Cache read + new input")
                         + (usage.contextWindow.map { L10n.text(" · 上限 ", " · limit ") + TokenFormat.short($0) } ?? ""))
                        .font(.ui(10)).foregroundStyle(theme.secondary)
                }
                .padding(.top, 8)
                .topDivider(theme.divider)
                ContextWindowChart(bars: bars, axis: byTurn ? .turn : .time, window: usage.contextWindow, theme: theme)
            }
        }
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }

    // MARK: Kinds

    private func kinds(_ usage: SessionUsage) -> some View {
        let total = usage.total.kinds, sum = max(1, total.total)
        return VStack(spacing: 0) {
            ForEach(Array(TokenKind.allCases.filter { total[$0] > 0 }.enumerated()), id: \.element) { index, kind in
                HStack(spacing: 10) {
                    RoundedRectangle(cornerRadius: 2.5).fill(theme.kind(kind)).frame(width: 9, height: 9)
                    Text(kind.label).font(.ui(12))
                    Spacer()
                    Text(TokenFormat.short(total[kind])).font(.tabular(12, .semibold)).help(total[kind].formatted())
                    Text(share(total[kind], of: sum)).font(.tabular(11)).foregroundStyle(theme.secondary).frame(width: 40, alignment: .trailing)
                }
                .padding(.vertical, 7)
                .overlay(alignment: .top) { if index > 0 { Rectangle().fill(theme.divider).frame(height: 1) } }
            }
        }
        .card(theme, padding: EdgeInsets(top: 3, leading: 14, bottom: 3, trailing: 14))
    }

    private func share(_ value: Int, of total: Int) -> String {
        let fraction = Double(value) / Double(total)
        return fraction < 0.01 ? "<1%" : TokenFormat.percent(fraction * 100)
    }

    // MARK: Models

    private func models(_ usage: SessionUsage) -> some View {
        let total = max(1, usage.total.kinds.total)
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text(L10n.text("模型", "Models")).font(.ui(13, .semibold))
                Spacer()
                Text(L10n.text("缓存命中", "Cache hits")).frame(width: 60, alignment: .trailing)
                Text(L10n.text("占比", "Share")).frame(width: 44, alignment: .trailing)
                Text("Token").frame(width: 60, alignment: .trailing)
            }
            .font(.ui(10)).foregroundStyle(theme.secondary)
            ForEach(usage.models, id: \.agentId) { model in
                let count = model.tokens.kinds.total
                let agent = descriptor(model.agentId)
                HStack(spacing: 8) {
                    Circle().fill(AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(model.agentId))).frame(width: 7, height: 7)
                    AgentLogo(vendor: agent.vendor, size: 12)
                    Text(store.consumerName(model.agentId)).lineLimit(1).frame(width: 150, alignment: .leading)
                    GeometryReader { proxy in
                        ZStack(alignment: .leading) {
                            Capsule().fill(theme.track)
                            Capsule().fill(AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(model.agentId)))
                                .frame(width: proxy.size.width * CGFloat(count) / CGFloat(total))
                        }
                    }
                    .frame(height: 5)
                    Text(model.tokens.kinds.cacheHitRate.map { TokenFormat.percent($0 * 100) } ?? "—").font(.tabular(11))
                        .foregroundStyle(theme.secondary).frame(width: 60, alignment: .trailing)
                    Text(share(count, of: total)).font(.tabular(11)).foregroundStyle(theme.secondary)
                        .frame(width: 44, alignment: .trailing)
                    Text(TokenFormat.short(count)).font(.tabular(11)).frame(width: 60, alignment: .trailing)
                }
                .font(.ui(12))
                .help(TokenKind.allCases.map { "\($0.label) \(model.tokens.kinds[$0].formatted())" }.joined(separator: " · "))
            }
            if let subagents = usage.subagents {
                Text(L10n.text("其中子 agent \(TokenFormat.short(subagents.kinds.total))，会话列表的合计不含这部分",
                               "Sub-agents spent \(TokenFormat.short(subagents.kinds.total)) of this; the session list leaves them out"))
                    .font(.ui(10)).foregroundStyle(theme.secondary)
            }
        }
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }

    private func descriptor(_ id: String) -> AgentDescriptor {
        store.consumers.first { $0.id == id }
            ?? AgentDescriptor(id: id, vendor: store.sessionSource(session).vendor ?? "", model: id, source: "", enabled: true)
    }

    // MARK: Last message

    private func lastMessage(_ message: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n.text("最近回复", "Last reply")).font(.ui(13, .semibold))
            Text(message)
                .font(.ui(12))
                .foregroundStyle(theme.secondary)
                .lineLimit(12)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }
}

/// A session's money, as its row and its page both show it; always an estimate.
enum SessionMoney {
    /// A priced account's own estimate, in the currency its balance is in.
    case account(Decimal, currency: String)
    /// A priced account with no estimate in that currency.
    case accountUnknown
    /// What the calls would cost at the vendors' API list prices, in US dollars.
    case listPrice(Decimal)

    var text: String {
        switch self {
        case .account(let amount, let currency): "≈" + MoneyFormat.amount(amount, currency: currency, estimated: true)
        case .accountUnknown: "—"
        case .listPrice(let amount): "≈" + MoneyFormat.amount(amount, currency: "USD")
        }
    }
}

extension UsageStore {
    func sessionMoney(_ session: LiveSession) -> SessionMoney? {
        if let billing = report?.billing.first(where: { $0.sessionCosts[session.id] != nil }) {
            return billing.estimatedCost(currency: billing.currency, sessionId: session.id).map { .account($0, currency: billing.currency) }
                ?? .accountUnknown
        }
        return sessionUsage(session)?.listCost.map(SessionMoney.listPrice)
    }
}
