import AppKit
import SwiftUI
import AgentHUDCore

/// One session in the statistics window: what it is, where its tokens went and what the agent last said.
struct SessionDetailView: View {
    let session: LiveSession
    let store: UsageStore
    let theme: Theme

    var body: some View {
        let usage = store.sessionUsage(session)
        VStack(alignment: .leading, spacing: 12) {
            header
            figures(usage)
            if let usage, !usage.periods.isEmpty { timeline(usage) }
            if let usage, !usage.models.isEmpty { models(usage) }
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
        [session.accountWide ? L10n.text("账户 · 跨设备", "Account · across devices") : session.terminal,
         L10n.text("开始于 ", "Started ") + ChartData.weekdayTime(session.startedAt),
         L10n.text("时长 ", "Duration ") + Countdown.format(session.duration(now: store.now))]
            .compactMap { $0 }.joined(separator: " · ")
    }

    // MARK: Figures

    private func figures(_ usage: SessionUsage?) -> some View {
        let total = usage?.total ?? .init(tokensIn: session.tokensIn, tokensOut: session.tokensOut, cacheReadTokens: session.cacheReadTokens)
        let dimensions = store.tokenDimensions
        return LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 12, alignment: .top)], spacing: 12) {
            figure("Token · " + dimensions.label, total.isEmpty ? "—" : TokenFormat.short(total.count(dimensions)),
                   note: usage?.subagents.map { L10n.text("含子 agent ", "Sub-agents ") + TokenFormat.short($0.count(dimensions)) }
                       ?? "In \(TokenFormat.short(total.tokensIn)) · Out \(TokenFormat.short(total.tokensOut))",
                   help: "In \(total.tokensIn.formatted()) · Out \(total.tokensOut.formatted()) · Cache \(total.cacheReadTokens.formatted())")
            figure(L10n.text("缓存命中", "Cache hits"), usage?.cacheHitRate.map { TokenFormat.percent($0 * 100) } ?? "—",
                   note: L10n.text("读取 ", "Read ") + TokenFormat.short(total.cacheReadTokens),
                   help: L10n.text("缓存读取占输入的比例", "Cache reads as a share of all input"))
            figure(L10n.text("上下文", "Context"), usage?.contextTokens.map(TokenFormat.short) ?? "—",
                   note: L10n.text("最近一次调用", "Latest call"),
                   help: L10n.text("最近一次模型调用的输入：新输入、缓存写入与缓存读取", "The latest model call's input: fresh, cache writes and cache reads"))
            figure(L10n.text("调用", "Calls"), usage.map { $0.calls.formatted() } ?? "—",
                   note: usage.map { L10n.text("\($0.models.count) 个模型", $0.models.count == 1 ? "1 model" : "\($0.models.count) models") } ?? "",
                   help: L10n.text("本机记录的模型调用次数", "Model calls this Mac recorded"))
            if let share = quotaOrCost {
                figure(share.title, share.value, note: share.note, help: share.note)
            }
        }
    }

    private func figure(_ title: String, _ value: String, note: String, help: String) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.ui(10)).foregroundStyle(theme.secondary).lineLimit(1)
            Text(value).font(.tabular(18, .semibold)).lineLimit(1).minimumScaleFactor(0.7)
            Text(note).font(.ui(10)).foregroundStyle(theme.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(theme, padding: EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
        .help(help)
    }

    /// Plans show the session's share of its quota window; priced accounts the estimated cost.
    private var quotaOrCost: (title: String, value: String, note: String)? {
        if let billing = store.report?.billing.first(where: { $0.sessionCosts[session.id] != nil }),
           let cost = billing.estimatedCost(currency: billing.currency, sessionId: session.id) {
            return (L10n.text("费用估算", "Estimated cost"), MoneyFormat.amount(cost, currency: billing.currency, estimated: true),
                    L10n.text("本会话", "This session"))
        }
        return session.pctOfWindow.map {
            (L10n.text("额度占比", "Quota share"), TokenFormat.percent1($0), L10n.text("占当前窗口", "Of the current window"))
        }
    }

    // MARK: Timeline

    private func timeline(_ usage: SessionUsage) -> some View {
        let chart = store.sessionColumns(session, usage: usage)
        let consumers = chart.agentIds.map(descriptor)
        let colors = chart.agentIds.map { AgentPalette.swiftUIColor(index: store.consumerPaletteIndex($0)) }
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(L10n.text("Token 消耗", "Tokens over time")).font(.ui(13, .semibold))
                Spacer()
                Text(ChartData.bucketSize(spanning: chart.interval.duration).label).font(.ui(11)).foregroundStyle(theme.secondary)
            }
            TokenBarsChart(columns: chart.columns, interval: chart.interval, colors: colors, consumers: consumers, theme: theme)
                .id([store.tokenDimensions.rawValue, chart.columns.count])
                .frame(height: 100)
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .zIndex(1)
            HStack {
                Text(ChartData.weekdayTime(chart.interval.start))
                Spacer()
                Text(session.isLive ? L10n.text("现在", "Now") : ChartData.weekdayTime(chart.interval.end))
            }
            .font(.ui(10))
            .foregroundStyle(theme.secondary)
            .padding(.horizontal, 12)
        }
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }

    // MARK: Models

    private func models(_ usage: SessionUsage) -> some View {
        let dimensions = store.tokenDimensions
        let total = max(1, usage.total.count(dimensions))
        return VStack(alignment: .leading, spacing: 8) {
            Text(L10n.text("模型", "Models")).font(.ui(13, .semibold))
            ForEach(usage.models, id: \.agentId) { model in
                let count = model.tokens.count(dimensions)
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
                    Text(TokenFormat.percent(Double(count) / Double(total) * 100)).font(.tabular(11)).foregroundStyle(theme.secondary)
                        .frame(width: 44, alignment: .trailing)
                    Text(TokenFormat.short(count)).font(.tabular(11)).frame(width: 60, alignment: .trailing)
                }
                .font(.ui(12))
                .help("In \(model.tokens.tokensIn.formatted()) · Out \(model.tokens.tokensOut.formatted()) · Cache \(model.tokens.cacheReadTokens.formatted())")
            }
            if let subagents = usage.subagents {
                Text(L10n.text("其中子 agent \(TokenFormat.short(subagents.count(dimensions)))，会话列表的合计不含这部分",
                               "Sub-agents spent \(TokenFormat.short(subagents.count(dimensions))) of this; the session list leaves them out"))
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
