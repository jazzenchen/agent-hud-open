import SwiftUI
import AgentHUDCore

/// Aligned quota labels and the selected measure with a flexible progress bar between them.
struct ModelUsageRow: View {
    let row: AgentRow
    let now: Date
    var metric = IslandQuotaMetric.quota
    let showReset: Bool
    var showVendor = true
    var insights: UsageInsights?
    var outlook: QuotaOutlook?
    var tokensPerHour: Double?
    var forecastHint: String?
    var theme: Theme = .island
    var isLoading = false
    @State private var isHovered = false

    var body: some View {
        if let displayedForecastHint {
            content
                .background(IslandHoverPopover(content: QuotaForecastDetails(agent: row.agent, hint: displayedForecastHint),
                                               isHovered: $isHovered))
                .accessibilityElement(children: .combine)
                .accessibilityHint(displayedForecastHint)
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
        case .burnRate: runsOut ? theme.status(.warning) : theme.status(.ok)
        case .tokens: theme.status(.ok)
        }
        let valueColor = row.level == nil ? theme.secondary : metricColor
        let markerColor = row.level == nil || value == nil ? theme.tertiary : metricColor
        return HStack(spacing: IslandRowLayout.spacing) {
            Circle()
                .fill(markerColor)
                .frame(width: IslandRowLayout.markerWidth, height: IslandRowLayout.markerWidth)
                .shadow(color: row.level == nil || value == nil ? .clear : markerColor, radius: 4)
            // A name is never cut: one too long for the column gives way to the window's short name.
            ViewThatFits(in: .horizontal) {
                nameText(row.agent.name)
                nameText(row.agent.shortName)
            }
            .lineLimit(1)
            .frame(width: IslandRowLayout.nameWidth, alignment: .leading)
            .accessibilityLabel(showVendor ? row.agent.displayName : row.agent.name)
            ProgressTrack(fraction: (row.usedPct ?? 0) / 100,
                          projectedFraction: metric == .burnRate ? projectedUsedPct / 100 : nil,
                          fill: isLoading || row.level == nil ? theme.secondary : color,
                          projectionFill: runsOut ? theme.status(.warning) : theme.secondary,
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
                    .foregroundStyle(metric == .burnRate && runsOut ? theme.status(.warning) : theme.secondary)
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

    private var metrics: QuotaRowMetrics {
        QuotaRowMetrics(row: row, insights: insights, outlook: outlook, now: now, tokensPerHour: tokensPerHour)
    }

    private var metricValue: String? { metrics.value(metric) }

    private var showsDetail: Bool { metric != .quota || showReset }

    private var metricDetail: String { metrics.detail(metric, isLoading: isLoading) }

    private var runsOut: Bool { metrics.runsOut }

    private var exhaustionTimeLabel: String? { metrics.exhaustionTimeLabel }

    private var displayedForecastHint: String? {
        guard metric == .burnRate, let exhaustionTimeLabel else { return forecastHint }
        return L10n.text("预计耗尽：\(exhaustionTimeLabel)", "Exhausts \(exhaustionTimeLabel)")
    }

    private var projectedAtReset: Double? { metrics.projectedAtReset }

    private var projectedUsedPct: Double {
        guard metric == .burnRate else { return row.usedPct ?? 0 }
        return runsOut ? 100 : projectedAtReset ?? row.usedPct ?? 0
    }

    private func nameText(_ label: String) -> Text {
        if showVendor {
            return Text(row.agent.vendorName).fontWeight(.semibold) + Text(" · \(label)").foregroundColor(theme.secondary)
        }
        return Text(label).fontWeight(.semibold)
    }
}

/// What a quota row's measures read at `now`: the share used, the burn rate from its window's insights and the rate
/// tokens are spent at, each with the detail shown beside it. The burn rate's detail, colour and projection follow the
/// window's outlook as Core weighs it (`ReportView.outlook(for:)`), so the row and the hover hint agree. A row whose
/// reading shows no level has no burn rate, outlook, projection or token rate.
struct QuotaRowMetrics {
    let row: AgentRow
    let insights: UsageInsights?
    let outlook: QuotaOutlook?
    let now: Date
    let tokensPerHour: Double?

    init(row: AgentRow, insights: UsageInsights?, outlook: QuotaOutlook?, now: Date, tokensPerHour: Double?) {
        self.row = row
        self.insights = row.assessment.showsLevel ? insights : nil
        self.outlook = row.assessment.showsLevel ? outlook : nil
        self.now = now
        self.tokensPerHour = row.assessment.showsLevel ? tokensPerHour : nil
    }

    func value(_ metric: IslandQuotaMetric) -> String? {
        switch metric {
        case .quota:
            row.usedPct.map(TokenFormat.percent) ?? row.missingQuotaLabel
        case .burnRate:
            insights?.burnRatePctPerHour.map { String(format: "%.1f%%/h", $0) }
        case .tokens:
            tokensPerHour.map { TokenFormat.short(Int($0.rounded())) + "/h" }
        }
    }

    func detail(_ metric: IslandQuotaMetric, isLoading: Bool) -> String {
        guard row.usedPct != nil else {
            return metric == .quota && !isLoading ? row.resetLabel(now: now) : "—"
        }
        switch metric {
        case .quota:
            return row.resetLabel(now: now)
        case .burnRate:
            guard let outlook else { return "—" }
            // The row gives the time the rest runs out at, where the hint gives how long it lasts.
            if let exhaustionTimeLabel { return exhaustionTimeLabel }
            return QuotaForecast.text(of: outlook, projectedUsedAtReset: projectedAtReset) ?? "—"
        case .tokens:
            return tokensPerHour.map { TokenFormat.short(Int(($0 * 24).rounded())) + L10n.text(" / 天", " / day") } ?? "—"
        }
    }

    /// Time at which this pace consumes the rest, only when the outlook has that happen before the window resets.
    var exhaustsBeforeReset: Date? {
        guard case .exhausts(let interval, beforeReset: true) = outlook else { return nil }
        return now.addingTimeInterval(interval)
    }

    /// Whether the window has run out, or runs out before its reset at this pace: the burn rate then wears the warning
    /// colour and the projection reaches the end of the track.
    var runsOut: Bool { outlook == .exhausted || exhaustsBeforeReset != nil }

    var exhaustionTimeLabel: String? {
        guard let date = exhaustsBeforeReset else { return nil }
        if date.timeIntervalSince(now) < 7 * 86400 { return ChartData.weekdayTime(date) }
        return date.formatted(Date.FormatStyle().month(.abbreviated).day()
            .hour(.twoDigits(amPM: .omitted)).minute(.twoDigits)
            .locale(L10n.dateLocale))
    }

    /// Used by the reset at the existing burn rate, when the outlook has the rest last past the reset; the UI does not
    /// invent a second forecast.
    var projectedAtReset: Double? {
        guard case .exhausts(_, beforeReset: false) = outlook else { return nil }
        return row.usedPct.flatMap { QuotaMath.projectedUsedAtReset(usedPct: $0, insights: insights, resetAt: row.resetAt, now: now) }
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
