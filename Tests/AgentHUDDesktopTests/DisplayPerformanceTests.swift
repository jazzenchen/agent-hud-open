import XCTest
import AgentHUDCore
@testable import AgentHUDDesktop

final class DisplayPerformanceTests: XCTestCase {
    @MainActor
    func testPreviewReusesBitmapUntilRenderingInputsChange() throws {
        let cache = GlowImageCache()
        var settings = Settings()
        func render() throws -> GlowImage {
            let glow = GlowGeometry.compute(islandWidth: 240, islandHeight: 30,
                islandRadius: 13, range: settings.glowRange, blur: settings.glowBlur)
            return try XCTUnwrap(cache.render(glow: glow, islandSize: CGSize(width: 240, height: 30),
                islandRadius: 13, outwardOnly: settings.glowOutwardOnly, stops: GlowGradient.idleStops, scale: 2))
        }
        let initial = try render()
        settings.glowBrightness = 0.5
        settings.breathSeconds = 5
        settings.breathAmplitude = 0.8
        let opacityOnly = try render()
        XCTAssertTrue(initial.image === opacityOnly.image)

        settings.glowRange += 1
        let wider = try render()
        XCTAssertFalse(initial.image === wider.image)
        XCTAssertGreaterThan(wider.size.width, initial.size.width)

        settings.glowBlur += 1
        let feathered = try render()
        XCTAssertFalse(wider.image === feathered.image)
        XCTAssertGreaterThan(feathered.padding, wider.padding)

        settings.glowOutwardOnly.toggle()
        let soft = try render()
        XCTAssertFalse(feathered.image === soft.image)
        XCTAssertTrue(soft.image === (try render()).image)
    }

    @MainActor
    func testSliderChangesDoNotTriggerUnrelatedSettingsEffects() async throws {
        let domain = "app.agenthud.tests.display.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: domain)!
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = SettingsStore(defaults: defaults)
        var languageChanges = 0
        var appearanceChanges = 0
        var loginChanges = 0
        observeChanges({ [weak settings] in
            settings?.settings.language
        }, onChange: { languageChanges += 1 })
        observeChanges({ [weak settings] in
            settings?.settings.appearance
        }, onChange: { appearanceChanges += 1 })
        observeChanges({ [weak settings] in
            settings?.settings.launchAtLogin
        }, onChange: { loginChanges += 1 })

        for value in [0.2, 0.4, 0.6] {
            settings.update { $0.glowBrightness = value }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(languageChanges, 0, "Dragging a glow slider must not run the language refresh handler")
        XCTAssertEqual(appearanceChanges, 0)
        XCTAssertEqual(loginChanges, 0)
        XCTAssertEqual(SettingsStore(defaults: defaults).settings.glowBrightness, 0.6)

        settings.update { $0.language = .en; $0.appearance = .dark; $0.launchAtLogin.toggle() }
        try await Task.sleep(for: .milliseconds(10))
        XCTAssertEqual(languageChanges, 1)
        XCTAssertEqual(appearanceChanges, 1)
        XCTAssertEqual(loginChanges, 1)

        settings.update { $0.glowBlur = 12 }
        try await Task.sleep(for: .milliseconds(10))
        XCTAssertEqual(languageChanges, 1)
        settings.update { $0.language = .zhHans }
        try await Task.sleep(for: .milliseconds(10))
        XCTAssertEqual(languageChanges, 2, "Observation must stay armed after unrelated changes")
    }
}
