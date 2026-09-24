import SwiftUI
import AgentHUDCore

/// The Sessions page: every session of the last seven days under the day it started on, newest first, below today's
/// totals. Today and any day with a session still running are open; the other days fold to their totals until clicked.
struct SessionList: View {
    let store: UsageStore
    let theme: Theme
    let source: SessionSource?
    /// Days the user opened or closed against how they start.
    @State private var flipped: Set<Date> = []

    var body: some View {
        let sessions = store.statsSessions.filter { source == nil || store.sessionSource($0) == source }
        let days = store.sessionsByDay(sessions)
        let calendar = Calendar.current, today = calendar.startOfDay(for: store.now)
        VStack(alignment: .leading, spacing: 12) {
            if let first = days.first, first.day == today {
                SessionsTodayCard(sessions: first.sessions, store: store, theme: theme)
            }
            ForEach(days, id: \.day) { day in
                let running = day.sessions.filter(store.isSessionLive).count
                let open = (day.day == today || running > 0) != flipped.contains(day.day)
                VStack(alignment: .leading, spacing: 6) {
                    dayHeader(day.day, sessions: day.sessions, running: running, open: open, today: today, calendar: calendar)
                    if open {
                        header
                        LazyVStack(spacing: 4) {
                            ForEach(day.sessions) { session in
                                SessionRow(session: session, store: store, theme: theme)
                            }
                        }
                    }
                }
                .card(theme, padding: EdgeInsets(top: 10, leading: 14, bottom: open ? 10 : 8, trailing: 14))
            }
            if sessions.isEmpty {
                Text(L10n.text("近 7 天没有会话", "No sessions in the last 7 days"))
                    .font(.ui(12)).foregroundStyle(theme.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .card(theme)
            }
        }
    }

    /// The day's name and count, and while it is folded what its sessions spent; a click folds or opens it.
    private func dayHeader(_ day: Date, sessions: [LiveSession], running: Int, open: Bool, today: Date, calendar: Calendar) -> some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) { flipped.formSymmetricDifference([day]) }
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(theme.secondary)
                    .rotationEffect(.degrees(open ? 90 : 0))
                    .frame(width: 10)
                Text(Self.title(day, today: today, calendar: calendar)).font(.ui(13, .semibold))
                Text("\(sessions.count)").font(.tabular(11)).foregroundStyle(theme.secondary)
                if running > 0 {
                    Text(L10n.text("\(running) 个运行中", "\(running) running")).font(.ui(11)).foregroundStyle(theme.status(.ok))
                }
                Spacer(minLength: 8)
                if !open {
                    if let costs = SessionsTodayCard.costs(sessions, store: store) {
                        Text(costs).font(.tabular(11)).foregroundStyle(theme.secondary)
                    }
                    Text(sessions.contains(where: \.hasTokenCounts)
                         ? TokenFormat.short(sessions.reduce(0) { $0 + store.tokenDimensions.count(store.sessionOwnTokens($1)) }) : "—")
                        .font(.tabular(11))
                        .frame(minWidth: 50, alignment: .trailing)
                        .help("Token · " + store.tokenDimensions.label)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Self.title(day, today: today, calendar: calendar))
        .accessibilityValue(open ? L10n.text("已展开", "Expanded") : L10n.text("已折叠", "Collapsed"))
    }

    /// Today, yesterday, then the weekday and date.
    static func title(_ day: Date, today: Date, calendar: Calendar) -> String {
        if day == today { return L10n.text("今天", "Today") }
        if day == calendar.date(byAdding: .day, value: -1, to: today) { return L10n.text("昨天", "Yesterday") }
        return day.formatted(.dateTime.weekday(.wide).month(.wide).day()
            .locale(Locale(identifier: L10n.resolved == .zhHans ? "zh_CN" : "en_GB")))
    }

    private var header: some View {
        HStack(spacing: 12) {
            Color.clear.frame(width: 8, height: 1)
            Text("Agent").frame(width: 130, alignment: .leading)
            Text(L10n.text("任务 · 项目", "Task · project")).frame(maxWidth: .infinity, alignment: .leading)
            Text(L10n.text("状态 · 轮次", "Status · turns")).frame(width: 112, alignment: .leading)
            Text(L10n.text("额度 / 费用", "Quota / cost")).frame(width: 70, alignment: .trailing)
            Text("Token · " + store.tokenDimensions.label)
                .lineLimit(1).minimumScaleFactor(0.8)
                .frame(width: 130, alignment: .trailing)
                .help(L10n.text("Token 为会话本身的累计，不含子 agent", "Tokens are the session's own totals, sub-agents not included"))
        }
        .font(.ui(10))
        .foregroundStyle(theme.secondary)
        .padding(.horizontal, 10)
    }
}

/// Today's sessions in one card: how many, what they cost, and every token they and their sub-agents spent by kind.
struct SessionsTodayCard: View {
    let sessions: [LiveSession]
    let store: UsageStore
    let theme: Theme

    var body: some View {
        let kinds = sessions.reduce(TokenKinds()) { $0 + store.sessionTokens($1) }
        let present = TokenKind.allCases.filter { kinds[$0] > 0 }
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text(L10n.text("今天 · \(sessions.count) 个会话", sessions.count == 1 ? "Today · 1 session" : "Today · \(sessions.count) sessions"))
                    .font(.ui(13, .semibold))
                Spacer(minLength: 8)
                if let costs {
                    Text(L10n.text("费用 ", "Cost ")).font(.ui(11)).foregroundStyle(theme.secondary)
                        + Text(costs).font(.tabular(12, .semibold))
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(TokenFormat.short(kinds.total)).font(.tabular(24, .semibold))
                Text(L10n.text("Token", "tokens")).font(.ui(12)).foregroundStyle(theme.secondary)
            }
            .padding(.top, 4)
            .padding(.bottom, 10)
            GeometryReader { proxy in
                let gaps = CGFloat(max(0, present.count - 1)) * 1.5
                HStack(spacing: 1.5) {
                    ForEach(present, id: \.self) { kind in
                        Rectangle().fill(theme.kind(kind))
                            .frame(width: max(1, (proxy.size.width - gaps) * CGFloat(kinds[kind]) / CGFloat(max(1, kinds.total))))
                    }
                }
            }
            .frame(height: 6)
            .clipShape(RoundedRectangle(cornerRadius: 3))
            .padding(.bottom, 10)
            // One row when the kinds fit side by side, else a grid.
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 20) {
                    ForEach(present, id: \.self) { legend($0, kinds) }
                    Spacer(minLength: 0)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 20, alignment: .leading)], alignment: .leading, spacing: 6) {
                    ForEach(present, id: \.self) { legend($0, kinds) }
                }
            }
        }
        .card(theme, padding: EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
    }

    private func legend(_ kind: TokenKind, _ kinds: TokenKinds) -> some View {
        HStack(spacing: 6) {
            RoundedRectangle(cornerRadius: 2).fill(theme.kind(kind)).frame(width: 7, height: 7)
            Text(kind.label).foregroundStyle(theme.secondary)
            Text(TokenFormat.short(kinds[kind])).font(.tabular(11))
        }
        .font(.ui(11))
        .fixedSize()
        .help("\(kind.label) \(kinds[kind].formatted())")
    }

    private var costs: String? { Self.costs(sessions, store: store) }

    /// Priced accounts' estimates and, for the rest, what their calls would cost at API list prices, added up per currency.
    static func costs(_ sessions: [LiveSession], store: UsageStore) -> String? {
        var sums: [String: Decimal] = [:]
        for session in sessions {
            switch store.sessionMoney(session) {
            case .account(let amount, let currency)?: sums[currency, default: 0] += amount
            case .listPrice(let amount)?: sums["USD", default: 0] += amount
            case .accountUnknown?, nil: break
            }
        }
        guard !sums.isEmpty else { return nil }
        return sums.keys.sorted().map { "≈" + MoneyFormat.amount(sums[$0]!, currency: $0) }.joined(separator: " · ")
    }
}

struct SessionRow: View {
    let session: LiveSession
    let store: UsageStore
    let theme: Theme
    @State private var hovered = false

    var body: some View {
        let dotColor = store.isSessionWaiting(session) ? theme.status(.warning)
            : store.isSessionLive(session) ? AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(session.agentId))
            : theme.dotEnded
        HStack(spacing: 12) {
            Circle().fill(dotColor).frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 5) {
                    if let agent = store.consumers.first(where: { $0.id == session.agentId }) ?? store.rows.first(where: { $0.id == session.agentId })?.agent {
                        AgentLogo(vendor: agent.vendor, size: 14)
                    }
                    Text(store.consumerName(session.agentId))
                }
                .font(.ui(12, .semibold))
                Text(store.sessionSource(session).name).font(.ui(10)).foregroundStyle(theme.secondary)
            }
            .lineLimit(1)
            .frame(width: 130, alignment: .leading)
            VStack(alignment: .leading, spacing: 3) {
                Text(session.task).foregroundStyle(theme.text)
                    .lineLimit(1).truncationMode(.tail)
                if session.accountWide {
                    Text(L10n.text("账户 · 跨设备", "Account · across devices")).font(.ui(10)).foregroundStyle(theme.secondary)
                } else if let path = session.displayPath {
                    Label(path, systemImage: "folder").labelStyle(.titleAndIcon)
                        .font(.ui(10)).foregroundStyle(theme.secondary)
                        .lineLimit(1).truncationMode(.middle)
                        .help(session.workingDirectory ?? path)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            VStack(alignment: .leading, spacing: 3) {
                Text(store.sessionStatusLabel(session))
                    .font(.tabular(12))
                    .lineLimit(1).minimumScaleFactor(0.8)
                let usage = store.sessionUsage(session)
                HStack(spacing: 6) {
                    if let turns = usage?.turnCount, turns > 0 {
                        Text(L10n.text("\(turns) 轮", turns == 1 ? "1 turn" : "\(turns) turns"))
                    }
                    if let usage, let fill = usage.contextFill { contextGauge(fill, usage) }
                }
                .font(.tabular(10))
            }
            .foregroundStyle(theme.secondary)
            .frame(width: 112, alignment: .leading)
            Text(quotaOrCost)
                .help(L10n.text("API 账户显示本会话费用估算；套餐显示本会话占当前 5 小时窗口的份额，没有份额时显示按 API 价估算的费用",
                                "APIs show the estimated session cost; plans show the session's share of the current 5-hour window, or without one what it would cost at API prices"))
                .font(.tabular(10, .semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.75)
                .frame(width: 70, alignment: .trailing)
            let tokens = store.sessionOwnTokens(session)
            Text(session.hasTokenCounts ? TokenFormat.short(store.tokenDimensions.count(tokens)) : "—")
                .help(TokenKind.allCases.map { "\($0.label) \(tokens[$0].formatted())" }.joined(separator: " · "))
                .font(.tabular(10))
                .foregroundStyle(theme.secondary)
                .frame(width: 130, alignment: .trailing)
        }
        .font(.ui(12))
        .padding(EdgeInsets(top: 4, leading: 10, bottom: 4, trailing: 10))
        .background(RoundedRectangle(cornerRadius: 7).fill(hovered ? theme.track : theme.sessionRowBackground))
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .onTapGesture { store.focusedSessionID = session.id }
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { store.focusedSessionID = session.id }
        .help([session.task, store.sessionSource(session).name, session.displayPath].compactMap { $0 }.joined(separator: " · "))
        .contextMenu {
            if let path = session.transcriptPath {
                Button(L10n.text("在 Finder 中显示日志", "Reveal log in Finder")) {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                }
            }
        }
    }

    /// How full the context of the session's latest call was, against its window.
    private func contextGauge(_ fill: Double, _ usage: SessionUsage) -> some View {
        HStack(spacing: 3) {
            Capsule().fill(theme.track).frame(width: 22, height: 3).overlay(alignment: .leading) {
                Capsule().fill(fill >= 0.8 ? theme.status(.warning) : theme.secondary).frame(width: 22 * min(1, fill), height: 3)
            }
            Text(TokenFormat.percent(fill * 100))
        }
        .help(L10n.text("上下文 ", "Context ") + [usage.contextTokens, usage.contextWindow].compactMap { $0.map(TokenFormat.short) }.joined(separator: " / "))
    }

    private var quotaOrCost: String {
        let money = store.sessionMoney(session)
        if case .listPrice? = money, let share = session.pctOfWindow { return TokenFormat.percent1(share) }
        return money?.text ?? session.pctOfWindow.map(TokenFormat.percent1) ?? "—"
    }
}
