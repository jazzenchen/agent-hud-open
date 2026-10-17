import SwiftUI
import AgentHUDCore

/// The camera stays empty; the account's client sits in one wing and how many resets it gained in the other.
struct ResetCreditAlertCompactView: View {
    let grant: ResetCreditGrant
    let cameraWidth: CGFloat
    let height: CGFloat
    let onOpen: () -> Void
    @Environment(\.stripScale) private var scale
    @Environment(\.compactWings) private var wings
    @Environment(\.hudTheme) private var theme

    /// What the reminder says beside its client, for whoever has to know how wide that is.
    static func status(of grant: ResetCreditGrant) -> String { ResetCreditAlertCopy(grant: grant).compactTitle }

    /// A length at the scale of the dock this reminder grows from.
    private func s(_ value: CGFloat) -> CGFloat { (value * scale).rounded() }
    /// The size of its name or status, from their size beside the notch: on a dock, the size of every word a dock writes.
    private func w(_ value: CGFloat) -> CGFloat { s(CompactWings.words(value, onDock: wings != nil)) }
    /// The size of the symbol beside its status, from its size beside the notch: a size smaller on a dock.
    private func t(_ value: CGFloat) -> CGFloat { s(CompactWings.size(value, onDock: wings != nil)) }

    var body: some View {
        let copy = ResetCreditAlertCopy(grant: grant)
        Button(action: onOpen) {
            HStack(spacing: 0) {
                HStack(spacing: s(8)) {
                    ReminderMarkView(vendor: grant.account.account.provider, size: s(17), mark: .credit)
                    Text(grant.account.account.provider)
                        .font(.ui(w(13), .semibold)).foregroundStyle(theme.alertText).lineLimit(1)
                }
                .frame(width: wings?.leading ?? s(IslandController.alertWingWidth), alignment: .leading)
                Color.clear.frame(width: wings?.gap ?? cameraWidth)
                HStack(spacing: s(7)) {
                    ResetCreditSymbol(grant: grant, size: t(12))
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
        .accessibilityIdentifier("island-alert-resetCredits")
        .accessibilityLabel(copy.accessibilityLabel)
        .help(L10n.text("悬停查看详情，点击打开统计", "Hover for details; click for statistics"))
    }
}

/// Hovering the event shows what the account holds now and its earliest deadline.
struct ResetCreditAlertDetailView: View {
    let grant: ResetCreditGrant
    let onOpen: () -> Void
    @Environment(\.hudTheme) private var theme

    var body: some View {
        let copy = ResetCreditAlertCopy(grant: grant)
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 9) {
                ReminderMarkView(vendor: grant.account.account.provider, size: 22, mark: .credit)
                VStack(alignment: .leading, spacing: 3) {
                    Text(grant.account.account.provider).font(.ui(13, .semibold)).foregroundStyle(theme.alertText)
                    Text(grant.account.displayName).font(.ui(10)).foregroundStyle(theme.alertInk(0.42))
                        .lineLimit(1).truncationMode(.middle)
                }
                Spacer(minLength: 8)
                HStack(spacing: 5) {
                    Circle().fill(copy.accentFill(theme)).frame(width: 5, height: 5)
                    Text(L10n.text("新增重置", "new reset")).font(.ui(10, .medium)).foregroundStyle(copy.accentText(theme))
                }
            }
            Text(copy.title).font(.ui(12)).foregroundStyle(theme.alertInk(0.58))
                .fixedSize(horizontal: false, vertical: true)
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                metric("+\(grant.added)", label: L10n.text("新增", "added"))
                metric("\(grant.credits.availableCount)", label: L10n.text("可用次数", "available"))
                metric(copy.expiry, label: L10n.text("最早到期", "first expires"))
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
struct ResetCreditAlertInlineView: View {
    let grant: ResetCreditGrant
    let onOpen: () -> Void
    @Environment(\.hudTheme) private var theme

    var body: some View {
        let copy = ResetCreditAlertCopy(grant: grant)
        Button(action: onOpen) {
            HStack(spacing: 10) {
                ResetCreditSymbol(grant: grant, size: 13)
                VStack(alignment: .leading, spacing: 4) {
                    Text(copy.title).font(.ui(12, .medium)).foregroundStyle(theme.alertInk(0.95))
                    Text("\(grant.account.account.provider) · \(grant.account.displayName)")
                        .font(.ui(10)).foregroundStyle(theme.alertInk(0.45)).lineLimit(1).truncationMode(.middle)
                }
                Spacer(minLength: 8)
                Image(systemName: "arrow.up.right").font(.ui(10)).foregroundStyle(theme.alertInk(0.4))
            }
            .padding(.vertical, 8).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-alert-resetCredits")
        .accessibilityLabel(copy.accessibilityLabel)
    }
}

/// The panel's own mark for usage resets, turned once as the new one arrives.
private struct ResetCreditSymbol: View {
    let grant: ResetCreditGrant
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.hudTheme) private var theme
    @State private var animate = false

    var body: some View {
        Image(systemName: "arrow.counterclockwise")
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(theme.alertCalmText)
            .rotationEffect(.degrees(animate ? -360 : 0))
            .frame(width: size + 2, height: size + 2)
            .task(id: grant.id) {
                animate = false
                guard !reduceMotion else { return }
                do { try await Task.sleep(for: .milliseconds(160)) } catch { return }
                withAnimation(.easeInOut(duration: 0.8)) { animate = true }
            }
    }
}

private struct ResetCreditAlertCopy {
    let grant: ResetCreditGrant
    /// The accent as a fill, and as text and thin glyphs, on a HUD drawn with `theme`.
    func accentFill(_ theme: Theme) -> Color { theme.alertCalmFill }
    func accentText(_ theme: Theme) -> Color { theme.alertCalmText }
    var compactTitle: String {
        L10n.text("+\(grant.added) 次重置", grant.added == 1 ? "+1 reset" : "+\(grant.added) resets")
    }
    var title: String {
        L10n.text("此账户新增了 \(grant.added) 次额度重置。",
                  grant.added == 1 ? "A usage reset was added to this account." : "\(grant.added) usage resets were added to this account.")
    }
    /// The account's earliest deadline, as the usage panel names it.
    var expiry: String {
        guard let deadline = grant.credits.creditsByExpiry.compactMap(\.expirationDate).first else { return "—" }
        return deadline.formatted(Date.FormatStyle().month(.abbreviated).day()
            .locale(L10n.dateLocale))
    }
    var accessibilityLabel: String {
        grant.account.account.provider + " · " + title + (grant.isPreview ? L10n.text(" · 动画测试", " · Animation test") : "")
    }
}
