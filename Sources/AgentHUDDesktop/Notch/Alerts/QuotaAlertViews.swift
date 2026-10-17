import SwiftUI
import AgentHUDCore

/// The camera stays empty; event information lives in the two wings beside it.
struct QuotaAlertCompactView: View {
    let alert: QuotaAlert
    let cameraWidth: CGFloat
    let height: CGFloat
    let onOpen: () -> Void
    @Environment(\.stripScale) private var scale
    @Environment(\.compactWings) private var wings
    @Environment(\.hudTheme) private var theme

    /// What the reminder says beside its agent, for whoever has to know how wide that is.
    static func status(of alert: QuotaAlert) -> String { QuotaAlertCopy(alert: alert).compactTitle }

    /// A length at the scale of the dock this reminder grows from.
    private func s(_ value: CGFloat) -> CGFloat { (value * scale).rounded() }
    /// The size of its name or status, from their size beside the notch: on a dock, the size of every word a dock writes.
    private func w(_ value: CGFloat) -> CGFloat { s(CompactWings.words(value, onDock: wings != nil)) }
    /// The size of the symbol beside its status, from its size beside the notch: a size smaller on a dock.
    private func t(_ value: CGFloat) -> CGFloat { s(CompactWings.size(value, onDock: wings != nil)) }

    var body: some View {
        let copy = QuotaAlertCopy(alert: alert)
        Button(action: onOpen) {
            HStack(spacing: 0) {
                HStack(spacing: s(8)) {
                    ReminderMarkView(vendor: alert.agent.vendor, size: s(17), mark: .quota(alert))
                    Text(alert.agent.vendorName)
                        .font(.ui(w(13), .semibold)).foregroundStyle(theme.alertText).lineLimit(1)
                }
                .frame(width: wings?.leading ?? s(IslandController.alertWingWidth), alignment: .leading)
                Color.clear.frame(width: wings?.gap ?? cameraWidth)
                HStack(spacing: s(7)) {
                    QuotaEventSymbol(alert: alert, size: t(12))
                    Text(copy.compactTitle)
                        .font(.tabular(w(12), .medium)).foregroundStyle(theme.alertInk(0.92)).lineLimit(1)
                }
                .frame(width: wings?.trailing ?? s(IslandController.alertWingWidth), alignment: .trailing)
            }
            .padding(.horizontal, wings?.padding ?? s(IslandController.alertSidePadding))
            .frame(height: height)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-alert-\(alert.kind.rawValue)")
        .accessibilityLabel(copy.accessibilityLabel)
        .help(L10n.text("悬停查看详情，点击打开统计", "Hover for details; click for statistics"))
    }
}

/// Hovering a brief event reveals a small detail surface on the island's own background.
struct QuotaAlertDetailView: View {
    let alert: QuotaAlert
    let onOpen: () -> Void
    @Environment(\.hudTheme) private var theme

    var body: some View {
        let copy = QuotaAlertCopy(alert: alert)
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 9) {
                ReminderMarkView(vendor: alert.agent.vendor, size: 22, mark: .quota(alert))
                VStack(alignment: .leading, spacing: 3) {
                    Text(alert.agent.vendorName).font(.ui(13, .semibold)).foregroundStyle(theme.alertText)
                    Text(alert.agent.name).font(.ui(10)).foregroundStyle(theme.alertInk(0.42))
                }
                Spacer(minLength: 8)
                HStack(spacing: 5) {
                    Circle().fill(copy.accentFill(theme)).frame(width: 5, height: 5)
                    Text(alert.kind == .reset ? L10n.text("已重置", "reset") : L10n.text("额度预警", "running low"))
                        .font(.ui(10, .medium)).foregroundStyle(copy.accentText(theme))
                }
            }
            Text(copy.title).font(.ui(12)).foregroundStyle(theme.alertInk(0.58))
                .fixedSize(horizontal: false, vertical: true)
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                metric(copy.used, label: L10n.text("已用额度", "used"))
                metric(alert.kind == .reset ? copy.remaining : copy.exhaustion,
                       label: alert.kind == .reset ? L10n.text("剩余额度", "remaining") : L10n.text("预计耗尽", "until empty"))
                metric(copy.reset, label: L10n.text("下次重置", "next reset"))
            }
            VStack(alignment: .leading, spacing: 7) {
                HStack {
                    Text(alert.agent.name)
                    Spacer()
                    Text(copy.used)
                }
                .font(.ui(10)).foregroundStyle(theme.alertInk(0.42))
                ProgressTrack(fraction: (copy.usedPct ?? 0) / 100,
                              fill: copy.accentFill(theme), track: theme.track).frame(height: 4)
                if !alert.otherExhaustedWindows.isEmpty {
                    Text(copy.otherLimits).font(.ui(10)).foregroundStyle(copy.accentText(theme))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Button(action: onOpen) {
                HStack(spacing: 6) {
                    Text(L10n.text("查看用量统计", "View usage statistics"))
                    Image(systemName: "arrow.up.right").font(.system(size: 9, weight: .semibold))
                }
                .font(.ui(12, .semibold)).foregroundStyle(theme.onAlertFill.opacity(0.9))
                .frame(maxWidth: .infinity).frame(height: 30)
                .background(copy.accentFill(theme), in: RoundedRectangle(cornerRadius: 7))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("island-alert-details-open")
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(copy.accessibilityLabel)
    }

    private func metric(_ value: String, label: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(value).font(.tabular(23, .medium)).foregroundStyle(theme.alertText)
                .lineLimit(1).minimumScaleFactor(0.75)
            Text(label).font(.ui(10)).foregroundStyle(theme.alertInk(0.4))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// An event in the normal panel adds a quiet line, not a nested notification card.
struct QuotaAlertInlineView: View {
    let alert: QuotaAlert
    let onOpen: () -> Void
    @Environment(\.hudTheme) private var theme

    var body: some View {
        let copy = QuotaAlertCopy(alert: alert)
        Button(action: onOpen) {
            HStack(spacing: 10) {
                QuotaEventSymbol(alert: alert, size: 13)
                VStack(alignment: .leading, spacing: 4) {
                    Text(copy.title).font(.ui(12, .medium)).foregroundStyle(theme.alertInk(0.95))
                    Text(alert.agent.displayName).font(.ui(10)).foregroundStyle(theme.alertInk(0.45))
                }
                Spacer(minLength: 8)
                Image(systemName: "arrow.up.right").font(.ui(10)).foregroundStyle(theme.alertInk(0.4))
            }
            .padding(.vertical, 8).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-alert-\(alert.kind.rawValue)")
        .accessibilityLabel(copy.accessibilityLabel)
    }
}

private struct QuotaEventSymbol: View {
    let alert: QuotaAlert
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.hudTheme) private var theme
    @State private var animate = false
    @State private var resetComplete = false

    var body: some View {
        Image(systemName: alert.kind == .reset ? (resetComplete ? "checkmark" : "arrow.clockwise") : "flame.fill")
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(QuotaAlertCopy(alert: alert).accentText(theme))
            .rotationEffect(.degrees(alert.kind == .reset && animate && !resetComplete ? 360 : 0))
            .opacity(alert.kind == .exhaustion && animate ? 0.5 : 1)
            .frame(width: size + 2, height: size + 2)
            .contentTransition(.opacity)
            .task(id: alert.id) {
                animate = false
                resetComplete = reduceMotion && alert.kind == .reset
                guard !reduceMotion else { return }
                do { try await Task.sleep(for: .milliseconds(160)) } catch { return }
                withAnimation(alert.kind == .reset ? .easeInOut(duration: 0.8) : .easeInOut(duration: 0.3).repeatCount(4, autoreverses: true)) {
                    animate = true
                }
                if alert.kind == .reset {
                    do { try await Task.sleep(for: .milliseconds(850)) } catch { return }
                    withAnimation(.easeOut(duration: 0.18)) { resetComplete = true }
                }
            }
    }
}

private struct QuotaAlertCopy {
    let alert: QuotaAlert
    /// The accent as a fill, and as text and thin glyphs, on a HUD drawn with `theme`.
    func accentFill(_ theme: Theme) -> Color { IslandAlert.quota(alert).accentFill(theme) }
    func accentText(_ theme: Theme) -> Color { IslandAlert.quota(alert).accentText(theme) }
    var usedPct: Double? { alert.snapshot.remainingPct.map { max(0, 100 - $0) } }
    var used: String { usedPct.map(TokenFormat.percent) ?? "N/A" }
    var remaining: String { alert.snapshot.remainingPct.map(TokenFormat.percent) ?? "N/A" }
    var exhaustion: String {
        guard alert.snapshot.remainingPct != nil, let duration = alert.timeToExhaust else { return "—" }
        return Countdown.compact(max(60, ceil(duration / 60) * 60))
    }
    var reset: String { Countdown.resetLabelCompact(alert.snapshot.resetAt, now: alert.snapshot.updatedAt) }
    var compactTitle: String {
        guard let remaining = alert.snapshot.remainingPct else { return L10n.text("额度不可用", "Quota unavailable") }
        if alert.kind == .reset { return L10n.text("已重置", "Reset") }
        if remaining <= AlertPolicy.exhaustedRemaining { return L10n.text("已耗尽", "Empty") }
        if alert.timeToExhaust != nil { return L10n.text("\(exhaustion) 后耗尽", "\(exhaustion) left") }
        return L10n.text("即将耗尽", "Running low")
    }
    var title: String {
        guard let remaining = alert.snapshot.remainingPct else { return L10n.text("额度暂不可用。", "Quota is unavailable.") }
        if alert.kind == .reset {
            return alert.otherExhaustedWindows.isEmpty
                ? L10n.text("此窗口额度已重置。", "This quota window has reset.")
                : L10n.text("此窗口已重置，其他窗口仍有限制。", "This window has reset; other limits still apply.")
        }
        if remaining <= AlertPolicy.exhaustedRemaining {
            return L10n.text("当前窗口额度已耗尽，等待重置。", "This window is empty. Waiting for reset.")
        }
        if alert.timeToExhaust != nil {
            return L10n.text("按最近的消耗速率，预计将在重置前耗尽。", "At the recent burn rate, this window may run out before it resets.")
        }
        return L10n.text("已用额度达到危险阈值，请留意后续任务。", "Usage has reached the critical threshold.")
    }
    var otherLimits: String {
        let names = alert.otherExhaustedWindows.map(\.name).joined(separator: "、")
        return L10n.text("\(names)额度仍已耗尽", "Still exhausted: \(names)")
    }
    var accessibilityLabel: String {
        alert.agent.displayName + " · " + compactTitle + (alert.isPreview ? L10n.text(" · 动画测试", " · Animation test") : "")
    }
}
