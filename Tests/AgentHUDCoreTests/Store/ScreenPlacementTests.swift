import XCTest
@testable import AgentHUDCore

final class ScreenPlacementTests: XCTestCase {
    func testLogoSizeAndEdgeDistanceBoundsAreIndependentOfSpacing() throws {
        for (size, distance, expectedSize, expectedDistance) in [
            (30.0, 0.0, 30.0, 0.0), (36, 20, 36, 20), (60, 20, 60, 20), (100, 100, 60, 20), (12, 0, 20, 0), (-1, -1, 20, 0)
        ] {
            let data = try JSONEncoder().encode(["logoSize": size, "logoEdgeDistance": distance, "gapScale": 0.35])
            let placement = try JSONDecoder().decode(ScreenPlacement.self, from: data)
            XCTAssertEqual(placement.logoSize, expectedSize)
            XCTAssertEqual(placement.logoEdgeDistance, expectedDistance)
            XCTAssertEqual(placement.gapScale, 0.35)
        }
    }

    func testSavedPlacementWithoutEdgeDistanceUsesTheDefault() throws {
        let placement = try JSONDecoder().decode(ScreenPlacement.self, from: Data(#"{"logoSize":24,"gapScale":0.2}"#.utf8))
        XCTAssertEqual(placement.logoSize, 24)
        XCTAssertEqual(placement.logoEdgeDistance, 2)
        XCTAssertEqual(placement.gapScale, 0.2)
    }

    @MainActor
    func testEachDisplaysLogoSizeAndEdgeDistanceSurviveSettingsReload() throws {
        let suite = "AgentHUDTests.ScreenPlacement.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = SettingsStore(defaults: defaults)
        let placements = [
            "one": ScreenPlacement(edge: .top, logoSize: 30, logoEdgeDistance: 0, gapScale: 0.3),
            "two": ScreenPlacement(edge: .right, logoSize: 36, logoEdgeDistance: 20, gapScale: 0.6),
        ]
        store.update { $0.screens = placements }
        XCTAssertEqual(SettingsStore(defaults: defaults).settings.screens, placements)
    }

    func testSimpleModeSettingsDecodeWithinTheirBoundsAndDefaultWhenAbsent() throws {
        let saved = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"mode":"simple","edge":"left","simpleSize":100,"simpleGapScale":0.9,"ringWidth":0.2,"showsRings":false}"#.utf8))
        XCTAssertEqual(saved.mode, .simple)
        XCTAssertEqual(saved.edge, .left)
        XCTAssertEqual(saved.simpleSize, 60)
        XCTAssertEqual(saved.simpleGapScale, 0.6)
        XCTAssertEqual(saved.ringWidth, 1)
        XCTAssertFalse(saved.showsRings)
        XCTAssertTrue(saved.showsNumbers)
        // The Simple panel once had a bot switch of its own; a placement saved with it still reads, without it.
        let lively = try JSONDecoder().decode(ScreenPlacement.self, from: Data(#"{"mode":"simple","simpleBots":true}"#.utf8))
        XCTAssertEqual(lively, ScreenPlacement(mode: .simple))

        let older = try JSONDecoder().decode(ScreenPlacement.self, from: Data(#"{"mode":"logos","logoSize":24}"#.utf8))
        XCTAssertEqual(older.simpleSize, 22)
        XCTAssertEqual(older.simpleGapScale, 0.45, "A placement saved before the Simple panel's spacing keeps the gap it had")
        XCTAssertEqual(older.ringWidth, 1.5)
        XCTAssertTrue(older.showsRings)
        XCTAssertTrue(older.showsNumbers)
        XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: JSONEncoder().encode(saved)), saved)
    }

    func testOnlyTheNotchIsNotDocked() {
        XCTAssertEqual(HUDMode.allCases.filter(\.isDocked), [.logos, .simple])
    }

    // MARK: The look

    /// A placement saved before the HUD had a look reads as Dark and Solid; a name a newer build saved, or a value of the
    /// wrong type, reads as the default and costs no other field; a look is written and read back as it was.
    func testTheLookDecodesFromOlderAndNewerSettings() throws {
        let older = try JSONDecoder().decode(ScreenPlacement.self, from: Data(#"{"mode":"simple","simpleSize":30}"#.utf8))
        XCTAssertEqual(older.appearance, .dark)
        XCTAssertEqual(older.material, .solid)
        XCTAssertEqual(older, ScreenPlacement(mode: .simple, simpleSize: 30))

        let newer = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"mode":"logos","logoSize":30,"appearance":"sepia","material":"paper"}"#.utf8))
        XCTAssertEqual(newer, ScreenPlacement(mode: .logos, logoSize: 30), "a name this build does not know is the default")
        let mistyped = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"edge":"left","appearance":1,"material":true}"#.utf8))
        XCTAssertEqual(mistyped, ScreenPlacement(edge: .left))

        let chosen = ScreenPlacement(mode: .simple, appearance: .system, material: .glass)
        let written = try JSONEncoder().encode(chosen)
        let fields = try XCTUnwrap(JSONSerialization.jsonObject(with: written) as? [String: Any])
        XCTAssertEqual(fields["appearance"] as? String, "system")
        XCTAssertEqual(fields["material"] as? String, "glass")
        XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: written), chosen)
        for hasNotch in [true, false] {
            XCTAssertEqual(ScreenPlacement.default(hasNotch: hasNotch).appearance, .dark)
            XCTAssertEqual(ScreenPlacement.default(hasNotch: hasNotch).material, .solid)
        }
    }

    /// One screen's look this build cannot read leaves that screen's other fields and every other screen as saved.
    func testAnUnreadableLookCostsNoOtherScreen() throws {
        let json = #"{"screens":{"A":{"mode":"simple","appearance":"neon"},"B":{"mode":"logos","edge":"left","material":"glass"}}}"#
        let settings = try JSONDecoder().decode(Settings.self, from: Data(json.utf8))
        XCTAssertEqual(settings.screens["A"], ScreenPlacement(mode: .simple))
        XCTAssertEqual(settings.screens["B"], ScreenPlacement(mode: .logos, edge: .left, material: .glass))
    }

    /// Each display's look survives a reload of the settings.
    @MainActor
    func testEachDisplaysLookSurvivesSettingsReload() throws {
        let suite = "AgentHUDTests.ScreenPlacement.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = SettingsStore(defaults: defaults)
        let placements = [
            "one": ScreenPlacement(mode: .simple, appearance: .light, material: .glass),
            "two": ScreenPlacement(mode: .notch, appearance: .system),
        ]
        store.update { $0.screens = placements }
        XCTAssertEqual(SettingsStore(defaults: defaults).settings.screens, placements)
    }

    /// The look a HUD draws is its own in every mode, the notch's at a hardware notch too, which keeps only the notch's
    /// row dark (`HUDLook.inNotchRow`, in the app): `system` as macOS is set, and glass solid under Reduce Transparency.
    func testTheLookDrawnFollowsMacOSAndReduceTransparencyInEveryMode() {
        for mode in HUDMode.allCases {
            let chosen = ScreenPlacement(mode: mode, appearance: .light, material: .glass)
            XCTAssertTrue(chosen.drawsLight(systemIsLight: false), "\(mode) draws its light look")
            XCTAssertTrue(chosen.drawsGlass(reducesTransparency: false), "\(mode) draws its glass")
            XCTAssertFalse(chosen.drawsGlass(reducesTransparency: true), "\(mode) under Reduce Transparency is solid")
        }

        let system = ScreenPlacement(appearance: .system)
        XCTAssertTrue(system.drawsLight(systemIsLight: true))
        XCTAssertFalse(system.drawsLight(systemIsLight: false))
        XCTAssertFalse(ScreenPlacement(appearance: .dark).drawsLight(systemIsLight: true))
        XCTAssertTrue(ScreenPlacement(appearance: .light).drawsLight(systemIsLight: false))

        let glass = ScreenPlacement(material: .glass)
        XCTAssertTrue(glass.drawsGlass(reducesTransparency: false))
        XCTAssertFalse(glass.drawsGlass(reducesTransparency: true), "Reduce Transparency draws it solid")
        XCTAssertFalse(ScreenPlacement().drawsGlass(reducesTransparency: false))
        XCTAssertFalse(ScreenPlacement().drawsLight(systemIsLight: true), "dark unless chosen")
    }

    // MARK: The transparency

    /// Glass's Transparency is 50 % unless chosen otherwise, on every display's starting point too, and a transparency
    /// is the slider's, held to 0 through 1, 0 to 100 %, whether it is given or read back, and kept as chosen: what is
    /// drawn of it is apart (`drawsTransparency(reducesTransparency:)`).
    func testGlassTransparencyStartsAtHalfAndIsHeldToItsRange() throws {
        XCTAssertEqual(ScreenPlacement.standardGlassTransparency, 0.5)
        XCTAssertEqual(ScreenPlacement.glassTransparencyRange, 0...1)
        XCTAssertEqual(ScreenPlacement.transparencyRange(for: .glass), ScreenPlacement.glassTransparencyRange)
        XCTAssertEqual(ScreenPlacement().glassTransparency, ScreenPlacement.standardGlassTransparency)
        for hasNotch in [true, false] {
            XCTAssertEqual(ScreenPlacement.default(hasNotch: hasNotch).glassTransparency, 0.5)
        }
        for (given, kept) in [(0.0, 0.0), (0.3, 0.3), (0.95, 0.95), (1.0, 1.0), (1.2, 1.0), (-0.2, 0.0), (Double.nan, 0.0)] {
            XCTAssertEqual(ScreenPlacement(material: .glass, glassTransparency: given).glassTransparency, kept, "\(given)")
        }
        for (saved, kept) in [(0.25, 0.25), (0.95, 0.95), (1, 1), (2, 1), (-1, 0)] {
            let placement = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
                #"{"material":"glass","glassTransparency":\#(saved)}"#.utf8))
            XCTAssertEqual(placement.glassTransparency, kept, "\(saved)")
            XCTAssertEqual(placement.material, .glass)
        }
    }

    /// A solid surface is opaque unless chosen otherwise, and its transparency is the slider's too, held to 0 through 1
    /// as glass's is, whether it is given or read back.
    func testSolidTransparencyStartsOpaqueAndIsHeldToTheSameRange() throws {
        XCTAssertEqual(ScreenPlacement.standardSolidTransparency, 0)
        XCTAssertEqual(ScreenPlacement.solidTransparencyRange, 0...1)
        XCTAssertEqual(ScreenPlacement.solidTransparencyRange, ScreenPlacement.glassTransparencyRange, "one range for both")
        XCTAssertEqual(ScreenPlacement.transparencyRange(for: .solid), ScreenPlacement.solidTransparencyRange)
        for hasNotch in [true, false] {
            XCTAssertEqual(ScreenPlacement.default(hasNotch: hasNotch).solidTransparency, 0)
        }
        for (given, kept) in [(0.0, 0.0), (0.3, 0.3), (0.95, 0.95), (1.0, 1.0), (1.2, 1.0), (-0.2, 0.0), (Double.nan, 0.0)] {
            XCTAssertEqual(ScreenPlacement(solidTransparency: given).solidTransparency, kept, "\(given)")
        }
        for (saved, kept) in [(0.25, 0.25), (1, 1), (2, 1), (-1, 0)] {
            let placement = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
                #"{"material":"solid","glassTransparency":2,"solidTransparency":\#(saved)}"#.utf8))
            XCTAssertEqual(placement.solidTransparency, kept, "\(saved)")
            XCTAssertEqual(placement.glassTransparency, 1, "glass's read to its own top")
        }
    }

    /// The transparency of the material chosen is held to 0 through 1, glass's and solid's alike, and leaves the other
    /// material's as it was.
    func testTheTransparencyIsHeldToTheRangeOfTheMaterialChosen() {
        var glass = ScreenPlacement(material: .glass, solidTransparency: 0.4)
        for (given, kept) in [(0.95, 0.95), (1.0, 1.0), (1.2, 1.0), (0.6, 0.6), (-0.1, 0.0)] {
            glass.transparency = given
            XCTAssertEqual(glass.glassTransparency, kept, "\(given)")
            XCTAssertEqual(glass.transparency, kept, "\(given)")
        }
        XCTAssertEqual(glass.solidTransparency, 0.4, "solid's kept")

        var solid = ScreenPlacement(material: .solid, glassTransparency: 0.3)
        for (given, kept) in [(0.95, 0.95), (1.0, 1.0), (1.2, 1.0), (0.6, 0.6), (-0.1, 0.0)] {
            solid.transparency = given
            XCTAssertEqual(solid.solidTransparency, kept, "\(given)")
            XCTAssertEqual(solid.transparency, kept, "\(given)")
        }
        XCTAssertEqual(solid.glassTransparency, 0.3, "glass's kept")
    }

    /// A placement saved before glass had a transparency reads as the standard one, 50 %, and costs no other field; a
    /// transparency is written under its own key and read back as it was, the top of 1 too, kept while the material is
    /// solid.
    func testGlassTransparencyDecodesFromOlderSettingsAndRoundTrips() throws {
        let older = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"mode":"simple","appearance":"light","material":"glass"}"#.utf8))
        XCTAssertEqual(older.glassTransparency, ScreenPlacement.standardGlassTransparency)
        XCTAssertEqual(older.glassTransparency, 0.5)
        XCTAssertEqual(older, ScreenPlacement(mode: .simple, appearance: .light, material: .glass))

        let chosen = ScreenPlacement(mode: .logos, material: .glass, glassTransparency: 0.2)
        let written = try JSONEncoder().encode(chosen)
        let fields = try XCTUnwrap(JSONSerialization.jsonObject(with: written) as? [String: Any])
        XCTAssertEqual(fields["glassTransparency"] as? Double, 0.2)
        XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: written), chosen)
        let top = ScreenPlacement(material: .glass, glassTransparency: 1)
        XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: JSONEncoder().encode(top)).glassTransparency, 1,
                       "the top, read back as it was")

        let solid = ScreenPlacement(material: .solid, glassTransparency: 0.7)
        XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: JSONEncoder().encode(solid)).glassTransparency, 0.7,
                       "kept for when glass is chosen again")
        XCTAssertNotEqual(solid, ScreenPlacement(), "a placement is its fields, the transparency among them")
    }

    // MARK: The transparency drawn

    /// Solid and Liquid Glass, clear or regular, draw three fifths of the Transparency chosen, and the material all of
    /// it, the owner's choice: regular glass with little of the look's colour beneath it turns light or dark by itself,
    /// piece by piece. The glass style a solid placement keeps for when glass is chosen again changes nothing.
    func testSolidAndLiquidGlassDrawThreeFifthsAndTheMaterialAllOfIt() {
        for style in HUDGlassStyle.allCases {
            XCTAssertEqual(ScreenPlacement.transparencyScale(material: .solid, glassStyle: style), 0.6, "solid kept \(style)")
        }
        XCTAssertEqual(ScreenPlacement.transparencyScale(material: .glass, glassStyle: .clear), 0.6, "clear")
        XCTAssertEqual(ScreenPlacement.transparencyScale(material: .glass, glassStyle: .regular), 0.6, "regular")
        XCTAssertEqual(ScreenPlacement.transparencyScale(material: .glass, glassStyle: .material), 1, "the material")
    }

    /// What a placement draws is its material's Transparency times that share: at the top, 100 %, 60 % for Solid, Clear
    /// and Regular and 100 % for the material; the standard glass's 50 %, on clear glass, 30 %; and none at all under
    /// Reduce Transparency, whatever was chosen. The other material's Transparency changes nothing, and the value saved
    /// stays the slider's.
    func testTheTransparencyDrawnIsTheChosenOneScaled() {
        for material in HUDMaterial.allCases {
            for style in HUDGlassStyle.allCases {
                let scale = material == .glass && style == .material ? 1.0 : 0.6
                for chosen in [0.0, 0.25, 0.5, 0.95, 1] {
                    var placement = ScreenPlacement(material: material, glassStyle: style)
                    placement.transparency = chosen
                    let what = "\(material) \(style) \(chosen)"
                    XCTAssertEqual(placement.drawsTransparency(reducesTransparency: false), chosen * scale,
                                   accuracy: 0.000_001, what)
                    XCTAssertEqual(placement.drawsTransparency(reducesTransparency: true), 0, "\(what): Reduce Transparency")
                    XCTAssertEqual(placement.transparency, chosen, "\(what): saved as chosen")
                }
            }
        }
        XCTAssertEqual(ScreenPlacement(material: .glass).drawsTransparency(reducesTransparency: false), 0.3,
                       accuracy: 0.000_001, "the standard glass, clear")
        XCTAssertEqual(ScreenPlacement(material: .glass, glassTransparency: 1).drawsTransparency(reducesTransparency: false),
                       0.6, "clear's top")
        XCTAssertEqual(ScreenPlacement(material: .glass, glassStyle: .regular, glassTransparency: 1)
            .drawsTransparency(reducesTransparency: false), 0.6, "regular's top")
        XCTAssertEqual(ScreenPlacement(material: .glass, glassStyle: .material, glassTransparency: 1)
            .drawsTransparency(reducesTransparency: false), 1, "the material's top, the bare material")
        XCTAssertEqual(ScreenPlacement(solidTransparency: 1).drawsTransparency(reducesTransparency: false), 0.6, "solid's top")
        XCTAssertEqual(ScreenPlacement(solidTransparency: 0.5).drawsTransparency(reducesTransparency: false), 0.3,
                       accuracy: 0.000_001)
        XCTAssertEqual(ScreenPlacement().drawsTransparency(reducesTransparency: false), 0, "a solid HUD opaque at first")
        XCTAssertEqual(ScreenPlacement(material: .glass, glassTransparency: 0.2, solidTransparency: 1)
            .drawsTransparency(reducesTransparency: false), 0.12, accuracy: 0.000_001, "glass's own, solid's apart")
        XCTAssertEqual(ScreenPlacement(material: .solid, glassStyle: .material, glassTransparency: 1, solidTransparency: 0.5)
            .drawsTransparency(reducesTransparency: false), 0.3, accuracy: 0.000_001, "solid's own, at solid's share")
    }

    // MARK: What glass is made of

    /// Glass is clear Liquid Glass unless chosen otherwise, on every display's starting point too; the three styles are
    /// saved by their own names.
    func testTheGlassStyleIsClearUnlessChosen() {
        XCTAssertEqual(HUDGlassStyle.allCases, [.clear, .regular, .material])
        XCTAssertEqual(HUDGlassStyle.allCases.map(\.rawValue), ["clear", "regular", "material"])
        XCTAssertEqual(ScreenPlacement().glassStyle, .clear)
        XCTAssertEqual(ScreenPlacement(material: .glass).glassStyle, .clear)
        for hasNotch in [true, false] {
            XCTAssertEqual(ScreenPlacement.default(hasNotch: hasNotch).glassStyle, .clear, "\(hasNotch)")
        }
    }

    /// Each style is written under its own key and read back as it was, on glass, and on solid, which keeps it for when
    /// glass is chosen again; the style is a field of the placement like any other.
    func testEachGlassStyleRoundTrips() throws {
        for style in HUDGlassStyle.allCases {
            for material in HUDMaterial.allCases {
                let chosen = ScreenPlacement(mode: .simple, appearance: .light, material: material, glassStyle: style,
                                             glassTransparency: 0.3)
                let written = try JSONEncoder().encode(chosen)
                let fields = try XCTUnwrap(JSONSerialization.jsonObject(with: written) as? [String: Any])
                XCTAssertEqual(fields["glassStyle"] as? String, style.rawValue, "\(style) on \(material)")
                XCTAssertEqual(try JSONDecoder().decode(ScreenPlacement.self, from: written), chosen, "\(style) on \(material)")
            }
        }
        XCTAssertNotEqual(ScreenPlacement(glassStyle: .regular), ScreenPlacement(), "a placement is its fields, the style among them")
    }

    /// A placement saved before glass had a style reads as clear glass, and a style a newer build saved, or a value of the
    /// wrong type, reads as clear too, costing the placement none of its other fields.
    func testTheGlassStyleDecodesFromOlderAndNewerSettings() throws {
        let older = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"mode":"logos","appearance":"light","material":"glass","glassTransparency":0.2}"#.utf8))
        XCTAssertEqual(older.glassStyle, .clear)
        XCTAssertEqual(older, ScreenPlacement(mode: .logos, appearance: .light, material: .glass, glassTransparency: 0.2))

        let newer = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"mode":"simple","edge":"left","material":"glass","glassStyle":"frosted-xyz","glassTransparency":0.7}"#.utf8))
        XCTAssertEqual(newer.glassStyle, .clear, "a name this build does not know is clear")
        XCTAssertEqual(newer, ScreenPlacement(mode: .simple, edge: .left, material: .glass, glassTransparency: 0.7))
        let mistyped = try JSONDecoder().decode(ScreenPlacement.self, from: Data(
            #"{"mode":"simple","material":"glass","glassStyle":2}"#.utf8))
        XCTAssertEqual(mistyped, ScreenPlacement(mode: .simple, material: .glass))
    }

    /// One screen's glass style this build cannot read leaves that screen's other fields and every other screen as saved.
    func testAnUnreadableGlassStyleCostsNoOtherScreen() throws {
        let json = #"{"screens":{"A":{"mode":"simple","material":"glass","glassStyle":"frosted-xyz"},"#
            + #""B":{"mode":"logos","material":"glass","glassStyle":"material"}}}"#
        let settings = try JSONDecoder().decode(Settings.self, from: Data(json.utf8))
        XCTAssertEqual(settings.screens["A"], ScreenPlacement(mode: .simple, material: .glass))
        XCTAssertEqual(settings.screens["B"], ScreenPlacement(mode: .logos, material: .glass, glassStyle: .material))
    }

    /// The style is kept while the material is solid, through a change of material and a reload of the settings, and is
    /// what glass is made of once glass is chosen again.
    @MainActor
    func testTheGlassStyleIsKeptWhileTheMaterialIsSolid() throws {
        var placement = ScreenPlacement(mode: .simple, material: .glass, glassStyle: .regular)
        placement.material = .solid
        XCTAssertEqual(placement.glassStyle, .regular, "kept while solid")
        placement.transparency = 0.4
        XCTAssertEqual(placement.glassStyle, .regular, "solid's transparency leaves it alone")

        let suite = "AgentHUDTests.ScreenPlacement.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = SettingsStore(defaults: defaults)
        store.update { $0.screens = ["one": placement, "two": ScreenPlacement(material: .glass, glassStyle: .material)] }
        let reloaded = SettingsStore(defaults: defaults).settings.screens
        XCTAssertEqual(reloaded["one"]?.glassStyle, .regular, "kept through a reload")
        XCTAssertEqual(reloaded["one"]?.material, .solid)
        XCTAssertEqual(reloaded["two"]?.glassStyle, .material)

        placement.material = .glass
        XCTAssertEqual(placement.glassStyle, .regular, "glass made of it again")
        XCTAssertEqual(placement.transparency, ScreenPlacement.standardGlassTransparency, "and glass's own transparency")
    }
}
