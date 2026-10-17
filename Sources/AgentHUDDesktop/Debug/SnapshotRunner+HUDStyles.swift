import AppKit
import SwiftUI
import AgentHUDCore

#if DEBUG
/// The stills of an agent's style drafts and of the panel's widths, named `panel-layout-…`.
extension SnapshotRunner {
    /// A screen's resting HUD under an agent's style draft, drawn as `get_hud_screenshot` draws it with show "hud": a
    /// notch turned into a Simple panel, a Dynamic dock with larger logos, and the notch's glow in another style; that
    /// screen's panel with the bar that says the style waits; the panel at the narrowest and the widest width a layout
    /// sets; and the Display page of Settings while the first display has a style draft, and before and after a person
    /// resets its HUD style. They are drawn from a settings store of their own.
    static func saveHUDStyles(preferences: AgentHUDCore.Settings, folder: URL) {
        let suite = "app.agenthud.open.snapshot.layout-styles"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        settings.update { $0 = preferences }
        let store = demoStore(settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: DemoClock.now))
        let layout = settings.panelLayout
        let notch = ScreenMetrics(menuBar: 37, notch: CGSize(width: 185, height: 32))
        func hud(_ name: String, _ screen: String, hasNotch: Bool) {
            let renderer = HUDPictureRenderer(store: store, screen: screen, hasNotch: hasNotch, look: .standard,
                                              metrics: hasNotch ? notch : .fallback)
            save("panel-layout-\(name)", renderer.view(), folder: folder, scheme: .dark)
        }

        // A notch an agent turned into a Simple panel, laid along the top as Settings previews it.
        layout.proposeStyle(HUDStyle(mode: .simple, position: 0, simpleSize: 28), for: "snapshot-notch", hasNotch: true)
        hud("style-simple", "snapshot-notch", hasNotch: true)
        // Its panel says the style waits, and that it is this display's.
        save("panel-layout-style-bar", IslandScene(store: store, settings: settings, open: true, light: false, look: .standard,
                                                   screen: "snapshot-notch"),
             folder: folder, scheme: .dark)
        layout.discard(.screen("snapshot-notch"))

        // A Dynamic dock as saved, and with an agent's larger logos.
        settings.update { $0.screens["snapshot-dock"] = ScreenPlacement(mode: .logos, logoSize: 20) }
        hud("style-logos-saved", "snapshot-dock", hasNotch: false)
        layout.proposeStyle(HUDStyle(logoSize: 34, logoGap: 0.3), for: "snapshot-dock", hasNotch: false)
        hud("style-logos-large", "snapshot-dock", hasNotch: false)

        // The notch's glow in another style.
        layout.proposeStyle(HUDStyle(glow: HUDStyle.Glow(style: .ascii, effect: .ripple, brightness: 1)), for: "snapshot-notch",
                            hasNotch: true)
        hud("style-glow", "snapshot-notch", hasNotch: true)
        layout.discard(.screen("snapshot-notch"))

        // The panel at the narrowest and the widest width a layout sets.
        for width in [PanelStyle.widthRange.lowerBound, PanelStyle.widthRange.upperBound] {
            layout.propose(PanelLayout(style: PanelStyle(width: width)))
            layout.save()
            save("panel-layout-width-\(width)", IslandScene(store: store, settings: settings, open: true, light: false, look: .standard),
                 folder: folder, scheme: .dark)
        }
        // The narrowest layout beside the widest Simple panel on a left edge, whose strip the panel keeps clear: the panel
        // is widened so its content is as wide as the narrowest panel's, and nothing in it is cut.
        layout.propose(PanelLayout(style: PanelStyle(width: PanelStyle.widthRange.lowerBound)))
        layout.save()
        let strip = SimpleMeterConfig(items: ScreenAgents(store: store, listed: [], mode: .simple).meterItems,
                                      placement: ScreenPlacement(mode: .simple, edge: .left,
                                                                 simpleSize: ScreenPlacement.simpleSizeRange.upperBound))
        let side = PanelPictureRenderer(store: store, display: .init(
            insets: IslandRootView.dockInsets(edge: .left, collapsedSize: CGSize(width: strip.stripSize.width, height: 200)),
            maximumHeight: 2_000, look: .standard))
        save("panel-layout-width-\(PanelStyle.widthRange.lowerBound)-side-dock",
             side.card(masked: false, height: CGFloat(side.fit().height)).frame(width: side.width)
                .padding(20).background(Color(white: 0.10)),
             folder: folder, scheme: .dark)
        layout.reset()

        // Settings while the first display shows an agent's style.
        if let first = ScreensPane.attached().first {
            layout.proposeStyle(HUDStyle(mode: first.hasNotch ? .logos : .simple), for: first.key, hasNotch: first.hasNotch)
            saveDisplaySettings("panel-layout-settings-style-dark", settings: settings, store: store, folder: folder)
            layout.discard(.screen(first.key))

            // The HUD style's Reset to Default on a display that uses the shared layout and has no version before:
            // with a style of its own, a Simple panel or a Dynamic dock and a glow in another style, the button is
            // offered and Revert to Previous rests; after the click the display has the default style, the button rests
            // and Revert to Previous is offered to bring the style back.
            settings.update {
                $0.screens[first.key] = ScreenPlacement(mode: first.hasNotch ? .logos : .simple, logoSize: 30, simpleSize: 30)
                var glow = $0.glow
                glow.style = .dots
                $0.screenGlow[first.key] = glow
            }
            saveDisplaySettings("panel-layout-settings-style-reset-dark", settings: settings, store: store, folder: folder)
            layout.resetLook(first.key)
            saveDisplaySettings("panel-layout-settings-style-reset-done-dark", settings: settings, store: store, folder: folder)
        }
    }
}
#endif
