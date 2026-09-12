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
                ScrollView { content(theme) }
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

    private func content(_ theme: Theme) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            if let error = store.lastError {
                Text(L10n.text("刷新失败：", "Refresh failed: ") + error)
                    .font(.ui(12)).foregroundStyle(theme.secondary)
                    .textSelection(.enabled)
            }
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
        .padding(EdgeInsets(top: 8, leading: 22, bottom: 12, trailing: 22))
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GeometryReader { proxy in
            Color.clear.preference(key: StatsIdealHeightKey.self, value: proxy.size.height)
        })
    }

    private func controls(_ theme: Theme) -> some View {
        HStack(spacing: 12) {
            // Leave room for the native traffic lights.
            Color.clear.frame(width: 54, height: 12)
            HStack(spacing: 10) {
                ForEach(TokenDimensions.choices, id: \.value) { choice in
                    Toggle(choice.label, isOn: Binding(
                        get: { store.tokenDimensions.contains(choice.value) },
                        set: { if $0 { store.tokenDimensions.insert(choice.value) } else { store.tokenDimensions.remove(choice.value) } }
                    ))
                    .toggleStyle(.checkbox)
                    .controlSize(.small)
                    .accessibilityLabel(L10n.text("统计维度 \(choice.label)", "Token dimension \(choice.label)"))
                }
            }
            Spacer(minLength: 12)
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
        .padding(EdgeInsets(top: 10, leading: 22, bottom: 10, trailing: 22))
        .background(theme.windowBackground)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.divider).frame(height: 1) }
    }

}

private struct StatsIdealHeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

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
            resizable: true,
            content: StatsView(store: store)
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
