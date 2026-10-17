import AppKit
import AgentHUDCore
import SwiftUI

/// Owns the local desktop presentation and observes the supplied usage store.
@MainActor
public final class DesktopApplication {
    public let settings: SettingsStore
    public let store: UsageStore
    private let options: DesktopLaunchOptions
    private let additionalSettingsPages: [DesktopSettingsPage]
    private let experimentalSettings: [DesktopSettingsSection]
    private let additionalMenuItems: () -> [NSMenuItem]
    private let additionalHUDControls: @MainActor (@escaping @MainActor () -> Void) -> AnyView
    private let onIslandEvents: ((IslandEventTracker.Update, UsageReport, Date) -> Void)?
    /// When the app started: what happened to an agent before it is history, for the island's events and the marks'.
    private let startedAt: Date
    private var islandEvents = IslandEventTracker()
    /// The requests already on the island, so a change to the waiting list says which ones arrived and which left.
    private var shownRequests: [String] = []
    /// The scenario a demo launched with `--demo-script` is playing.
    private var demoScript: DemoScriptRunner?
    /// The movie a launch with `--record` is writing.
    private var recorder: HUDRecorder?
    /// The clients whose observer directory existed at the last look, so one that appears later is noticed.
    private var observedClients: Set<String> = []
    private let antigravityApprovals = AntigravityPermissionObserver()
    private let deepseekQuestions = DeepSeekQuestionObserver()
    private let codexDesktopApprovals = CodexDesktopPermissionObserver()
    /// Where the MCP process a client started hands its tool calls to this copy of the application, open while the
    /// switch in Settings → Experimental is on.
    private var toolChannel: AgentToolsChannel?
    private var notch: IslandController?
    private var statusItem: StatusItemController?
    /// What the Mac tells the windows that follow their screen's look (`WindowLook`): a change of displays, of its light
    /// or dark setting, or of Reduce Transparency, none of which the settings observe.
    private var windowLookObservers: [NSObjectProtocol] = []
    private lazy var settingsWindow = SettingsWindowController(
        settings: settings, store: store, additionalPages: additionalSettingsPages, experimentalSections: experimentalSettings
    )
    private lazy var statsWindow = StatsWindowController(store: store)
    private let onboardingWindow: OnboardingWindowController

    /// `onIslandEvents` receives every island event check after the island has presented it, including checks that
    /// found nothing, with the report and time the check used. `experimentalSettings` are drawn on the built-in
    /// Experimental page, below its own rows, in the order given.
    public init(options: DesktopLaunchOptions, settings: SettingsStore, store: UsageStore,
                additionalSettingsPages: [DesktopSettingsPage] = [],
                experimentalSettings: [DesktopSettingsSection] = [],
                additionalMenuItems: @escaping () -> [NSMenuItem] = { [] },
                additionalHUDControls: @escaping @MainActor (@escaping @MainActor () -> Void) -> AnyView = { _ in AnyView(EmptyView()) },
                onIslandEvents: ((IslandEventTracker.Update, UsageReport, Date) -> Void)? = nil) {
        self.options = options
        self.settings = settings
        self.store = store
        self.additionalSettingsPages = additionalSettingsPages
        self.experimentalSettings = experimentalSettings
        self.additionalMenuItems = additionalMenuItems
        self.additionalHUDControls = additionalHUDControls
        self.onIslandEvents = onIslandEvents
        startedAt = store.clock()
        islandEvents = IslandEventTracker(startedAt: startedAt)
        onboardingWindow = OnboardingWindowController(settings: settings, store: store,
            sources: options.demo ? { DemoData.sources } : { SourceDetector.detect() })
        onboardingWindow.onFinish = { [weak self] in
            guard let self else { return }
            self.settings.markOnboardingComplete()
            Task { await self.store.refresh() }
        }
    }

    public func start() {
        let sources = options.demo ? DemoData.sources : SourceDetector.resolve(SourceDetector.detect(), report: store.report)
        settings.update { $0.applyLiveStatusDefaults(sources: sources) }
        WindowLook.source = { [weak self] screen in self?.settings.windowLook(on: screen) }
        applyAppearance()
        // Clawd plays the Claude app's own sprites, bundled with Agent HUD and decoded once its first mark is built, in a
        // demo and a recording too.
        ClawdSprites.shared.source = .shared
        let notch = IslandController(store: store, settings: settings, additionalHUDControls: additionalHUDControls,
                                     startedAt: startedAt)
        notch.onOpenStats = { [weak self] in self?.showStats() }
        notch.onOpenSettings = { [weak self] in self?.showSettings() }
        self.notch = notch
        let statusItem = StatusItemController(store: store, settings: settings, additionalMenuItems: additionalMenuItems)
        statusItem.actions = MenuActions(
            toggleGlow: { [weak self] in self?.toggleGlow() },
            openSettings: { [weak self] in self?.showSettings() },
            openStats: { [weak self] in self?.showStatsOverview() },
            quit: { NSApp.terminate(nil) }
        )
        self.statusItem = statusItem
        HotKeyCenter.shared.register(id: 1, keyCode: HotKeyCenter.keyH, modifiers: HotKeyCenter.commandOption) { [weak self] in
            self?.toggleGlow()
        }
        // The app's Appearance, an agent's unsaved draft of it included, reaches every window that does not match its
        // screen.
        trackChanges({ [weak self] in
            self?.settings.effectiveSettings.appearance
        }, onChange: { [weak self] in self?.applyAppearance() })
        // Under Match screen a screen's new look, its style draft's included, reaches the windows on that screen. The
        // settings with the app's draft and the style drafts (any one of them read reads them all) are read apart from
        // the displays as well, so the watch is armed with no display attached too; a window whose look has not changed
        // is left as it is.
        trackChanges({ [weak self] in
            guard let self else { return }
            _ = self.settings.effectiveSettings
            _ = self.settings.panelLayout.styleDraft(of: "")
            _ = NSScreen.screens.map { self.settings.windowLook(on: $0) }
        }, onChange: { [weak self] in self?.refreshLooks() })
        observeWindowLooks()
        trackChanges({ [weak self] in
            self?.settings.settings.language
        }, onChange: { [weak self] in
            guard let self else { return }
            self.statusItem?.refreshButton()
            self.notch?.apply(animated: false)
            Task { await self.store.refresh() }
        })
        trackChanges({ [weak self] in
            _ = self?.store.lastError
        }, onChange: { [weak self] in
            if let error = self?.store.lastError { NSLog("[AgentHUD] refresh failed: %@", error) }
        })
        // A new install starts with launch at login on, and nothing registers it until the setting is applied: a start
        // registers an item that was never registered, and a flip of the switch applies at once. The demo leaves the
        // system's login items alone.
        if !options.demo, settings.settings.launchAtLogin { LoginItem.registerIfNeverRegistered() }
        trackChanges({ [weak self] in
            self?.settings.settings.launchAtLogin
        }, onChange: { [weak self] in
            guard let self, !self.options.demo else { return }
            LoginItem.set(self.settings.settings.launchAtLogin)
        })
        PermissionRequests.shared.holdTime = TimeInterval(settings.settings.approvalWaitMinutes * 60)
        trackChanges({ [weak self] in
            self?.settings.settings.approvalWaitMinutes
        }, onChange: { [weak self] in
            guard let self else { return }
            PermissionRequests.shared.holdTime = TimeInterval(self.settings.settings.approvalWaitMinutes * 60)
        })
        // The host installs the handlers at launch; a change of mind while running applies at once, with the same
        // executable the host gave them.
        trackChanges({ [weak self] in
            self?.settings.settings.clientHooks
        }, onChange: { [weak self] in
            guard let self, !self.options.demo, let executable = Bundle.main.executableURL else { return }
            SessionObservers.configure(executable: executable, enabled: self.settings.settings.clientHooks)
            self.antigravityApprovals.setEnabled(self.settings.settings.clientHooks)
            self.deepseekQuestions.setEnabled(self.settings.settings.clientHooks)
            self.codexDesktopApprovals.setEnabled(self.settings.settings.clientHooks)
        })
        // A client run for the first time creates its directory, and its observer goes in with the next report
        // rather than at the next launch.
        if !options.demo { observedClients = SessionObservers.observedClients() }
        trackChanges({ [weak self] in
            _ = self?.store.report
        }, onChange: { [weak self] in
            self?.installObserversForNewClients()
            if let self { self.codexDesktopApprovals.updateSessions(self.store.report?.sessions ?? []) }
            self?.showClaudeDesktopAsks()
        })
        trackChanges({ [weak self] in
            _ = self?.store.report
            _ = self?.settings.agents
            _ = self?.settings.settings.disabledLiveStatusSources
        }, onChange: { [weak self] in self?.checkIslandEvents() })
        // The channel is open whenever the app is: a client that asks while it is closed keeps its own prompt.
        trackChanges({ PermissionRequests.shared.pending.map(\.id) },
                       onChange: { [weak self] in self?.syncPermissionRequests() })
        // Seeded after the island is listening, so the demo's requests arrive the way a client's would. A script
        // brings them in one at a time instead, and sets what the demo reports before the first report is read.
        if let name = options.demoScript {
            demoScript = DemoScriptRunner.load(name, store: store, island: notch)
        }
        if !options.demo { PermissionRequests.shared.start() } else if demoScript == nil { PermissionRequests.shared.seedDemo() }
        if options.toolChannel != .never {
            // A demo shares the data directory with the real copy, so it takes the tool calls only when asked to; any
            // other launch takes them while the switch in Settings → Experimental is on. A screen's panel is drawn and
            // measured as that display's island draws it, off screen; the shared layout as the main display's. One agent's
            // own panel is drawn as the card its mark opens on that display's dock, in its look.
            let renderer = { @MainActor [weak self] (scope: PanelLayoutScope, agent: String?) -> PanelPictureRenderer? in
                guard let self else { return nil }
                return PanelPictureRenderer(store: self.store, display: self.notch?.panelDisplay(for: scope.screenKey) ?? .main,
                                            screen: scope.screenKey, agent: agent)
            }
            // A screen's resting HUD is drawn as Settings previews it, off screen, with that display's notch.
            let hud = { @MainActor [weak self] (key: String) -> HUDPicture? in
                guard let self else { return nil }
                let display = NSScreen.screens.first { ScreenIdentity.key(for: $0) == key }
                return HUDPictureRenderer(store: self.store, screen: key, hasNotch: display.map(ScreenIdentity.hasNotch) ?? false,
                                          look: self.store.settings.restingLook(on: key),
                                          metrics: ScreenMetrics(screen: display)).picture()
            }
            let tools = HUDTools(store: store, screens: { [weak self] in self?.notch?.panelScreens ?? [] },
                                   picture: { renderer($0, nil)?.picture() }, fit: { renderer($0, nil)?.fit() }, hud: hud,
                                   agentPicture: { renderer(.screen($0), $1)?.picture() })
            let channel = AgentToolsChannel(channel: MCPAppChannel { tools.answer($0) }, settings: settings, mode: options.toolChannel)
            toolChannel = channel
            channel.start()
        }
        if !options.demo {
            antigravityApprovals.setEnabled(settings.settings.clientHooks)
            deepseekQuestions.setEnabled(settings.settings.clientHooks)
            codexDesktopApprovals.updateSessions(store.report?.sessions ?? [])
            codexDesktopApprovals.setEnabled(settings.settings.clientHooks)
        }
        store.start()
        startRecording()
        demoScript?.start()
        if options.openPanel { notch.forceOpen() }
        if store.isAccessAllowed, options.showOnboarding || !settings.hasCompletedOnboarding { showOnboarding() }
        if options.showSettings { showSettings() }
        if options.showStats { showStats() }
    }

    public func stop() {
        antigravityApprovals.stop()
        deepseekQuestions.stop()
        codexDesktopApprovals.stop()
        // Quitting must never leave a client waiting on an answer that is no longer coming.
        PermissionRequests.shared.stop()
        toolChannel?.stop()
        demoScript?.stop()
        recorder?.finishAndWait()
        store.stop()
    }

    /// `--record`: the main display's HUD, from now until the time given or until the app quits.
    private func startRecording() {
        guard let path = options.recordPath, let screen = NSScreen.main else { return }
        var recording = HUDRecorder.Options(url: URL(fileURLWithPath: (path as NSString).expandingTildeInPath))
        recording.seconds = options.recordSeconds
        if let fps = options.recordFPS { recording.framesPerSecond = fps }
        recording.region = options.recordRegion.flatMap(HUDRecorder.Options.region)
        do {
            let recorder = try HUDRecorder(options: recording, screen: screen) { [weak self] in self?.notch?.recordingSources ?? [] }
            recorder.onFinish = { NSApp.terminate(nil) }
            try recorder.start()
            self.recorder = recorder
            // The movie is drawn from the windows' layers, which hold no glass or blur it could show.
            notch?.drawsGlassStandIn = true
        } catch {
            NSLog("[AgentHUD] Recording to %@ could not start: %@", path, String(describing: error))
        }
    }

    /// How the host sends the words of an answer to the Claude desktop app's next-step box into its session, such as the
    /// private app's plugin; nil leaves the box to be answered in the app.
    public var claudeDesktopReply: ClaudeDesktopAsks.Reply? {
        didSet { showClaudeDesktopAsks() }
    }

    /// The questions the Claude desktop app asks in its own window, which only its session logs record, among the requests
    /// the user is asked about. The demo has none.
    private func showClaudeDesktopAsks() {
        guard !options.demo else { return }
        ClaudeDesktopAsks.show(store.view.recentSessions.filter(\.liveStatus).map(\.session), now: Date(), reply: claudeDesktopReply)
    }

    /// Mirrors the requests waiting for the user onto the island: a new one is shown, and one the client took back
    /// disappears without being answered.
    private func syncPermissionRequests() {
        let pending = PermissionRequests.shared.pending
        let ids = pending.map(\.id)
        for id in shownRequests where !ids.contains(id) { notch?.withdraw(requestID: id) }
        for request in pending where !shownRequests.contains(request.id) { notch?.present(.permission(request)) }
        shownRequests = ids
        QuestionDraft.keep(Set(ids))
        PlanDraft.keep(Set(ids))
        // A request that only joined or left the queue changes no card, but it does change how many are waiting.
        notch?.apply(animated: true)
    }
    public func showSettings(pageID: String? = nil) { settingsWindow.show(pageID: pageID) }
    public func showStats() {
        Task { await store.refreshAccounts() }
        statsWindow.show()
    }
    /// The statistics window on its Tokens page: the menu's rows are about quotas and balances. A session page opened
    /// from the HUD outlives its window, so the Sessions page starts from its list again.
    public func showStatsOverview() {
        store.focusedSessionID = nil
        store.statsTab = .tokens
        showStats()
    }
    public func showOnboarding() { onboardingWindow.show() }
    public func toggleGlow() { store.glowHidden.toggle() }

    /// Start-up installs observers only where a client's directory already exists. A client that already runs loads its
    /// observer when it next starts or reloads.
    private func installObserversForNewClients() {
        guard !options.demo else { return }
        let present = SessionObservers.observedClients()
        if settings.settings.clientHooks, !present.subtracting(observedClients).isEmpty { SessionObservers.installObservers() }
        observedClients = present
    }

    /// A paused store or a failed refresh leaves the island silent; the baselines wait for the next good report.
    private func checkIslandEvents() {
        guard !store.isPaused, store.lastError == nil, let report = store.report else { return }
        let now = store.clock()
        let update = islandEvents.update(report: report, agents: settings.agents, now: now, settings: settings.settings)
        for alert in update.quotaAlerts { notch?.present(alert) }
        for grant in update.resetCreditGrants { notch?.present(.resetCredits(grant)) }
        for completion in update.completions { notch?.present(.completion(completion)) }
        onIslandEvents?(update, report, now)
    }

    /// Has the windows take their screen's look again whenever the Mac changes what it depends on. Each waits a turn of
    /// the run loop, so that the HUDs have taken a change of displays first and every screen is known by its key again.
    private func observeWindowLooks() {
        let refresh: @Sendable (Notification) -> Void = { [weak self] _ in
            Task { @MainActor in self?.refreshLooks() }
        }
        windowLookObservers = [
            NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification,
                                                   object: nil, queue: .main, using: refresh),
            DistributedNotificationCenter.default().addObserver(forName: Notification.Name("AppleInterfaceThemeChangedNotification"),
                                                                object: nil, queue: .main, using: refresh),
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil, queue: .main,
                using: refresh),
        ]
    }

    private func applyAppearance() {
        // With Match screen on, each window and the menu take their screen's over this (`WindowLook`).
        switch settings.effectiveSettings.appearance {
        case .system: NSApp.appearance = nil
        case .dark: NSApp.appearance = NSAppearance(named: .darkAqua)
        case .light: NSApp.appearance = NSAppearance(named: .aqua)
        }
        refreshLooks()
    }

    /// Every window and the open menu-bar menu take their screen's look again (`WindowLook`).
    private func refreshLooks() {
        WindowLook.refreshAll()
        statusItem?.refreshLook()
    }
}
