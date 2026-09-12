import SwiftUI
import AgentHUDCore

/// Running and recently finished sessions from local logs.
struct LiveSessionsCard: View {
    let store: UsageStore
    let theme: Theme
    @State private var showAll = false
    @State private var selectedSource: SessionSource?

    private var sources: [SessionSource] {
        Array(Set(store.sessions.map(store.sessionSource))).sorted {
            if $0.vendor != $1.vendor { return ($0.vendor ?? "") < ($1.vendor ?? "") }
            return $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }
    }

    private var filteredSessions: [LiveSession] {
        store.statsSessions.filter { selectedSource == nil || store.sessionSource($0) == selectedSource }
    }

    private var previewSessions: [LiveSession] { store.sessionPreview(from: filteredSessions) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                HStack(spacing: 8) {
                    Circle().fill(filteredSessions.contains(where: store.isSessionLive) ? theme.status(.ok) : theme.tertiary).frame(width: 7, height: 7)
                    Text(L10n.text("会话", "Sessions")).font(.ui(13, .semibold))
                    Text(store.statsRange.recentLabel).font(.ui(11)).foregroundStyle(theme.secondary)
                }
                Spacer()
                SelectionMenu(
                    title: L10n.text("会话来源", "Session source"),
                    options: [SegmentOption(value: Optional<SessionSource>.none, label: L10n.text("全部", "All"))]
                        + sources.map { SegmentOption(value: Optional($0), label: $0.name) },
                    selection: $selectedSource,
                    theme: theme,
                    width: 185
                )
                .accessibilityIdentifier("session-source-filter")
                .onChange(of: selectedSource) { _, _ in showAll = false }
                .onChange(of: store.statsRange) { _, _ in showAll = false }
            }
            header
            LazyVStack(spacing: 4) {
                ForEach(showAll ? filteredSessions : previewSessions) { session in
                    SessionRow(session: session, store: store, theme: theme)
                }
            }
            if filteredSessions.isEmpty {
                Text(L10n.text("此时间窗口内没有会话", "No sessions in this range")).font(.ui(12)).foregroundStyle(theme.secondary).padding(.horizontal, 10)
            }
            HStack {
                Text(L10n.text(
                    "\(filteredSessions.count) 个会话 · \(filteredSessions.filter(store.isSessionLive).count) 个正在运行",
                    "\(filteredSessions.count) sessions · \(filteredSessions.filter(store.isSessionLive).count) running"
                ))
                Spacer()
                if filteredSessions.count > previewSessions.count {
                    Button(showAll ? L10n.text("收起", "Show less") : L10n.text("查看全部 \(filteredSessions.count) 个会话", "All \(filteredSessions.count) sessions")) { showAll.toggle() }
                        .buttonStyle(.plain)
                }
            }
            .font(.ui(11))
            .foregroundStyle(theme.secondary)
            .padding(.top, 4)
            .topDivider(theme.divider)
        }
        .card(theme, padding: EdgeInsets(top: 10, leading: 14, bottom: 10, trailing: 14))
    }

    private var header: some View {
        HStack(spacing: 12) {
            Color.clear.frame(width: 8, height: 1)
            Text("Agent").frame(width: 130, alignment: .leading)
            Text(L10n.text("任务 · 来源", "Task · source")).frame(maxWidth: .infinity, alignment: .leading)
            Text(L10n.text("时长", "Duration")).frame(width: 90, alignment: .leading)
            Text(L10n.text("额度 / 费用", "Quota / cost")).frame(width: 70, alignment: .trailing)
            Text("Token · " + store.tokenDimensions.label)
                .lineLimit(1).minimumScaleFactor(0.8)
                .frame(width: 130, alignment: .trailing)
                .help(L10n.text("按最近活动筛选 · Token 为会话累计", "Filtered by activity · tokens are session totals"))
        }
        .font(.ui(10))
        .foregroundStyle(theme.secondary)
        .padding(.horizontal, 10)
    }
}

struct SessionRow: View {
    let session: LiveSession
    let store: UsageStore
    let theme: Theme

    var body: some View {
        let dotColor = store.isSessionLive(session) ? AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(session.agentId)) : theme.dotEnded
        HStack(spacing: 12) {
            Circle().fill(dotColor).frame(width: 8, height: 8)
            HStack(spacing: 5) {
                if let agent = store.consumers.first(where: { $0.id == session.agentId }) ?? store.rows.first(where: { $0.id == session.agentId })?.agent {
                    AgentLogo(vendor: agent.vendor, size: 14)
                }
                Text(store.consumerName(session.agentId))
            }
                .font(.ui(12, .semibold))
                .lineLimit(1)
                .frame(width: 130, alignment: .leading)
            VStack(alignment: .leading, spacing: 3) {
                Text(session.task).foregroundStyle(theme.text)
                    .lineLimit(1).truncationMode(.tail)
                Text([store.sessionSource(session).name,
                    session.accountWide ? L10n.text("账户 · 跨设备", "Account · across devices") : session.terminal].compactMap { $0 }.joined(separator: " · "))
                    .font(.ui(10)).foregroundStyle(theme.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Text(store.sessionStatusLabel(session))
                .font(.tabular(12))
                .lineLimit(1).minimumScaleFactor(0.8)
                .foregroundStyle(theme.secondary)
                .frame(width: 90, alignment: .leading)
            Text(quotaOrCost)
                .help(L10n.text("订阅显示额度占比；API 显示本会话费用估算", "Subscriptions show quota share; APIs show the estimated session cost"))
                .font(.tabular(10, .semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.75)
                .frame(width: 70, alignment: .trailing)
            Text(session.hasTokenCounts ? TokenFormat.short(store.tokenDimensions.count(input: session.tokensIn, output: session.tokensOut,
                                                                                       cache: session.cacheReadTokens)) : "—")
                .help("In \(session.tokensIn.formatted()) · Out \(session.tokensOut.formatted()) · Cache \(session.cacheReadTokens.formatted())")
                .font(.tabular(10))
                .foregroundStyle(theme.secondary)
                .frame(width: 130, alignment: .trailing)
        }
        .font(.ui(12))
        .padding(EdgeInsets(top: 4, leading: 10, bottom: 4, trailing: 10))
        .background(RoundedRectangle(cornerRadius: 7).fill(theme.sessionRowBackground))
        .help([session.task, store.sessionSource(session).name, session.terminal].compactMap { $0 }.joined(separator: " · "))
        .contextMenu {
            if let path = session.transcriptPath {
                Button(L10n.text("在 Finder 中显示日志", "Reveal log in Finder")) {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                }
            }
        }
    }

    private var quotaOrCost: String {
        if let billing = store.report?.billing.first(where: { $0.costs.contains(where: { $0.sessionId == session.id }) }) {
            return billing.estimatedCost(currency: billing.currency, sessionId: session.id)
                .map { "≈" + MoneyFormat.amount($0, currency: billing.currency, estimated: true) } ?? "—"
        }
        return session.pctOfWindow.map(TokenFormat.percent1) ?? "—"
    }
}
