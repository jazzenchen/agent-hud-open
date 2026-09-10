import SwiftUI
import AgentHUDCore

/// The camera stays empty; event information lives in the two wings beside it.
struct QuotaAlertCompactView: View {
    let alert: QuotaAlert
    let cameraWidth: CGFloat
    let height: CGFloat
    let onOpen: () -> Void

    var body: some View {
        let copy = QuotaAlertCopy(alert: alert)
        Button(action: onOpen) {
            HStack(spacing: 0) {
                HStack(spacing: 8) {
                    AgentLogo(vendor: alert.agent.vendor, size: 17)
                    Text(alert.agent.displayVendor)
                        .font(.ui(13, .semibold)).foregroundStyle(.white).lineLimit(1)
                }
                .frame(width: NotchController.alertWingWidth, alignment: .leading)
                Color.clear.frame(width: cameraWidth)
                HStack(spacing: 7) {
                    QuotaEventSymbol(alert: alert, size: 12)
                    Text(copy.compactTitle)
                        .font(.tabular(12, .medium)).foregroundStyle(.white.opacity(0.92)).lineLimit(1)
                }
                .frame(width: NotchController.alertWingWidth, alignment: .trailing)
            }
            .padding(.horizontal, NotchController.alertSidePadding)
            .frame(height: height)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-alert-\(alert.kind.rawValue)")
        .accessibilityLabel(copy.accessibilityLabel)
        .help(L10n.text("悬停查看详情，点击打开统计", "Hover for details; click for statistics"))
    }
}

/// Hovering a brief event reveals a small detail surface on the island's black background.
struct QuotaAlertDetailView: View {
    let alert: QuotaAlert
    let onOpen: () -> Void

    var body: some View {
        let copy = QuotaAlertCopy(alert: alert)
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 9) {
                AgentLogo(vendor: alert.agent.vendor, size: 22)
                VStack(alignment: .leading, spacing: 3) {
                    Text(alert.agent.displayVendor).font(.ui(13, .semibold)).foregroundStyle(.white)
                    Text(L10n.modelLabel(alert.agent.model)).font(.ui(10)).foregroundStyle(.white.opacity(0.42))
                }
                Spacer(minLength: 8)
                HStack(spacing: 5) {
                    Circle().fill(copy.accent).frame(width: 5, height: 5)
                    Text(alert.kind == .reset ? L10n.text("已重置", "reset") : L10n.text("额度预警", "running low"))
                        .font(.ui(10, .medium)).foregroundStyle(copy.accent)
                }
            }
            Text(copy.title).font(.ui(12)).foregroundStyle(.white.opacity(0.58))
                .fixedSize(horizontal: false, vertical: true)
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                metric(copy.used, label: L10n.text("已用额度", "used"))
                metric(alert.kind == .reset ? copy.remaining : copy.exhaustion,
                       label: alert.kind == .reset ? L10n.text("剩余额度", "remaining") : L10n.text("预计耗尽", "until empty"))
                metric(copy.reset, label: L10n.text("下次重置", "next reset"))
            }
            VStack(alignment: .leading, spacing: 7) {
                HStack {
                    Text(L10n.modelLabel(alert.agent.model))
                    Spacer()
                    Text(copy.used)
                }
                .font(.ui(10)).foregroundStyle(.white.opacity(0.42))
                ProgressTrack(fraction: max(0, 100 - alert.snapshot.remainingPct) / 100,
                              fill: copy.accent, track: .white.opacity(0.12)).frame(height: 4)
                if !alert.otherExhaustedWindows.isEmpty {
                    Text(copy.otherLimits).font(.ui(10)).foregroundStyle(copy.accent)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Button(action: onOpen) {
                HStack(spacing: 6) {
                    Text(L10n.text("查看用量统计", "View usage statistics"))
                    Image(systemName: "arrow.up.right").font(.system(size: 9, weight: .semibold))
                }
                .font(.ui(12, .semibold)).foregroundStyle(.black.opacity(0.9))
                .frame(maxWidth: .infinity).frame(height: 30)
                .background(copy.accent, in: RoundedRectangle(cornerRadius: 7))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("island-alert-details-open")
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(copy.accessibilityLabel)
    }

    private func metric(_ value: String, label: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(value).font(.tabular(23, .medium)).foregroundStyle(.white)
                .lineLimit(1).minimumScaleFactor(0.75)
            Text(label).font(.ui(10)).foregroundStyle(.white.opacity(0.4))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// An event in the normal panel adds a quiet line, not a nested notification card.
struct QuotaAlertView: View {
    let alert: QuotaAlert
    let onOpen: () -> Void

    var body: some View {
        let copy = QuotaAlertCopy(alert: alert)
        Button(action: onOpen) {
            HStack(spacing: 10) {
                QuotaEventSymbol(alert: alert, size: 13)
                VStack(alignment: .leading, spacing: 4) {
                    Text(copy.title).font(.ui(12, .medium)).foregroundStyle(.white.opacity(0.95))
                    Text(alert.agent.displayName).font(.ui(10)).foregroundStyle(.white.opacity(0.45))
                }
                Spacer(minLength: 8)
                Image(systemName: "arrow.up.right").font(.ui(10)).foregroundStyle(.white.opacity(0.4))
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
    @State private var animate = false
    @State private var resetComplete = false

    var body: some View {
        Image(systemName: alert.kind == .reset ? (resetComplete ? "checkmark" : "arrow.clockwise") : "flame.fill")
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(QuotaAlertCopy(alert: alert).accent)
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
    var accent: Color { Color(hex: alert.kind == .reset ? 0x6cd8ac : 0xe9a16d) }
    var used: String { TokenFormat.percent(max(0, 100 - alert.snapshot.remainingPct)) }
    var remaining: String { TokenFormat.percent(alert.snapshot.remainingPct) }
    var exhaustion: String {
        guard let duration = alert.timeToExhaust else { return "—" }
        return Countdown.compact(max(60, ceil(duration / 60) * 60))
    }
    var reset: String { Countdown.resetLabelCompact(alert.snapshot.resetAt, now: alert.snapshot.updatedAt) }
    var compactTitle: String {
        if alert.kind == .reset { return L10n.text("已重置", "Reset") }
        if alert.snapshot.remainingPct <= 0 { return L10n.text("已耗尽", "Empty") }
        if alert.timeToExhaust != nil { return L10n.text("\(exhaustion) 后耗尽", "\(exhaustion) left") }
        return L10n.text("即将耗尽", "Running low")
    }
    var title: String {
        if alert.kind == .reset {
            return alert.otherExhaustedWindows.isEmpty
                ? L10n.text("此窗口额度已重置。", "This quota window has reset.")
                : L10n.text("此窗口已重置，其他窗口仍有限制。", "This window has reset; other limits still apply.")
        }
        if alert.snapshot.remainingPct <= 0 { return L10n.text("当前窗口额度已耗尽，等待重置。", "This window is empty. Waiting for reset.") }
        if alert.timeToExhaust != nil {
            return L10n.text("按本周期的平均消耗速率，预计将在重置前耗尽。", "At this cycle's average rate, this window may run out before it resets.")
        }
        return L10n.text("已用额度达到危险阈值，请留意后续任务。", "Usage has reached the critical threshold.")
    }
    var otherLimits: String {
        let names = alert.otherExhaustedWindows.map(L10n.modelLabel).joined(separator: "、")
        return L10n.text("\(names)额度仍已耗尽", "Still exhausted: \(names)")
    }
    var accessibilityLabel: String {
        alert.agent.displayName + " · " + compactTitle + (alert.isPreview ? L10n.text(" · 动画测试", " · Animation test") : "")
    }
}
