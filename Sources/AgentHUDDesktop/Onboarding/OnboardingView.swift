import AppKit
import SwiftUI
import AgentHUDCore

/// Single-screen first launch: auto-detected sources, no choices, no sign-in.
struct OnboardingView: View {
    let settings: SettingsStore
    let store: UsageStore
    let sources: [SourceStatus]
    let onFinish: () -> Void
    @Environment(\.colorScheme) private var scheme

    /// Once the first poll has answered, the Claude Code row shows the real plan ("已就绪 · Max").
    private var resolvedSources: [SourceStatus] {
        SourceDetector.resolve(sources, report: store.report)
    }

    var body: some View {
        let theme = Theme.forScheme(scheme)
        VStack(alignment: .leading, spacing: 16) {
            // Native traffic lights occupy this strip.
            Color.clear.frame(height: 12)
            ZStack(alignment: .top) {
                LinearGradient(colors: [Color(hex: 0x5b6b8c), Color(hex: 0x7c8aa6)], startPoint: .top, endPoint: .bottom)
                GlowPreview(
                    appearance: store.glowAppearance(light: false),
                    settings: settings.settings,
                    islandSize: CGSize(width: 190, height: 26),
                    islandRadius: 13,
                    scale: 0.9
                )
            }
            .frame(height: 96)
            .clipShape(RoundedRectangle(cornerRadius: 10))

            VStack(alignment: .leading, spacing: 6) {
                Text(L10n.text("灵动岛已就绪", "Your notch is ready")).font(.ui(20, .semibold))
                Text(L10n.text("在灵动岛查看额度和会话，随时掌握用量。", "Keep track of your usage and sessions from the notch."))
                    .font(.ui(13))
                    .foregroundStyle(theme.secondary)
            }

            VStack(spacing: 6) {
                ForEach(resolvedSources) { source in
                    SourceRow(source: source, theme: theme)
                }
            }

            HStack {
                Text(L10n.text("登录时自动启动 · ", "Launch at login · ") + (settings.settings.launchAtLogin ? L10n.text("已开启", "on") : L10n.text("已关闭", "off")))
                    .font(.ui(12))
                    .foregroundStyle(theme.secondary)
                Spacer()
                Button(L10n.text("开始使用", "Get started"), action: onFinish)
                    .buttonStyle(PrimaryButtonStyle())
            }
        }
        .padding(EdgeInsets(top: 20, leading: 26, bottom: 22, trailing: 26))
        .frame(width: 440)
        .background(theme.windowBackground)
        .foregroundStyle(theme.text)
        .font(.ui(13))
        .id(settings.settings.language)
    }
}

struct SourceRow: View {
    let source: SourceStatus
    let theme: Theme

    var body: some View {
        HStack(spacing: 12) {
            AgentLogo(vendor: source.id == "chatgpt" ? "ChatGPT" : source.name, size: 20)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Text(source.name).font(.ui(13, .semibold))
                    if let plan = source.planLabel {
                        SubscriptionBadge(plan: plan, theme: theme)
                    }
                }
            }
            Spacer()
        }
        .padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
        .background(RoundedRectangle(cornerRadius: 8).fill(theme.rowBackground))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(theme.rowBorder, lineWidth: 1))
        .foregroundStyle(theme.text)
    }
}

@MainActor
final class OnboardingWindowController: HostedWindowController {
    var onFinish: (() -> Void)?
    private let settings: SettingsStore
    private let store: UsageStore
    private let sources: () -> [SourceStatus]

    init(settings: SettingsStore, store: UsageStore, sources: @escaping () -> [SourceStatus]) {
        self.settings = settings
        self.store = store
        self.sources = sources
        super.init(size: CGSize(width: 440, height: 560), title: AppResources.applicationName, fitToContent: true, content: Color.clear)
    }

    /// Detection runs each time the window is shown so the list reflects the machine right now.
    override func show() {
        let view = OnboardingView(settings: settings, store: store, sources: sources()) { [weak self] in
            self?.onFinish?()
            self?.window?.close()
        }
        setContent(view, fitToContent: true)
        super.show()
    }
}
