import AppKit
import SwiftUI
import AgentHUDCore

/// Usage statistics that fill the available window width, on two pages: token charts, and sessions.
struct StatsView: View {
    let store: UsageStore
    var scrollable = true
    var onIdealHeightChange: ((CGFloat) -> Void)?
    @Environment(\.colorScheme) private var scheme
    @State private var sessionSource: SessionSource?
    /// How the Sessions page lists its sessions: under their days, or those active in the last day without days.
    @State private var arrangement = SessionArrangement.day

    var body: some View {
        let theme = Theme.forScheme(scheme)
        VStack(spacing: 0) {
            header(theme)
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: StatsIdealHeightKey.self, value: proxy.size.height)
                })
            if scrollable {
                ScrollViewReader { proxy in
                    ScrollView { content(theme) }
                        .onChange(of: store.selectedQuotaId, initial: true) { _, id in
                            guard let id, let vendor = store.tokenCardVendors(for: id).first else { return }
                            withAnimation { proxy.scrollTo(AgentCards.anchor(vendor), anchor: .center) }
                            // The tile is pointed out for a moment; it keeps showing the window afterwards.
                            Task {
                                try? await Task.sleep(for: .seconds(2.5))
                                if store.selectedQuotaId == id { withAnimation { store.selectedQuotaId = nil } }
                            }
                        }
                        .onChange(of: store.focusedSessionID) { _, _ in proxy.scrollTo(Self.top, anchor: .top) }
                        // A quota window being pointed out scrolls to its tile instead.
                        .onChange(of: store.statsTab) { _, _ in
                            if store.selectedQuotaId == nil { proxy.scrollTo(Self.top, anchor: .top) }
                        }
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

    private func content(_ theme: Theme) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Color.clear.frame(height: 0).id(Self.top)
            if let error = store.lastError {
                Text(L10n.text("刷新失败：", "Refresh failed: ") + error)
                    .font(.ui(12)).foregroundStyle(theme.secondary)
                    .textSelection(.enabled)
            }
            switch store.statsTab {
            case .tokens:
                tokens(theme)
            case .sessions:
                if let session = store.focusedSession {
                    SessionDetailView(session: session, store: store, theme: theme)
                } else {
                    SessionList(store: store, theme: theme, source: sessionSource, activeOnly: arrangement == .active)
                }
            }
        }
        .padding(EdgeInsets(top: 8, leading: 22, bottom: 12, trailing: 22))
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GeometryReader { proxy in
            Color.clear.preference(key: StatsIdealHeightKey.self, value: proxy.size.height)
        })
    }

    @ViewBuilder
    private func tokens(_ theme: Theme) -> some View {
        UsageChartsCard(store: store, theme: theme)
        AgentCards(store: store, theme: theme)
        UsagePeriodsCard(store: store, theme: theme)
        HeatmapCard(grid: store.statsActivity, consumers: store.consumers, theme: theme)
    }

    /// The pages sit in the title bar, centred on the window and level with the traffic lights; the row below holds the
    /// controls of the page on screen.
    private func header(_ theme: Theme) -> some View {
        VStack(spacing: 12) {
            Picker(L10n.text("页面", "Page"), selection: Binding(get: { store.statsTab }, set: { store.statsTab = $0 })) {
                ForEach([StatsTab.tokens, .sessions], id: \.self) { Text($0.label).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            .frame(maxWidth: .infinity)
            .accessibilityIdentifier("stats-tab")
            controls(theme).frame(height: 28)
        }
        .padding(EdgeInsets(top: 4, leading: 22, bottom: 10, trailing: 22))
        .background(theme.windowBackground)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.divider).frame(height: 1) }
    }

    private func controls(_ theme: Theme) -> some View {
        HStack(spacing: 12) {
            switch store.statsTab {
            case .tokens:
                dimensions
                Spacer(minLength: 12)
                if store.statsRange.bucketSizes.count > 1 {
                    SegmentedPills(
                        options: store.statsRange.bucketSizes.map { SegmentOption(value: $0, label: $0.label) },
                        selection: Binding(get: { store.tokenBucketSize }, set: { store.tokenBucketSize = $0 }),
                        theme: theme
                    )
                }
                SegmentedPills(
                    options: StatsRange.allCases.map { SegmentOption(value: $0, label: $0.label) },
                    selection: Binding(get: { store.statsRange }, set: { store.setStatsRange($0) }),
                    theme: theme
                )
            case .sessions where store.focusedSession != nil:
                Button {
                    store.focusedSessionID = nil
                } label: {
                    Label(L10n.text("全部会话", "All sessions"), systemImage: "chevron.left")
                }
                .buttonStyle(.plain)
                .font(.ui(12, .semibold))
                .keyboardShortcut("[", modifiers: .command)
                .help(L10n.text("回到会话列表", "Back to the session list"))
                Spacer(minLength: 12)
            case .sessions:
                Picker(L10n.text("排列", "Arrange"), selection: $arrangement.animation(.easeOut(duration: 0.15))) {
                    Text(L10n.text("按日", "Day")).tag(SessionArrangement.day)
                    Text(L10n.text("活跃", "Active")).tag(SessionArrangement.active)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .controlSize(.small)
                .fixedSize()
                .help(L10n.text("按日期列出近 7 天的会话，或只列近 24 小时有活动的会话、不分日期",
                                "The last seven days' sessions under their days, or only those active in the last 24 hours, without days"))
                .accessibilityIdentifier("sessions-arrangement")
                Spacer(minLength: 12)
                sessionCount(theme)
                SelectionMenu(
                    title: L10n.text("会话来源", "Session source"),
                    options: [SegmentOption(value: Optional<SessionSource>.none, label: L10n.text("全部来源", "All sources"))]
                        + sessionSources.map { SegmentOption(value: Optional($0), label: $0.name) },
                    selection: $sessionSource,
                    theme: theme,
                    width: 185
                )
                .accessibilityIdentifier("session-source-filter")
            }
        }
    }

    private var sessionSources: [SessionSource] {
        Array(Set(store.statsSessions.map(store.sessionSource))).sorted {
            if $0.vendor != $1.vendor { return ($0.vendor ?? "") < ($1.vendor ?? "") }
            return $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }
    }

    private func sessionCount(_ theme: Theme) -> some View {
        let activeOnly = arrangement == .active
        let counts = Self.sessionCounts(store, source: sessionSource, activeOnly: activeOnly)
        let listed = counts.listed, running = counts.running
        return HStack(spacing: 6) {
            Circle().fill(running > 0 ? theme.status(.ok) : theme.tertiary).frame(width: 6, height: 6)
            Text(activeOnly ? L10n.text("近 24 小时 \(listed) 个 · \(running) 个运行中", "\(listed) in 24 hours · \(running) running")
                            : L10n.text("近 7 天 \(listed) 个 · \(running) 个运行中", "\(listed) in 7 days · \(running) running"))
        }
        .font(.ui(11))
        .foregroundStyle(theme.secondary)
    }

    /// How many sessions the list shows, and how many of them are running.
    static func sessionCounts(_ store: UsageStore, source: SessionSource?, activeOnly: Bool) -> (listed: Int, running: Int) {
        let sessions = store.listedSessions(source: source, activeOnly: activeOnly), view = store.view
        return (sessions.count, sessions.filter { view.phase(of: $0).isInFlight }.count)
    }

    /// Which token kinds the charts count: what calls added, everything, or any kinds picked one by one.
    private var dimensions: some View {
        Menu {
            Toggle(L10n.text("不含缓存读取", "Excluding cache reads"), isOn: Binding(
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

private extension StatsTab {
    /// The page's name in the window's picker.
    var label: String {
        switch self {
        case .tokens: L10n.text("Token", "Tokens")
        case .sessions: L10n.text("会话", "Sessions")
        }
    }
}
