import AppKit
import AgentHUDCore

@MainActor
public struct DesktopMenuAction {
    public var title: () -> String
    public var action: () -> Void

    public init(title: @escaping () -> String, action: @escaping () -> Void) {
        self.title = title
        self.action = action
    }
}

/// Owns the local desktop presentation and observes the supplied usage store.
@MainActor
public final class DesktopApplication {
    public let settings: SettingsStore
    public let store: UsageStore
    private let options: DesktopLaunchOptions
    private let additionalMenuActions: [DesktopMenuAction]
    private var notch: NotchController?
    private var statusItem: StatusItemController?
    private lazy var settingsWindow = SettingsWindowController(settings: settings, store: store)
    private let statsWindow: StatsWindowController
    private let onboardingWindow: OnboardingWindowController

    public init(options: DesktopLaunchOptions, settings: SettingsStore, store: UsageStore,
                additionalMenuActions: [DesktopMenuAction] = []) {
        self.options = options
        self.settings = settings
        self.store = store
        self.additionalMenuActions = additionalMenuActions
        statsWindow = StatsWindowController(store: store)
        onboardingWindow = OnboardingWindowController(settings: settings, store: store,
            sources: options.demo ? { DemoData.sources } : { SourceDetector.detect() })
        onboardingWindow.onFinish = { [weak self] in
            guard let self else { return }
            self.settings.markOnboardingComplete()
            Task { await self.store.refresh() }
        }
    }

    public func start() {
        applyAppearance()
        let notch = NotchController(store: store, settings: settings)
        notch.onOpenStats = { [weak self] in self?.showStats() }
        notch.onOpenSettings = { [weak self] in self?.showSettings() }
        self.notch = notch
        let statusItem = StatusItemController(store: store, settings: settings)
        statusItem.actions = MenuActions(
            toggleGlow: { [weak self] in self?.toggleGlow() },
            openSettings: { [weak self] in self?.showSettings() },
            openStats: { [weak self] in self?.showStats() },
            additional: additionalMenuActions,
            quit: { NSApp.terminate(nil) }
        )
        self.statusItem = statusItem
        HotKeyCenter.shared.register(id: 1, keyCode: HotKeyCenter.keyH, modifiers: HotKeyCenter.commandOption) { [weak self] in
            self?.toggleGlow()
        }
        observeChanges({ [weak self] in
            _ = self?.settings.settings.appearance
        }, onChange: { [weak self] in self?.applyAppearance() })
        observeChanges({ [weak self] in
            _ = self?.settings.settings.language
        }, onChange: { [weak self] in
            guard let self else { return }
            self.statusItem?.refreshButton()
            self.notch?.apply(animated: false)
            Task { await self.store.refresh() }
        })
        observeChanges({ [weak self] in
            _ = self?.store.lastError
        }, onChange: { [weak self] in
            if let error = self?.store.lastError { NSLog("[AgentHUD] refresh failed: %@", error) }
        })
        observeChanges({ [weak self] in
            _ = self?.settings.settings.launchAtLogin
        }, onChange: { [weak self] in
            guard let self else { return }
            LoginItem.set(self.settings.settings.launchAtLogin)
        })
        store.start()
        if options.openPanel { notch.forceOpen() }
        if store.isAccessAllowed, options.showOnboarding || !settings.hasCompletedOnboarding { showOnboarding() }
        if options.showSettings { showSettings() }
        if options.showStats { showStats() }
    }

    public func stop() { store.stop() }
    public func showSettings() { settingsWindow.show() }
    public func showStats() { statsWindow.show() }
    public func showOnboarding() { onboardingWindow.show() }
    public func toggleGlow() { store.glowHidden.toggle() }
    public func present(_ alert: QuotaAlert) { notch?.present(alert) }
    public func present(_ completion: SessionCompletion) { notch?.present(.completion(completion)) }

    private func applyAppearance() {
        switch settings.settings.appearance {
        case .system: NSApp.appearance = nil
        case .dark: NSApp.appearance = NSAppearance(named: .darkAqua)
        case .light: NSApp.appearance = NSAppearance(named: .aqua)
        }
    }
}
