import SwiftUI
import AgentHUDCore
import Observation

@MainActor
@Observable
final class SettingsNavigation {
    var pageID: String
    init(tab: SettingsTab = .display) { pageID = tab.id }
}

enum SettingsTab: String, CaseIterable, Identifiable {
    case general, sources, display

    var id: String { rawValue }
    var slug: String { rawValue }

    var label: String {
        switch self {
        case .general: return L10n.text("通用", "General")
        case .sources: return L10n.text("智能体", "Agents")
        case .display: return L10n.text("显示", "Display")
        }
    }

    var subtitle: String {
        switch self {
        case .general: return L10n.text("设置外观、语言和启动方式。", "Appearance, language and startup preferences.")
        case .sources: return L10n.text("管理实时状态、订阅与显示窗口。", "Manage live status, subscriptions and visible windows.")
        case .display: return L10n.text("自定义刘海光晕与面板内容，更改自动生效。", "Customize your notch and panel. Changes apply automatically.")
        }
    }

    var symbol: String {
        switch self {
        case .general: return "gearshape.fill"
        case .sources: return "puzzlepiece.extension.fill"
        case .display: return "display"
        }
    }

    var color: Color {
        switch self {
        case .general: return Color(hex: 0x8e8e93)
        case .sources: return Color(hex: 0x30b0c7)
        case .display: return Color(hex: 0x7c65e8)
        }
    }
}

enum SettingsWindowLayout {
    static let size = CGSize(width: 760, height: 720)
    static let minimum = CGSize(width: 680, height: 560)
    static let sidebarWidth: CGFloat = 212
}

struct SettingsView: View {
    let settings: SettingsStore
    let store: UsageStore
    var sourceStatuses: [SourceStatus]?
    var initiallyExpandedAgents: Set<String>
    let additionalPages: [DesktopSettingsPage]
    @State private var navigation: SettingsNavigation
    @Environment(\.colorScheme) private var scheme

    init(settings: SettingsStore, store: UsageStore, initialTab: SettingsTab = .display,
         navigation: SettingsNavigation? = nil,
         additionalPages: [DesktopSettingsPage] = [],
         sourceStatuses: [SourceStatus]? = nil, initiallyExpandedAgents: Set<String> = []) {
        self.settings = settings
        self.store = store
        self.sourceStatuses = sourceStatuses
        self.initiallyExpandedAgents = initiallyExpandedAgents
        self.additionalPages = additionalPages
        _navigation = State(initialValue: navigation ?? SettingsNavigation(tab: initialTab))
    }

    var body: some View {
        @Bindable var navigation = navigation
        let theme = Theme.forScheme(scheme)
        HStack(spacing: 0) {
            SettingsSidebar(selection: $navigation.pageID, additionalPages: additionalPages, theme: theme)
                .frame(width: SettingsWindowLayout.sidebarWidth)
                .layoutPriority(1)
            Rectangle().fill(theme.sidebarBorder).frame(width: 1)
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 7) {
                    Text(pageTitle).font(.ui(24, .semibold))
                    if !pageSubtitle.isEmpty {
                        Text(pageSubtitle).font(.ui(12)).foregroundStyle(theme.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 24)
                .padding(.bottom, 20)

                ScrollView {
                    pane(theme)
                        .frame(maxWidth: additionalPage?.preferredContentWidth ?? 640, alignment: .leading)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .padding(.horizontal, 20)
                        .padding(.bottom, 32)
                }
                .id(navigation.pageID)
            }
            .frame(minWidth: 0, maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .clipped()
        }
        .frame(minWidth: SettingsWindowLayout.minimum.width, idealWidth: SettingsWindowLayout.size.width,
               minHeight: SettingsWindowLayout.minimum.height, idealHeight: SettingsWindowLayout.size.height)
        .background(theme.windowBackground)
        .foregroundStyle(theme.text)
        .font(.ui(13))
        .tint(Color.accentColor)
        .id(settings.settings.language)
    }

    private var tab: SettingsTab? { SettingsTab(rawValue: navigation.pageID) }
    private var additionalPage: DesktopSettingsPage? { additionalPages.first { $0.id == navigation.pageID } }
    private var pageTitle: String { tab?.label ?? additionalPage?.heading?() ?? additionalPage?.title() ?? "" }
    private var pageSubtitle: String { tab?.subtitle ?? additionalPage?.subtitle() ?? "" }

    @ViewBuilder
    private func pane(_ theme: Theme) -> some View {
        if let tab {
            switch tab {
            case .general: GeneralPane(settings: settings, theme: theme)
            case .sources: SourcesPane(settings: settings, store: store, theme: theme, sources: sourceStatuses,
                                       initiallyExpanded: initiallyExpandedAgents)
            case .display: DisplayPane(settings: settings, store: store, theme: theme)
            }
        } else if let additionalPage {
            additionalPage.content()
        }
    }
}

struct SettingsSidebar: View {
    @Binding var selection: String
    let additionalPages: [DesktopSettingsPage]
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Color.clear.frame(height: 44)
            Text(L10n.text("偏好设置", "Preferences"))
                .font(.ui(11, .medium))
                .foregroundStyle(theme.secondary)
                .padding(.horizontal, 12)
                .padding(.bottom, 8)
            ForEach(SettingsTab.allCases) { tab in
                pageButton(id: tab.id, title: tab.label, symbol: tab.symbol, color: tab.color)
            }
            ForEach(additionalPages) { page in
                pageButton(id: page.id, title: page.title(), symbol: page.symbol, color: page.color)
            }
            Spacer()
            HStack(spacing: 10) {
                Image(nsImage: NSApplication.shared.applicationIconImage)
                    .resizable().scaledToFit().frame(width: 32, height: 32)
                VStack(alignment: .leading, spacing: 3) {
                    Text(AppResources.applicationName).font(.ui(12, .semibold)).foregroundStyle(theme.text)
                    Text(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")
                        .font(.ui(11)).foregroundStyle(theme.secondary)
                }
            }
            .padding(12)
        }
        .padding(.horizontal, 10)
        .padding(.bottom, 12)
        .frame(maxHeight: .infinity)
        .background(theme.sidebarBackground)
    }

    private func pageButton(id: String, title: String, symbol: String, color: Color) -> some View {
        let selected = id == selection
        return Button { selection = id } label: {
            HStack(spacing: 8) {
                Image(systemName: symbol)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.white)
                    .frame(width: 26, height: 26)
                    .background(color, in: RoundedRectangle(cornerRadius: 7))
                Text(title).font(.ui(13, selected ? .semibold : .regular))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 9)
            .background(selected ? Color.accentColor : .clear, in: RoundedRectangle(cornerRadius: 9))
            .foregroundStyle(selected ? .white : theme.sidebarText)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("settings-tab-\(id)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// One titled group for settings content.
public struct SettingsSection<Content: View>: View {
    let title: String
    var subtitle: String? = nil
    let theme: Theme
    @ViewBuilder let content: () -> Content

    public init(title: String, subtitle: String? = nil, theme: Theme, @ViewBuilder content: @escaping () -> Content) {
        self.title = title
        self.subtitle = subtitle
        self.theme = theme
        self.content = content
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 5) {
                Text(title).font(.ui(15, .semibold))
                if let subtitle {
                    Text(subtitle).font(.ui(12)).foregroundStyle(theme.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            VStack(spacing: 0, content: content)
                .background(theme.card, in: RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(theme.cardBorder.opacity(0.55), lineWidth: 1))
        }
    }
}

struct SettingsDivider: View {
    let theme: Theme
    var body: some View { Rectangle().fill(theme.divider).frame(height: 1).padding(.horizontal, 16) }
}

struct SettingsToggleRow: View {
    let label: String
    var subtitle: String? = nil
    @Binding var isOn: Bool

    var body: some View {
        SettingRow(label: label, subtitle: subtitle) {
            Toggle(label, isOn: $isOn).labelsHidden().toggleStyle(.switch).controlSize(.small)
        }
    }
}
