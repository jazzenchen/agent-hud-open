import AppKit
import SwiftUI
import AgentHUDCore

/// Usage statistics that fill the available window width.
struct StatsView: View {
    let store: UsageStore
    var scrollable = true
    var onIdealHeightChange: ((CGFloat) -> Void)?
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let theme = Theme.forScheme(scheme)
        VStack(spacing: 0) {
            controls(theme)
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: StatsIdealHeightKey.self, value: proxy.size.height)
                })
            if scrollable {
                ScrollViewReader { proxy in
                    ScrollView { content(theme) }
                        .onChange(of: store.selectedQuotaId, initial: true) { _, id in
                            guard let id, let vendor = quotaVendor(id) else { return }
                            withAnimation { proxy.scrollTo(MetricCards.anchor(vendor), anchor: .center) }
                            // The tile is pointed out for a moment; it keeps showing the window afterwards.
                            Task {
                                try? await Task.sleep(for: .seconds(2.5))
                                if store.selectedQuotaId == id { withAnimation { store.selectedQuotaId = nil } }
                            }
                        }
                        .onChange(of: store.focusedSessionID) { _, _ in proxy.scrollTo(Self.top, anchor: .top) }
                }
            } else {
                content(theme)
            }
        }
        .background(theme.windowBackground)
        .foregroundStyle(theme.text)
        .font(.ui(13))
        .id(store.settings.settings.language)
        .onPreferenceChange(StatsIdealHeightKey.self) { height in
            onIdealHeightChange?(height)
        }
    }

    private static let top = "stats-top"

    private func quotaVendor(_ id: String) -> String? {
        store.rowGroups.first { $0.rows.contains { $0.id == id } }?.vendor
    }

    private func content(_ theme: Theme) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Color.clear.frame(height: 0).id(Self.top)
            if let error = store.lastError {
                Text(L10n.text("刷新失败：", "Refresh failed: ") + error)
                    .font(.ui(12)).foregroundStyle(theme.secondary)
                    .textSelection(.enabled)
            }
            if let session = store.focusedSession {
                SessionDetailView(session: session, store: store, theme: theme)
            } else {
                overview(theme)
            }
        }
        .padding(EdgeInsets(top: 8, leading: 22, bottom: 12, trailing: 22))
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GeometryReader { proxy in
            Color.clear.preference(key: StatsIdealHeightKey.self, value: proxy.size.height)
        })
    }

    @ViewBuilder
    private func overview(_ theme: Theme) -> some View {
        UsageChartsCard(store: store, theme: theme)
        MetricCards(store: store, theme: theme)
        LiveSessionsCard(store: store, theme: theme)
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 12) {
                WeeklyTokenShareCard(store: store, theme: theme).frame(minWidth: 380)
                HeatmapCard(grid: store.statsActivity, consumers: store.consumers, theme: theme).frame(minWidth: 420)
            }
            .fixedSize(horizontal: false, vertical: true)
            VStack(spacing: 12) {
                WeeklyTokenShareCard(store: store, theme: theme)
                HeatmapCard(grid: store.statsActivity, consumers: store.consumers, theme: theme)
            }
        }
    }

    private func controls(_ theme: Theme) -> some View {
        HStack(spacing: 12) {
            // Leave room for the native traffic lights.
            Color.clear.frame(width: 54, height: 12)
            if store.focusedSession != nil {
                Button {
                    store.focusedSessionID = nil
                } label: {
                    Label(L10n.text("用量统计", "Usage statistics"), systemImage: "chevron.left")
                }
                .buttonStyle(.plain)
                .font(.ui(12, .semibold))
                .keyboardShortcut("[", modifiers: .command)
                .help(L10n.text("回到用量统计", "Back to usage statistics"))
            }
            // A session's page shows every kind and its own span, so the kinds, range and column width belong to the overview.
            if store.focusedSession == nil { dimensions }
            Spacer(minLength: 12)
            if store.focusedSession == nil {
                SegmentedPills(
                    options: TokenBucketSize.allCases.map { SegmentOption(value: $0, label: $0.label) },
                    selection: Binding(get: { store.tokenBucketSize }, set: { store.tokenBucketSize = $0 }),
                    theme: theme
                )
                SegmentedPills(
                    options: StatsRange.allCases.map { SegmentOption(value: $0, label: $0.label) },
                    selection: Binding(get: { store.statsRange }, set: { store.setStatsRange($0) }),
                    theme: theme
                )
            }
        }
        .padding(EdgeInsets(top: 10, leading: 22, bottom: 10, trailing: 22))
        .background(theme.windowBackground)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.divider).frame(height: 1) }
    }

    /// Which token kinds the charts count: what calls added, everything, or any kinds picked one by one.
    private var dimensions: some View {
        Menu {
            Toggle(L10n.text("新增 Token（不含缓存读取）", "New tokens (no cache reads)"), isOn: Binding(
                get: { store.tokenDimensions == .fresh }, set: { if $0 { store.tokenDimensions = .fresh } }))
            Toggle(L10n.text("全部 Token", "All tokens"), isOn: Binding(
                get: { store.tokenDimensions == .all }, set: { if $0 { store.tokenDimensions = .all } }))
            Divider()
            ForEach(TokenDimensions.choices, id: \.value) { choice in
                Toggle(choice.label, isOn: Binding(
                    get: { store.tokenDimensions.contains(choice.value) },
                    set: { selected in
                        if selected { store.tokenDimensions.insert(choice.value) }
                        // At least one kind stays counted.
                        else if store.tokenDimensions != choice.value { store.tokenDimensions.remove(choice.value) }
                    }))
            }
        } label: {
            Text("Token · " + store.tokenDimensions.label).font(.ui(12))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .accessibilityLabel(L10n.text("统计的 Token 种类", "Token kinds counted"))
    }

}

private struct StatsIdealHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value += nextValue()
    }
}

@MainActor
enum StatsWindowLayout {
    // User-selected portrait reference: height : width = 3 : 2.
    static let preferredSize = CGSize(width: 800, height: 1200)
    static let minimumWidth: CGFloat = 750
}

@MainActor
final class StatsWindowController: HostedWindowController {
    private var idealContentHeight: CGFloat = 0

    init(store: UsageStore) {
        let preferred = StatsWindowLayout.preferredSize
        let visible = NSScreen.main?.visibleFrame.height ?? preferred.height
        super.init(
            size: CGSize(width: preferred.width, height: min(preferred.height, visible - 24)),
            title: Self.title,
            resizable: true
        )
        window?.minSize = CGSize(width: StatsWindowLayout.minimumWidth, height: 480)
        setContent(StatsView(store: store, onIdealHeightChange: { [weak self] height in
            self?.idealContentHeight = height
            self?.fitHeightToContent()
        }))
    }

    private static var title: String { L10n.text("用量统计", "Usage statistics") }

    override func show() {
        window?.title = Self.title
        super.show()
        fitHeightToContent()
    }

    func windowDidEndLiveResize(_ notification: Notification) {
        fitHeightToContent()
    }

    func windowDidChangeScreen(_ notification: Notification) {
        fitHeightToContent()
    }

    func windowDidExitFullScreen(_ notification: Notification) {
        fitHeightToContent()
    }

    /// Keep the top edge stable and let the scroll view handle content taller than the screen.
    private func fitHeightToContent() {
        guard idealContentHeight > 0, let window,
              !window.inLiveResize, !window.styleMask.contains(.fullScreen),
              let screen = window.screen ?? NSScreen.main else { return }
        let available = screen.visibleFrame.insetBy(dx: 0, dy: 12)
        let chrome = window.frame.height - (window.contentView?.bounds.height ?? window.frame.height)
        let height = min(available.height, max(window.minSize.height, ceil(idealContentHeight + chrome)))
        var frame = window.frame
        frame.origin.y = max(available.minY, min(frame.maxY, available.maxY) - height)
        frame.size.height = height
        guard abs(frame.height - window.frame.height) > 1 || abs(frame.minY - window.frame.minY) > 1 else { return }
        window.setFrame(frame, display: true)
    }
}
