import AppKit
import SwiftUI
import AgentHUDCore

#if DEBUG
/// The stills of the HUD in each look, named `hud-<look>-<state>`.
extension SnapshotRunner {
    /// The looks the stills are drawn in, each with the name it gives a file. A glass look is drawn as its stand-in, its
    /// tint over the wallpaper: an off-screen hosting view has nothing behind it to blur (`drawsGlassStandIn`).
    static let hudLooks: [(name: String, look: HUDLook)] = [
        ("dark", .standard),
        ("light", HUDLook(appearance: .light)),
        ("glass-dark", HUDLook(appearance: .dark, material: .glass)),
        ("glass-light", HUDLook(appearance: .light, material: .glass)),
    ]

    /// The HUD in each look, every state drawn from the same scene with only the look differing: the open panel in the
    /// default layout; the panel with ring, number and chart widgets and blocks half a line wide; under its sender's
    /// mark, a request to run Bash, a question, a finished turn, a window that ran out and usage resets added; the
    /// Simple panel with its bots; a Dynamic dock's marks, a single-colour one among them; one agent's peek on a dock
    /// that separates its agents; and the panel with an unsaved layout's bar. The open panel and the dock are drawn
    /// again over a white desktop, with the hairline a light desktop gives the island, and over a black one, where a
    /// light or glass look is likeliest to fail.
    ///
    /// Each HUD hangs from the top edge of a Dynamic dock or floats as a Simple panel, never from a hardware notch, where
    /// the island keeps the standard look whatever the screen's. They are drawn from a settings store of their own.
    static func saveHUDLooks(preferences: AgentHUDCore.Settings, folder: URL) {
        if let prefix = ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"],
           !"hud-".hasPrefix(prefix) && !prefix.hasPrefix("hud-") { return }
        let suite = "app.agenthud.open.snapshot.hud-looks"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        settings.update { $0 = preferences }
        let store = demoStore(settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: DemoClock.now))
        let layout = settings.panelLayout
        let quota = store.view.panelBlocks.filter { $0.kind == .quota }
        let balance = store.view.panelBlocks.first { $0.kind == .billing }

        let bash = PermissionRequest(id: "snapshot-look-bash", source: .claude, sessionID: "snapshot", toolName: "Bash",
                                     summary: L10n.text("删除构建产物", "Remove the build output"),
                                     detail: "rm -rf dist", cwd: "/Users/me/Projects/acme-web", at: DemoClock.now)
        let question = snapshotQuestion()
        let reply = IslandAlert.completion(SessionCompletion(
            sessionID: "snapshot", vendor: "Claude", turnID: "look",
            task: L10n.text("自动更新检查", "Automatic update checks"), model: "Fable 5.1",
            startedAt: DemoClock.now.addingTimeInterval(-83), completedAt: DemoClock.now,
            message: L10n.text("已改好：每次打开应用立即检查更新，之后每小时检查一次。",
                               "The app now checks for updates every time it opens, then once an hour."),
            navigationTarget: .iTermSession(id: "snapshot-terminal")))
        let agent = AgentDescriptor(id: "claude-session", vendor: "Claude", model: L10n.windowSession, source: "", enabled: true)
        let exhausted = IslandAlert.quota(QuotaAlert.preview(.exhaustion, agent: agent, now: DemoClock.now))
        let grant = IslandAlert.resetCredits(.preview(now: DemoClock.now))

        for (name, look) in hudLooks {
            func shot(_ state: String, _ hud: some View, size: CGSize, desktop: Color? = nil) {
                save("hud-\(name)-\(state)", onWallpaper(hud, size: size, desktop: desktop), folder: folder,
                     scheme: look.colorScheme)
            }
            func dock(desktop: Color? = nil, _ configure: (inout HUDLookDock) -> Void = { _ in }) {
                var scene = HUDLookDock(store: store, settings: settings, look: look)
                configure(&scene)
                let root = scene.root()
                shot(scene.state, root, size: scene.canvas, desktop: desktop)
            }

            dock { $0.state = "panel-default"; $0.open = true }
            if quota.count > 1, let balance {
                // The first agent's rings beside the balance's number, the second agent's chart across the line below.
                layout.propose(PanelLayout(blocks: [
                    quota[0].key: PanelLayout.Block(widget: .ring, span: .half),
                    balance.key: PanelLayout.Block(span: .half),
                    quota[1].key: PanelLayout.Block(widget: .chart),
                ], order: [quota[0].key, balance.key, quota[1].key]))
                layout.save()
                dock { $0.state = "panel-widgets"; $0.open = true }
                layout.reset()
            }
            dock { $0.state = "permission-bash"; $0.open = true; $0.alert = .permission(bash); $0.waiting = [bash] }
            dock { $0.state = "question"; $0.open = true; $0.alert = .permission(question); $0.waiting = [question] }
            dock { $0.state = "completion"; $0.open = true; $0.alert = reply }
            dock { $0.state = "quota-exhausted"; $0.open = true; $0.alert = exhausted }
            dock { $0.state = "reset-credits"; $0.open = true; $0.alert = grant }
            dock { $0.state = "dynamic-dock"; $0.extraMarks = ["Cursor"] }
            for (desktop, color, light) in [("white", Color.white, true), ("black", Color.black, false)] {
                dock(desktop: color) { $0.state = "panel-default-on-\(desktop)"; $0.open = true; $0.lightBorder = light }
                dock(desktop: color) {
                    $0.state = "dynamic-dock-on-\(desktop)"; $0.extraMarks = ["Cursor"]; $0.lightBorder = light
                }
            }
            dock { $0.state = "agent-peek"; $0.open = true; $0.peekVendor = "Claude" }
            // A layout an agent proposed and nobody has saved yet.
            layout.propose(PanelLayout(style: PanelStyle(spacing: .compact)))
            dock { $0.state = "panel-draft"; $0.open = true }
            layout.discard()

            let simple = simpleWithBots(store: store, look: look)
            shot("simple-bots", simple.view, size: simple.size)
        }
    }

    /// The scene every look's stills share: a wallpaper, or a desktop of one colour, with the HUD hanging from its top
    /// edge, which stands for the screen's, `size` the HUD's own canvas.
    private static func onWallpaper(_ hud: some View, size: CGSize, desktop: Color?) -> some View {
        hud
            .frame(width: size.width, height: size.height, alignment: .top)
            .padding(EdgeInsets(top: 0, leading: 40, bottom: 40, trailing: 40))
            .background {
                if let desktop {
                    desktop
                } else {
                    LinearGradient(
                        stops: [
                            .init(color: Color(hex: 0x5b6b8c), location: 0),
                            .init(color: Color(hex: 0x7c8aa6), location: 0.45),
                            .init(color: Color(hex: 0xb7a6a1), location: 1),
                        ],
                        startPoint: .top, endPoint: .bottom
                    )
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 14))
    }

    /// The Simple panel with its bots on, as the island draws it at rest, and the bots of its agents at work beside it as
    /// stills on the look's surface: a bot is an animating layer, which a bitmap of the panel cannot see.
    private static func simpleWithBots(store: UsageStore, look: HUDLook) -> (view: some View, size: CGSize) {
        let placement = ScreenPlacement(mode: .simple, simpleSize: 28)
        let config = SimpleMeterConfig(items: ScreenAgents(store: store, listed: [], mode: .simple).meterItems,
                                       placement: placement, bots: true)
        let side: CGFloat = 28
        let view = VStack(spacing: 14) {
            SimpleMeterView(config: config)
                .frame(width: config.stripSize.width, height: config.stripSize.height)
            HStack(spacing: 10) {
                ForEach(config.items) { item in
                    if let form = BotForm.form(for: item.vendor, ground: look.appearance),
                       let image = workingBot(form, side: side) {
                        Image(decorative: image, scale: 2)
                    } else {
                        AgentLogo(vendor: item.vendor, size: side)
                    }
                }
            }
            .padding(10)
            .background(HUDSurface(RoundedRectangle(cornerRadius: config.cornerRadius, style: .continuous), look: look,
                                   fill: look.theme.islandFill, contact: nil))
            .overlay(RoundedRectangle(cornerRadius: config.cornerRadius, style: .continuous)
                .strokeBorder(look.theme.islandOutline, lineWidth: 1))
        }
        .hudLook(look)
        let width = max(config.stripSize.width, CGFloat(config.items.count) * (side * (1 + 2 * BotRenderer.margin) + 10) + 20)
        return (view, CGSize(width: width, height: config.stripSize.height + 14 + side * (1 + 2 * BotRenderer.margin) + 20))
    }

    /// A bot a moment into its work, looking a little down and aside, the same on every run.
    private static func workingBot(_ form: BotForm, side: CGFloat) -> CGImage? {
        var motion = BotMotion(seed: 11)
        motion.spin = form.spec.spin
        motion.set(.working, at: 0)
        var now: TimeInterval = 0
        while now <= 1.2 {
            motion.step(to: now, aim: CGPoint(x: 0.35, y: 0.45))
            now += 1.0 / 30
        }
        return BotRenderer.image(form, motion: motion, side: side * 2)
    }
}

/// One still of a Dynamic dock on a top edge, the island as the screen builds it (`ScreenHUD.makeRoot`): its marks
/// alone at rest, the panel grown out of the dock with them over it, or a card under one of them, an event's or an
/// agent's peek.
@MainActor
struct HUDLookDock {
    let store: UsageStore
    let settings: SettingsStore
    let look: HUDLook
    /// The still's name after the look's.
    var state = ""
    var open = false
    var alert: IslandAlert? = nil
    var waiting: [PermissionRequest] = []
    /// The agent whose peek floats under its mark, on a dock that separates its agents.
    var peekVendor: String? = nil
    /// Marks after the demo agents' own, such as a single-colour one, which the demo has none of.
    var extraMarks: [String] = []
    /// The desktop behind the island is light, which a hairline round its silhouette keeps it apart from.
    var lightBorder = false

    private var placement: ScreenPlacement { ScreenPlacement(mode: .logos, logoSize: 24, separatesAgents: peekVendor != nil) }
    private var queue: LogoQueueConfig {
        LogoQueueConfig(items: ScreenAgents(store: store, listed: [], mode: .logos).queueItems
                            + extraMarks.map { LogoQueueItem(vendor: $0, isWorking: false) },
                        placement: placement, settings: settings.settings)
    }
    private var flare: CGFloat { NotchGeometry.expandedTopRadius }
    private var panelWidth: CGFloat { CGFloat(settings.panelLayout.effective(for: nil).style.panelWidth) }
    private var peek: AgentPeek? { peekVendor.map { AgentPeek.make(vendor: $0, store: store) } }

    /// The agent whose card floats under its mark: an event's sender, or the agent whose peek is shown.
    private var cardVendor: String? { open ? alert?.vendor ?? peekVendor : nil }

    /// The card under a mark: a peek, or an event's card at its natural height.
    private var cardSize: CGSize? {
        guard open else { return nil }
        if let alert { return CGSize(width: alert.detailWidth, height: cardHeight) }
        return peek?.size
    }

    /// The canvas the island is drawn in: the strip with a card under it, or the panel with its shoulders and the
    /// strip, whichever is larger.
    var canvas: CGSize {
        let strip = queue.size
        if let card = cardSize {
            return CGSize(width: max(strip.width, card.width) + 2 * flare, height: strip.height + 6 + card.height)
        }
        return CGSize(width: max(strip.width, open ? panelWidth : 0) + 2 * flare, height: max(strip.height, open ? cardHeight : 0))
    }

    /// The card's natural height, measured as the island measures it before it opens.
    private var cardHeight: CGFloat {
        var natural = base()
        natural.presentationSize = nil
        natural.presentationFrame = nil
        return max(80, IslandWindowController.naturalHeight(of: natural.content).rounded())
    }

    private func base() -> IslandRootView {
        var root = IslandRootView(store: store, isOpen: open, collapsedSize: queue.size,
                                  collapsedTopRadius: NotchGeometry.expandedTopRadius, collapsedBottomRadius: 10,
                                  look: look, lightBorder: lightBorder, onOpenStats: {})
        root.alert = alert
        root.showsAlertDetails = open && alert != nil
        root.waitingRequests = waiting
        root.sessionEvents = alert.map { $0.isSessionEvent ? [$0] : [] } ?? []
        root.panelWidth = panelWidth
        root.logoQueue = queue
        root.hidesSilhouette = true
        root.separatesAgents = placement.separatesAgents
        root.edge = .top
        root.animatesGeometry = false
        if let cardVendor {
            root.focusVendor = cardVendor
            root.peek = alert == nil ? peek : nil
        }
        return root
    }

    /// The island, laid out in `canvas` the way the screen lays it out in its window.
    func root() -> IslandRootView {
        var root = base()
        let canvas = canvas, strip = queue.size
        let stripFrame = CGRect(x: (canvas.width - strip.width) / 2, y: 0, width: strip.width, height: strip.height)
        root.logoQueueFrame = stripFrame
        root.presentationSize = canvas
        if let card = cardSize, let vendor = cardVendor {
            // Under its agent's mark, or the middle of the dock for an agent with none, held inside the canvas.
            let centre = queue.markCentres.first { AgentArtwork.markKey($0.vendor) == AgentArtwork.markKey(vendor) }?.along
                ?? strip.width / 2
            let x = min(max(0, stripFrame.minX + centre - card.width / 2), canvas.width - card.width)
            root.presentationFrame = CGRect(x: x, y: strip.height + 6, width: card.width, height: card.height)
            root.cardScreenFrame = root.presentationFrame
        } else if open {
            let width = panelWidth + 2 * flare
            root.presentationFrame = CGRect(x: (canvas.width - width) / 2, y: 0, width: width, height: canvas.height)
        } else {
            root.presentationFrame = stripFrame
        }
        return root
    }
}
#endif
