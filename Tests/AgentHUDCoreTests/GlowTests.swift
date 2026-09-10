import XCTest
@testable import AgentHUDCore

final class GlowGradientTests: XCTestCase {
    func testStopLocationsAreCentredPerAgent() {
        let stops = GlowGradient.stops(levels: [.ok, .warning, .ok, .critical])
        XCTAssertEqual(stops.map(\.location), [0.125, 0.375, 0.625, 0.875])
        XCTAssertEqual(stops.map(\.color.hexString), ["#3ddc84", "#ffd23f", "#3ddc84", "#ff453a"])
    }

    func testSingleAgentSitsInTheMiddle() {
        XCTAssertEqual(GlowGradient.stops(levels: [.ok]).map(\.location), [0.5])
    }

    func testEmptyLevelsFallBackToIdle() {
        XCTAssertEqual(GlowGradient.stops(levels: []), GlowGradient.idleStops)
    }

    func testCSSMatchesDesignFormat() {
        let css = GlowGradient.css(GlowGradient.stops(levels: [.ok, .critical]))
        XCTAssertEqual(css, "linear-gradient(90deg,#3ddc84 25.0%,#ff453a 75.0%)")
    }
}

final class GlowAppearanceTests: XCTestCase {
    func testActiveAgentsBreatheWithDefaults() {
        let a = GlowAppearance.resolve(levels: [.ok, .warning], paused: false, anyAgentActive: true, settings: Settings())
        XCTAssertFalse(a.hidden)
        XCTAssertTrue(a.breathing)
        XCTAssertEqual(a.peakOpacity, 0.9, accuracy: 0.001)
        XCTAssertEqual(a.troughOpacity, 0.36, accuracy: 0.001)
        XCTAssertEqual(a.breathSeconds, 3)
        XCTAssertEqual(a.stops.count, 2)
    }

    func testPausedIsGreyAndStill() {
        let a = GlowAppearance.resolve(levels: [.ok], paused: true, anyAgentActive: true, settings: Settings())
        XCTAssertEqual(a.stops, GlowGradient.idleStops)
        XCTAssertFalse(a.breathing)
        XCTAssertEqual(a.peakOpacity, GlowAppearance.idleOpacity)
        XCTAssertFalse(a.hidden)
    }

    func testIdleKeepsColoursAndOnlyStopsBreathing() {
        let idle = GlowAppearance.resolve(levels: [.ok, .critical], paused: false, anyAgentActive: false, settings: Settings())
        XCTAssertFalse(idle.hidden)
        XCTAssertFalse(idle.breathing)
        XCTAssertEqual(idle.stops, GlowGradient.stops(levels: [.ok, .critical]), "a quiet stretch never greys the glow")
        XCTAssertEqual(idle.peakOpacity, 0.9, accuracy: 0.001)
    }

    func testNoDataIsGreyUntilTheFirstQuotaArrives() {
        let a = GlowAppearance.resolve(levels: [], paused: false, anyAgentActive: true, settings: Settings())
        XCTAssertEqual(a.stops, GlowGradient.idleStops)
        XCTAssertFalse(a.hidden)
    }

    func testLegacyIdleKeysAreIgnored() throws {
        let json = #"{"idleBehavior":"hide","breatheOnlyWhenActive":false,"glowRange":4}"#
        let decoded = try JSONDecoder().decode(Settings.self, from: Data(json.utf8))
        XCTAssertEqual(decoded.glowRange, 4)
        let a = GlowAppearance.resolve(levels: [.ok], paused: false, anyAgentActive: false, settings: decoded)
        XCTAssertFalse(a.hidden, "settings saved with the old idle options no longer hide or grey the glow")
    }
}

final class GlowGeometryTests: XCTestCase {
    func testAroundMatchesDesignFormula() {
        let g = GlowGeometry.compute(islandWidth: 380, islandHeight: 44, islandRadius: 22, range: 14, blur: 8)
        XCTAssertEqual(g.width, 408)
        XCTAssertEqual(g.height, 44 + 14 + 24)
        XCTAssertEqual(g.topOffset, -24)
        XCTAssertEqual(g.cornerRadius, 36)
        XCTAssertEqual(g.sideInset, 14)
        XCTAssertEqual(g.visibleHeight, 58)
    }
}
