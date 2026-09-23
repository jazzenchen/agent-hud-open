import SwiftUI
import AgentHUDCore

/// Reports the panel's natural height so the island window can size itself to the content.
struct PanelHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

/// Shared columns for quota rows, account resets and API balances in the island.
enum IslandRowLayout {
    static let inset: CGFloat = 6
    static let markerWidth: CGFloat = 8
    static let spacing: CGFloat = 12
    static let nameWidth: CGFloat = 152
    static let textInset = inset + markerWidth + spacing
    static let headingFont = Font.ui(13, .bold)
    static let headingVerticalPadding: CGFloat = 1
}

/// The three readings a quota block can show. The Mac uses direct buttons where the phone uses a horizontal swipe.
enum IslandQuotaMetric: CaseIterable, Identifiable {
    case quota, burnRate, tokens

    var id: Self { self }

    var label: String {
        switch self {
        case .quota: L10n.text("额度", "Quota")
        case .burnRate: L10n.text("消耗速率", "Burn rate")
        case .tokens: L10n.text("Token 速率", "Tokens / h")
        }
    }

    var symbol: String {
        switch self {
        case .quota: "percent"
        case .burnRate: "flame.fill"
        case .tokens: "number"
        }
    }
}

/// Compact, direct metric selection for a provider block.
struct IslandQuotaMetricPicker: View {
    @Binding var selection: IslandQuotaMetric
    var theme: Theme = .island
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 2) {
            ForEach(IslandQuotaMetric.allCases) { metric in
                Button {
                    withAnimation(reduceMotion ? nil : .snappy(duration: 0.28)) { selection = metric }
                } label: {
                    Image(systemName: metric.symbol)
                        .font(.ui(9, .semibold))
                        .frame(width: 20, height: 18)
                        .foregroundStyle(selection == metric ? theme.text : theme.tertiary)
                        .background(
                            RoundedRectangle(cornerRadius: 5)
                                .fill(selection == metric ? theme.text.opacity(0.12) : .clear)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(metric.label)
                .accessibilityLabel(metric.label)
                .accessibilityAddTraits(selection == metric ? .isSelected : [])
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(L10n.text("额度指标", "Quota metric"))
    }
}

/// User-selected quota rows, token usage and active sessions.
struct HoverPanelView: View {
    let store: UsageStore
    let onOpenStats: () -> Void
    var onOpenSettings: () -> Void = {}
    var alert: IslandAlert? = nil
    var onOpenAlert: () -> Void = {}
    var onDecideAlert: (PermissionDecision) -> Void = { _ in }
    var waitingRequests: [PermissionRequest] = []

    private let theme = Theme.island

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let error = store.lastError {
                Text(L10n.text("刷新失败：", "Refresh failed: ") + error)
                    .font(.ui(11)).foregroundStyle(theme.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let alert {
                IslandAlertInlineView(alert: alert, onOpen: onOpenAlert, onDecide: onDecideAlert,
                                      waitingRequests: waitingRequests).id(alert.id)
                    .padding(.bottom, 4)
            }
            if store.settings.settings.showIslandQuota, !store.rows.isEmpty {
                quotaBlock
            }
            if store.settings.settings.showIslandQuota {
                ForEach(store.enabledBilling) { billing in
                    APIBillingCard(billing: billing, store: store, theme: theme, compact: true)
                        .padding(.top, 8)
                        .topDivider(theme.divider)
                }
            }
            if store.settings.settings.showIslandTokens, store.isLoading || store.isIndexing || !store.consumers.isEmpty {
                TokenConsumptionChart(store: store, theme: theme, context: .island)
                    .padding(.top, 8)
                    .topDivider(theme.divider)
            }
            if store.settings.settings.showIslandSessions {
                sessionLine
            }
            footer
        }
        .padding(EdgeInsets(top: 32, leading: 18, bottom: 14, trailing: 18))
        .foregroundStyle(theme.text)
        .background(GeometryReader { proxy in
            Color.clear.preference(key: PanelHeightKey.self, value: proxy.size.height)
        })
    }

    private var quotaBlock: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(store.rowGroups.enumerated()), id: \.element.vendor) { index, group in
                ProviderQuotaBlock(store: store, vendor: group.vendor, rows: group.rows)
                .padding(.top, index > 0 ? 8 : 0)
                .topDivider(index > 0 ? theme.divider : .clear)
            }
            if store.rows.isEmpty {
                Text(L10n.text("暂无额度数据", "No quota data yet"))
                    .font(.ui(12))
                    .foregroundStyle(theme.secondary)
            }
        }
    }

    /// What is running right now: every running session, up to `sessionRowLimit` of them, and the rest as a count.
    /// The statistics window's range never applies here — that range belongs to the session card, which answers a
    /// different question. When nothing is running, the sessions that ended most recently take the same rows.
    private var sessionLine: some View {
        Button(action: onOpenStats) {
            let running = store.liveSessions
            let shown = Array((running.isEmpty ? store.sessions : running).prefix(Self.sessionRowLimit))
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Circle().fill(running.isEmpty ? theme.tertiary : theme.status(.ok)).frame(width: 6, height: 6)
                    Text(L10n.text("活跃会话", "Active sessions")).foregroundStyle(theme.text)
                    Spacer()
                    Text(running.isEmpty
                         ? L10n.text("最近结束", "Recently ended")
                         : L10n.text("\(running.count) 个运行中", "\(running.count) running"))
                        .foregroundStyle(theme.secondary)
                }
                if shown.isEmpty {
                    Text(L10n.text("还没有会话", "No sessions yet"))
                        .foregroundStyle(theme.secondary)
                } else {
                    ForEach(shown) { session in
                        HStack(spacing: 8) {
                            Circle().fill(sessionDot(session)).frame(width: 6, height: 6)
                            Text("\(Self.shortTask(session.task)) · \(session.terminal ?? "—")")
                                .lineLimit(1)
                                .truncationMode(.tail)
                            Spacer(minLength: 8)
                            Text("\(TokenFormat.short(session.tokensIn + session.tokensOut)) tok")
                                .fixedSize()
                        }
                        .foregroundStyle(theme.secondary)
                    }
                    if running.count > shown.count {
                        Text(L10n.text("还有 \(running.count - shown.count) 个", "+\(running.count - shown.count) more"))
                            .foregroundStyle(theme.secondary)
                    }
                }
            }
            .font(.ui(12))
            .padding(.top, 8)
            .topDivider(theme.divider)
        }
        .buttonStyle(.plain)
        .help(L10n.text("查看会话列表", "Show sessions"))
    }

    /// A running session wears its agent's colour, one blocked on the user the warning colour, and an ended one grey.
    private func sessionDot(_ session: LiveSession) -> Color {
        if store.isSessionWaiting(session) { return theme.status(.warning) }
        guard store.isSessionLive(session) else { return theme.dotEnded }
        return AgentPalette.swiftUIColor(index: store.consumerPaletteIndex(session.agentId))
    }

    private var footer: some View {
        HStack {
            Button(action: onOpenSettings) {
                Image(systemName: "gearshape")
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .help(L10n.text("设置", "Settings"))
            .accessibilityLabel(L10n.text("设置", "Settings"))
            Spacer()
            Button(action: onOpenStats) {
                Image(systemName: "chart.bar.xaxis")
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .help(L10n.text("用量统计", "Usage statistics"))
            .accessibilityLabel(L10n.text("用量统计", "Usage statistics"))
        }
        .buttonStyle(.plain)
        .font(.ui(14))
        .foregroundStyle(theme.secondary)
        .padding(.top, 6)
        .overlay(alignment: .top) { Rectangle().fill(theme.divider).frame(height: 1) }
    }

    /// "fix auth bug in middleware" → "fix auth bug"
    /// How many session rows the island shows before the rest become a count.
    static let sessionRowLimit = 3

    static func shortTask(_ task: String) -> String {
        let words = task.split(separator: " ")
        if words.count > 3 { return words.prefix(3).joined(separator: " ") }
        return String(task.prefix(14))
    }
}

/// One independently switchable provider block. Refreshes keep its selected metric because the vendor is its identity.
private struct ProviderQuotaBlock: View {
    let store: UsageStore
    let vendor: String
    let rows: [AgentRow]
    @State private var metric = IslandQuotaMetric.quota
    private let theme = Theme.island

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                AgentLogo(vendor: vendor, size: 14)
                    .frame(width: IslandRowLayout.inset * 2 + IslandRowLayout.markerWidth)
                Text(vendor)
                    .font(IslandRowLayout.headingFont)
                    .foregroundStyle(theme.text)
                if store.isLoading {
                    LoadingSpinner(color: theme.secondary)
                        .accessibilityLabel(L10n.text("正在读取额度", "Loading quota"))
                }
                Spacer()
                IslandQuotaMetricPicker(selection: $metric, theme: theme)
                    .padding(.trailing, 2)
            }
            .font(.ui(11))
            .foregroundStyle(theme.secondary)
            .padding(.vertical, IslandRowLayout.headingVerticalPadding)

            let sections = store.accountSections(rows)
            ForEach(sections) { section in
                if let account = section.account {
                    AccountSectionHeader(account: account, now: store.now,
                                         notice: account.quotaNotice ?? store.report?.sourceNotices[account.account.provider])
                }
                ForEach(section.rows) { row in
                    ModelUsageRow(row: row, now: store.now, metric: metric,
                                  showReset: store.settings.settings.showResetCountdown, showVendor: false,
                                  insights: store.report?.insightsByAgent[row.id],
                                  tokensPerHour: store.quotaTokensPerHour(for: row.id),
                                  forecastHint: section.isCurrent ? store.quotaForecastHint(for: row.id) : nil,
                                  isLoading: store.isLoading)
                        .opacity(section.isCurrent ? 1 : 0.55)
                }
                // Earned resets belong to the signed-in Codex account.
                if section.isCurrent, section.rows.contains(where: { $0.agent.vendor == "Codex" }),
                   let resets = store.report?.resetCredits(for: section.id) {
                    CodexResetCreditsView(resets: resets, showExpiry: store.settings.settings.showResetCountdown)
                }
            }
        }
    }
}

/// Names the account above its windows once a client has more than one, or when it is no longer signed in. A client
/// that said why it has no current reading says it here, where the stale rows are.
struct AccountSectionHeader: View {
    let account: AccountObservation
    let now: Date
    var notice: String?
    private let theme = Theme.island

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Text(account.displayName)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .foregroundStyle(theme.text.opacity(account.isCurrent ? 0.85 : 0.6))
                if let plan = account.planLabel {
                    Text(plan).foregroundStyle(theme.secondary)
                }
                Spacer(minLength: 8)
                Text(account.statusLabel(now: now))
                    .foregroundStyle(theme.tertiary)
                    .lineLimit(1)
            }
            if let notice {
                Text(notice)
                    .foregroundStyle(theme.statusText(.warning))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .font(.ui(11))
        .padding(.horizontal, IslandRowLayout.inset)
        .padding(.top, 2)
        .accessibilityElement(children: .combine)
    }
}

/// Earned resets belong to the account, so they appear once beneath its quota windows.
struct CodexResetCreditsView: View {
    let resets: CodexResetCredits
    let showExpiry: Bool
    private let theme = Theme.island
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: IslandRowLayout.spacing) {
            Image(systemName: "arrow.counterclockwise")
                .font(.ui(12, .medium))
                .frame(width: IslandRowLayout.markerWidth)
            Text(L10n.text("额度重置", "Usage resets"))
                .font(.ui(12, .medium))
                .frame(width: IslandRowLayout.nameWidth, alignment: .leading)
            Text(availabilityLabel)
                .font(.tabular(12, .semibold))
                .foregroundStyle(resets.availableCount > 0 ? theme.status(.ok) : theme.secondary)
                .fixedSize()
            Spacer(minLength: 8)
            if resets.availableCount > 0 {
                if showExpiry {
                    Text(nextExpiryLabel)
                        .font(.tabular(11))
                }
                Image(systemName: "info.circle")
                    .font(.ui(11))
                    .foregroundStyle(isHovered ? theme.secondary : theme.tertiary)
            }
        }
        .lineLimit(1)
        .foregroundStyle(theme.secondary)
        .padding(.horizontal, IslandRowLayout.inset)
        .padding(.bottom, 2)
        .background(RoundedRectangle(cornerRadius: 6).fill(theme.text.opacity(isHovered ? 0.06 : 0)))
        .contentShape(RoundedRectangle(cornerRadius: 6))
        .background(IslandHoverPopover(content: ResetCreditsDetails(resets: resets),
                                       enabled: resets.availableCount > 0, isHovered: $isHovered))
        .animation(.easeOut(duration: 0.12), value: isHovered)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(L10n.text("额度重置", "Usage resets"))
        .accessibilityValue(ResetCreditsDetails.accessibilityText(resets))
    }

    private var availabilityLabel: String {
        L10n.text("\(resets.availableCount) 次可用", "\(resets.availableCount) available")
    }

    private var nextExpiryLabel: String {
        let dates = resets.creditsByExpiry.compactMap(\.expirationDate)
        guard let date = dates.first else {
            return L10n.text("有效期未提供", "Expiry unavailable")
        }
        guard dates.count == resets.availableCount else {
            return L10n.text("部分有效期可查看", "Partial expiry details")
        }
        let label = date.formatted(Date.FormatStyle().month(.abbreviated).day().locale(dateLocale))
        return L10n.text("最早 \(label) 到期", "First expires \(label)")
    }

    private var dateLocale: Locale {
        Locale(identifier: L10n.resolved == .zhHans ? "zh_CN" : "en_GB")
    }
}

/// Aligned quota labels and the selected measure with a flexible progress bar between them.
struct ModelUsageRow: View {
    let row: AgentRow
    let now: Date
    var metric = IslandQuotaMetric.quota
    let showReset: Bool
    var showVendor = true
    var insights: UsageInsights?
    var tokensPerHour: Double?
    var forecastHint: String?
    var theme: Theme = .island
    var isLoading = false
    @State private var isHovered = false

    var body: some View {
        if let forecastHint {
            content
                .background(IslandHoverPopover(content: QuotaForecastDetails(agent: row.agent, hint: forecastHint),
                                               isHovered: $isHovered))
                .accessibilityElement(children: .combine)
                .accessibilityHint(forecastHint)
                .onDisappear { isHovered = false }
        } else {
            content
        }
    }

    private var content: some View {
        let level = row.level ?? .ok
        let color = theme.status(level)
        let value = metricValue
        let metricColor = switch metric {
        case .quota: color
        case .burnRate: exhaustsBeforeReset == nil ? theme.status(.ok) : theme.status(.warning)
        case .tokens: theme.status(.ok)
        }
        let valueColor = row.level == nil ? theme.secondary : metricColor
        let markerColor = row.level == nil || value == nil ? theme.tertiary : metricColor
        return HStack(spacing: IslandRowLayout.spacing) {
            Circle()
                .fill(markerColor)
                .frame(width: IslandRowLayout.markerWidth, height: IslandRowLayout.markerWidth)
                .shadow(color: row.level == nil || value == nil ? .clear : markerColor, radius: 4)
            nameText
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(width: IslandRowLayout.nameWidth, alignment: .leading)
            ProgressTrack(fraction: (row.usedPct ?? 0) / 100,
                          projectedFraction: metric == .burnRate ? projectedUsedPct / 100 : nil,
                          fill: isLoading || row.level == nil ? theme.secondary : color,
                          projectionFill: exhaustsBeforeReset == nil ? theme.secondary : theme.status(.warning),
                          track: theme.track, isLoading: isLoading)
                .frame(height: 4)
            Text(value ?? "—")
                .font(.tabular(13, .semibold))
                .foregroundStyle(value == nil ? theme.secondary : valueColor)
                .frame(width: 62, alignment: .trailing)
                .contentTransition(.numericText())
            if showsDetail {
                Text(metricDetail)
                    .font(.tabular(12))
                    .foregroundStyle(metric == .burnRate && exhaustsBeforeReset != nil ? theme.status(.warning) : theme.secondary)
                    .lineLimit(1)
                    .frame(width: 96, alignment: .trailing)
                    .contentTransition(.numericText())
            }
        }
        .font(.ui(13))
        .padding(.vertical, 4)
        .padding(.horizontal, IslandRowLayout.inset)
        .background(RoundedRectangle(cornerRadius: 6).fill(theme.text.opacity(isHovered ? 0.06 : 0)))
        .contentShape(Rectangle())
        .animation(.easeOut(duration: 0.12), value: isHovered)
    }

    private var metricValue: String? {
        switch metric {
        case .quota:
            row.usedPct.map(TokenFormat.percent)
        case .burnRate:
            insights?.burnRatePctPerHour.map { String(format: "%.1f%%/h", $0) }
        case .tokens:
            tokensPerHour.map { TokenFormat.short(Int($0.rounded())) + "/h" }
        }
    }

    private var showsDetail: Bool { metric != .quota || showReset }

    private var metricDetail: String {
        guard row.usedPct != nil else { return isLoading ? "—" : row.missingQuotaLabel }
        switch metric {
        case .quota:
            return row.resetLabel(now: now)
        case .burnRate:
            if row.usedPct == 100 { return L10n.text("已耗尽", "Exhausted") }
            if exhaustsBeforeReset != nil { return forecastHint ?? L10n.text("预计耗尽", "May run out") }
            if let projected = projectedAtReset {
                return L10n.text("重置时 \(Int(projected.rounded()))%", "\(Int(projected.rounded()))% by reset")
            }
            if insights?.burnRatePctPerHour == 0 { return L10n.text("暂无消耗", "No usage") }
            return L10n.text("记录不足", "Insufficient data")
        case .tokens:
            return tokensPerHour.map { TokenFormat.short(Int(($0 * 24).rounded())) + L10n.text(" / 天", " / day") } ?? "—"
        }
    }

    /// Time at which this pace consumes the rest, only when that happens before the provider resets the window.
    private var exhaustsBeforeReset: Date? {
        guard let interval = insights?.timeToExhaust, interval > 0, interval.isFinite else { return nil }
        let date = now.addingTimeInterval(interval)
        return row.resetAt.map { date < $0 ? date : nil } ?? date
    }

    /// Used by the reset at the existing burn rate; the UI does not invent a second forecast.
    private var projectedAtReset: Double? {
        guard let used = row.usedPct, let rate = insights?.burnRatePctPerHour,
              let reset = row.resetAt, reset > now else { return nil }
        return min(100, used + rate * reset.timeIntervalSince(now) / 3600)
    }

    private var projectedUsedPct: Double {
        guard metric == .burnRate else { return row.usedPct ?? 0 }
        return exhaustsBeforeReset == nil ? projectedAtReset ?? row.usedPct ?? 0 : 100
    }

    private var nameText: Text {
        let label = L10n.modelLabel(row.agent.model)
        if showVendor {
            return Text(row.agent.displayVendor).fontWeight(.semibold) + Text(" · \(label)").foregroundColor(theme.secondary)
        }
        return Text(label).fontWeight(.semibold)
    }
}

struct ProgressTrack: View {
    var fraction: Double
    var projectedFraction: Double? = nil
    let fill: Color
    var projectionFill: Color? = nil
    let track: Color
    var isLoading = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(track)
                if isLoading {
                    if reduceMotion {
                        Capsule().fill(fill.opacity(0.3))
                    } else {
                        Capsule().fill(fill)
                            .phaseAnimator([false, true]) { content, bright in
                                content.opacity(bright ? 0.45 : 0.12)
                            } animation: { _ in
                                .easeInOut(duration: 1)
                            }
                    }
                } else {
                    if let projectedFraction, projectedFraction > fraction {
                        Capsule()
                            .fill((projectionFill ?? fill).opacity(0.38))
                            .frame(width: max(0, min(1, projectedFraction)) * proxy.size.width)
                    }
                    Capsule().fill(fill).frame(width: max(0, min(1, fraction)) * proxy.size.width)
                }
            }
        }
    }
}
