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

/// User-selected quota rows, token usage and active sessions.
struct HoverPanelView: View {
    let store: UsageStore
    let onOpenStats: () -> Void
    var onOpenSettings: () -> Void = {}
    var alert: IslandAlert? = nil
    var onOpenAlert: () -> Void = {}

    private let theme = Theme.island

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let alert {
                IslandAlertInlineView(alert: alert, onOpen: onOpenAlert).id(alert.id)
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
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        AgentLogo(vendor: group.vendor, size: 14)
                            .frame(width: IslandRowLayout.inset * 2 + IslandRowLayout.markerWidth)
                        Text(L10n.vendorLabel(group.vendor))
                            .font(IslandRowLayout.headingFont)
                            .foregroundStyle(theme.text)
                        if store.isLoading {
                            LoadingSpinner(color: theme.secondary)
                                .accessibilityLabel(L10n.text("正在读取额度", "Loading quota"))
                        }
                        Spacer()
                        if store.settings.settings.showResetCountdown {
                            Text(L10n.text("重置", "Resets"))
                                .padding(.trailing, 6)
                        }
                    }
                    .font(.ui(11))
                    .foregroundStyle(theme.secondary)
                    .padding(.vertical, IslandRowLayout.headingVerticalPadding)
                    ForEach(group.rows) { row in
                        ModelUsageRow(row: row, now: store.now, showReset: store.settings.settings.showResetCountdown,
                                      showVendor: false, forecastHint: store.quotaForecastHint(for: row.id), isLoading: store.isLoading)
                    }
                    if group.rows.contains(where: { $0.agent.vendor == "Codex" }), let resets = store.report?.codexResetCredits {
                        CodexResetCreditsView(resets: resets, showExpiry: store.settings.settings.showResetCountdown)
                    }
                }
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

    private var sessionLine: some View {
        Button(action: onOpenStats) {
            let sessions = store.statsSessions
            let liveCount = sessions.filter(\.isLive).count
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Circle().fill(liveCount > 0 ? theme.status(.ok) : theme.tertiary).frame(width: 6, height: 6)
                    Text(L10n.text("活跃会话", "Active sessions")).foregroundStyle(theme.text)
                    Spacer()
                    Text(L10n.text("\(sessions.count) 个会话 · \(liveCount) 个运行中", "\(sessions.count) sessions · \(liveCount) running"))
                        .foregroundStyle(theme.secondary)
                }
                if let first = sessions.first {
                    HStack(spacing: 8) {
                        Text("\(Self.shortTask(first.task)) · \(first.terminal ?? "—")")
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 8)
                        Text("\(TokenFormat.short(first.tokensIn + first.tokensOut)) tok")
                            .fixedSize()
                    }
                    .foregroundStyle(theme.secondary)
                } else {
                    Text(L10n.text("此时间窗口内没有活跃会话", "No active sessions in this range"))
                        .foregroundStyle(theme.secondary)
                }
            }
            .font(.ui(12))
            .padding(.top, 8)
            .topDivider(theme.divider)
        }
        .buttonStyle(.plain)
        .help(L10n.text("查看会话列表", "Show sessions"))
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
    static func shortTask(_ task: String) -> String {
        let words = task.split(separator: " ")
        if words.count > 3 { return words.prefix(3).joined(separator: " ") }
        return String(task.prefix(14))
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

/// Aligned quota labels and reset times with a flexible progress bar between them.
struct ModelUsageRow: View {
    let row: AgentRow
    let now: Date
    let showReset: Bool
    var showVendor = true
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
        return HStack(spacing: IslandRowLayout.spacing) {
            Circle()
                .fill(row.level == nil ? theme.tertiary : color)
                .frame(width: IslandRowLayout.markerWidth, height: IslandRowLayout.markerWidth)
                .shadow(color: row.level == nil ? .clear : color, radius: 4)
            nameText
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(width: IslandRowLayout.nameWidth, alignment: .leading)
            ProgressTrack(fraction: (row.usedPct ?? 0) / 100, fill: isLoading ? theme.secondary : color,
                          track: theme.track, isLoading: isLoading)
                .frame(height: 4)
            Text(row.usedPct.map(TokenFormat.percent) ?? "—")
                .font(.tabular(13, .semibold))
                .foregroundStyle(row.level == nil ? theme.secondary : color)
                .frame(width: 44, alignment: .trailing)
            if showReset {
                Text(row.usedPct == nil
                     ? (isLoading ? "—" : row.missingQuotaLabel)
                     : Countdown.resetLabel(row.resetAt, now: now))
                    .font(.tabular(12))
                    .foregroundStyle(theme.secondary)
                    .lineLimit(1)
                    .frame(width: 96, alignment: .trailing)
            }
        }
        .font(.ui(13))
        .padding(.vertical, 4)
        .padding(.horizontal, IslandRowLayout.inset)
        .background(RoundedRectangle(cornerRadius: 6).fill(theme.text.opacity(isHovered ? 0.06 : 0)))
        .contentShape(Rectangle())
        .animation(.easeOut(duration: 0.12), value: isHovered)
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
    let fraction: Double
    let fill: Color
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
                    Capsule().fill(fill).frame(width: max(0, min(1, fraction)) * proxy.size.width)
                }
            }
        }
    }
}
