import AppKit
import AgentHUDCore

@MainActor
final class SettingsWindowController: HostedWindowController {
    private let navigation = SettingsNavigation()
    private let pageIDs: Set<String>

    init(settings: SettingsStore, store: UsageStore, additionalPages: [DesktopSettingsPage] = []) {
        pageIDs = Set(SettingsTab.allCases.map(\.id) + additionalPages.map(\.id))
        var size = SettingsWindowLayout.size
        if let contentWidth = additionalPages.compactMap(\.preferredContentWidth).max() {
            size.width = max(size.width, contentWidth + SettingsWindowLayout.sidebarWidth + 40)
        }
        super.init(
            size: size,
            title: Self.title,
            resizable: true,
            content: SettingsView(settings: settings, store: store, navigation: navigation,
                                  additionalPages: additionalPages)
        )
        // Custom controls handle their own drags; only the title bar should move this window.
        window?.isMovableByWindowBackground = false
        window?.minSize = SettingsWindowLayout.minimum
    }

    private static var title: String { AppResources.applicationName + L10n.text(" 设置", " Settings") }

    override func show() {
        show(pageID: nil)
    }

    func show(pageID: String?) {
        if let pageID, pageIDs.contains(pageID) { navigation.pageID = pageID }
        window?.title = Self.title
        super.show()
    }

}
