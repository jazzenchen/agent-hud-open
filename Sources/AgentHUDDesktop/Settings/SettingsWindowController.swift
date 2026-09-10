import AppKit
import AgentHUDCore

@MainActor
final class SettingsWindowController: HostedWindowController {
    private let navigation = SettingsNavigation()

    init(settings: SettingsStore, store: UsageStore) {
        super.init(
            size: SettingsWindowLayout.size,
            title: Self.title,
            resizable: true,
            content: SettingsView(settings: settings, store: store, navigation: navigation)
        )
        // Custom controls handle their own drags; only the title bar should move this window.
        window?.isMovableByWindowBackground = false
        window?.minSize = SettingsWindowLayout.minimum
    }

    private static var title: String { AppResources.applicationName + L10n.text(" 设置", " Settings") }

    override func show() {
        window?.title = Self.title
        super.show()
    }

}
