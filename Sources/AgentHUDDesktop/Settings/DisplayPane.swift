import SwiftUI
import AgentHUDCore

struct DisplayPane: View {
    let settings: SettingsStore
    let store: UsageStore
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            GlowPane(settings: settings, store: store, theme: theme)
            IslandPane(settings: settings, theme: theme)
            SettingsSection(title: L10n.text("菜单栏", "Menu bar"), theme: theme) {
                SettingsToggleRow(
                    label: L10n.text("显示菜单栏图标", "Show menu bar icon"),
                    subtitle: L10n.text("快速查看用量、刷新数据和打开设置。", "Quick access to usage, refresh and settings."),
                    isOn: settings.binding(\.showMenuBarIcon)
                )
            }
        }
    }
}

/// Decorative wallpaper for the glow preview.
struct SettingsPreview<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .frame(maxWidth: .infinity)
            .background {
                GeometryReader { geometry in
                    Image(nsImage: SettingsPreviewArtwork.wallpaper)
                        .resizable().scaledToFill()
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .clipped()
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .padding(12)
            .accessibilityHidden(true)
    }
}

private enum SettingsPreviewArtwork {
    static let wallpaper = NSImage(contentsOf: AppResources.bundle.url(forResource: "settings-wallpaper", withExtension: "png")!)!
}
