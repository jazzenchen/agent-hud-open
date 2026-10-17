import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// The app's own settings under `draft_hud`'s `app`: a draft held beside the screens' drafts and drawn over the
/// settings at once for every screen, whatever screen the call names, kept by the Save in any panel's bar and dropped by
/// its Discard, by a relaunch and, a field at a time, by a person's own change of that setting in Settings. A proposal is
/// whole, refused whole with every reason, and one that leaves the app out leaves its draft as it is. A call with only
/// app leaves every layout and style, and every hold, as they are. No tool writes the settings.
final class HUDToolsAppTests: HUDToolsCase {
    private let builtIn = PanelScreen(key: "BUILT-IN-UUID", name: "Built-in display", mode: .notch, edge: .top, hasNotch: true,
                                      agents: ["Claude", "Codex", "DeepSeek"],
                                      systemIsLight: false, reducesTransparency: false)
    private let second = PanelScreen(key: "SECOND-UUID", name: "Display 2", mode: .logos, edge: .right, hasNotch: false,
                                     agents: ["Claude", "Codex", "Cursor"],
                                     systemIsLight: false, reducesTransparency: false, hasPointer: true)
    private let key = "SECOND-UUID"
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    private let kept = "The app's own settings show as an unsaved draft on every display: the user keeps it by hovering the "
        + "Agent HUD on any of them and pressing Save in the panel, which keeps whatever else that panel shows unsaved "
        + "with it, or drops it with Discard; it is dropped when Agent HUD restarts, and a setting of it also when the "
        + "user changes that setting in Settings."
    private let alone = " A call with only app leaves every layout and every screen's HUD style as it is."

    private lazy var screenTools = HUDTools(store: store, launch: 0, screens: { [unowned self] in [builtIn, second] })

    override func setUp() {
        super.setUp()
        MainActor.assumeIsolated {
            // The holds run on a clock that stands still, so a test reads the very time a hold was given.
            layout.holds.clock = { [unowned self] in now }
            layout.holds.sleep = { _ in try? await Task.sleep(for: .seconds(3_600)) }
        }
    }

    override func tearDown() {
        MainActor.assumeIsolated { layout.holds.endAll() }
        super.tearDown()
    }

    private func call(_ arguments: [String: JSONValue], line: UInt = #line) throws -> [String: JSONValue] {
        try call(MCPTools.draftHUD, arguments, line: line)
    }

    private func call(_ tool: String, _ arguments: [String: JSONValue], line: UInt = #line) throws -> [String: JSONValue] {
        guard case .result(let result, _) = screenTools.answer(MCPToolRequest(tool: tool, arguments: arguments)) else {
            XCTFail("\(tool) was refused", line: line)
            throw CancellationError()
        }
        return result
    }

    private func refused(_ arguments: [String: JSONValue], line: UInt = #line) throws -> String {
        let before = (layout.revision, layout.appDraft, settings.settings)
        guard case .failure(let sentence) = screenTools.answer(MCPToolRequest(tool: MCPTools.draftHUD,
                                                                             arguments: arguments)) else {
            XCTFail("the proposal was taken", line: line)
            throw CancellationError()
        }
        XCTAssertEqual(layout.revision, before.0, "a refused proposal changes nothing", line: line)
        XCTAssertEqual(layout.appDraft, before.1, line: line)
        XCTAssertEqual(settings.settings, before.2, line: line)
        XCTAssertTrue(sentence.hasPrefix("Nothing was changed. "), sentence, line: line)
        return sentence
    }

    private func note(_ panel: [String: JSONValue]) -> String { panel["note"]?.stringValue ?? "" }

    // MARK: A draft drawn at once

    /// An app proposal is the app's draft: drawn over the settings at once wherever the app reads them, read back by
    /// get_hud as the app draws it with appUnsaved, and shown in the bar of every screen's panel, while the settings,
    /// the layouts and the styles stay as they are and nothing is written.
    func testAnAppProposalIsADraftDrawnOverTheSettings() throws {
        changes = 0
        let saved = settings.settings
        let panel = try call(["app": .object(["botLogos": .bool(false), "showMenuBarIcon": .bool(false),
                                              "hover": .object(["delayMs": .integer(600), "pressOptionToOpen": .bool(true)])])])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false, showMenuBarIcon: false, pressOptionToOpen: true, delayMs: 600))
        let drawn = settings.effectiveSettings
        XCTAssertFalse(drawn.botLogos)
        XCTAssertFalse(drawn.showMenuBarIcon)
        XCTAssertTrue(drawn.requiresOptionToOpen)
        XCTAssertEqual(drawn.hoverDelayMs, 600)
        XCTAssertEqual(drawn.hoverDelay, 0.6, accuracy: 0.000_1, "the hover waits as long as the draft says")
        XCTAssertEqual(drawn.collapseDelayMs, saved.collapseDelayMs, "a field it leaves out is drawn as saved")
        XCTAssertEqual(settings.settings, saved, "the settings stay the saved truth")
        XCTAssertEqual(changes, 0, "a tool writes nothing")

        XCTAssertEqual(panel["app"]?["botLogos"], .bool(false), "read back as the app draws it")
        XCTAssertEqual(panel["app"]?["showMenuBarIcon"], .bool(false))
        XCTAssertEqual(panel["app"]?["hover"]["delayMs"], .integer(600))
        XCTAssertEqual(panel["appUnsaved"], .bool(true))
        XCTAssertEqual(panel["unsaved"], .bool(true), "the shared panel's bar shows the app's draft")
        XCTAssertEqual(note(panel), kept + " With the menu-bar icon hidden, Save, Discard and the Settings button stay in "
                       + "the HUD's panel. With Press Option to open on, the user holds Option while hovering the HUD to "
                       + "open that panel." + alone)
        XCTAssertNil(layout.draft, "no layout draft")
        XCTAssertTrue(layout.hasDraft(on: key), "every screen's panel has the bar, with Save and Discard")
        XCTAssertTrue(layout.hasDraft(on: "screen:any"))
        XCTAssertTrue(layout.hasDraft(on: nil))
        let screens = (try call(MCPTools.getHUD, [:])["screens"]?.arrayValue ?? []).compactMap(\.objectValue)
        XCTAssertEqual(screens.map { $0["unsaved"] }, [.bool(true), .bool(true)])
        XCTAssertEqual(screens.map { $0["styleUnsaved"] }, [.bool(false), .bool(false)])

        // The app's settings are every screen's: a call for one screen drafts the same.
        layout.discardApp()
        _ = try call(["screen": .string(key), "app": .object(["botLogos": .bool(false)])])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        XCTAssertFalse(settings.effectiveSettings.botLogos)
    }

    /// A proposal is whole: a field it leaves out is what is saved, so a new one replaces the draft and one that sets
    /// nothing drops it. A field set to what is saved changes nothing, so the app get_hud lists, the read-only language
    /// included, sent back, keeps the draft as it is, and the app as saved leaves none.
    func testAnAppProposalIsWholeAndTheListedAppCanBeSentBack() throws {
        _ = try call(["app": .object(["botLogos": .bool(false)])])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        let listed = try XCTUnwrap(try call(MCPTools.getHUD, [:])["app"])
        let revision = layout.revision
        XCTAssertEqual(try call(["app": listed])["app"], listed)
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false), "the draft as it was")
        XCTAssertEqual(layout.revision, revision, "and nothing changed")

        _ = try call(["app": .object(["hover": .object(["collapseDelayMs": .integer(500)])])])
        XCTAssertEqual(layout.appDraft, AppDraft(collapseDelayMs: 500), "Bot logos is back to what is saved")
        XCTAssertTrue(settings.effectiveSettings.botLogos)

        let saved = try call(["app": .object(["botLogos": .bool(true), "appearance": .string("system"),
                                              "hover": .object(["collapseDelayMs": .integer(200)])])])
        XCTAssertNil(layout.appDraft, "what is saved, spelled out, is no draft")
        XCTAssertEqual(saved["appUnsaved"], .bool(false))
        XCTAssertEqual(note(saved), "The app's own settings are as they are saved, so there is no draft of them." + alone)

        _ = try call(["app": .object(["botLogos": .bool(false)])])
        _ = try call(["app": .object([:])])
        XCTAssertNil(layout.appDraft, "an app that sets nothing drops the draft")
        XCTAssertEqual(changes, 0)
    }

    /// The app's draft is the person's to keep: a call that leaves the app out, a layout, a style, from or a drop, leaves
    /// it as it is, and discard_hud_draft says so.
    func testACallThatLeavesTheAppOutLeavesItsDraft() throws {
        _ = try call(["app": .object(["botLogos": .bool(false)])])
        let layoutAndStyle = try call(["screen": .string(key), "hiddenSections": .array([.string("tokens")]),
                                       "hud": .object(["logoSize": .integer(30)])])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        XCTAssertFalse(note(layoutAndStyle).contains("app's own settings"), note(layoutAndStyle))
        _ = try call(["from": .string("default")])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        let dropped = try call(MCPTools.discardHUDDraft, ["screen": .string(key)])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        XCTAssertTrue(note(dropped).hasSuffix(" The app's own settings keep their unsaved draft, which only "
                                              + "draft_hud's app changes: an app that sets nothing drops it."),
                      note(dropped))
        XCTAssertEqual(dropped["unsaved"], .bool(true), "the screen's bar still shows the app's draft")
        _ = try call(MCPTools.discardHUDDraft, [:])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        let finished = try call(MCPTools.finishHUDEdit, [:])
        XCTAssertTrue(note(finished).hasSuffix(" Drafts wait for the user to press Save in the panel."), note(finished))
    }

    /// An app beside a layout, a style or from proposes both, and the note says what each did.
    func testAnAppBesideALayoutOrFromProposesBoth() throws {
        let both = try call(["screen": .string(key), "hiddenSections": .array([.string("tokens")]),
                             "hud": .object(["logoSize": .integer(30)]), "app": .object(["botLogos": .bool(false)])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hiddenSections: [.tokens])))
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(logoSize: 30))
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        XCTAssertTrue(note(both).hasSuffix(" " + kept), note(both))

        let from = try call(["from": .string("default"), "app": .object(["matchScreen": .bool(true)])])
        XCTAssertNil(layout.draft, "the default view is what the shared layout has saved")
        XCTAssertEqual(layout.appDraft, AppDraft(matchScreen: true))
        XCTAssertTrue(note(from).hasSuffix(" " + kept), note(from))

        // A style alone with an app leaves the layout as it is, as a style alone does.
        _ = try call(["screen": .string(key), "hud": .object(["logoSize": .integer(24)]),
                      "app": .object(["botLogos": .bool(false)])])
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hiddenSections: [.tokens])), "the layout draft as it was")
        XCTAssertEqual(layout.styleDraft(of: key), HUDStyle(logoSize: 24))
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
    }

    // MARK: Refusals

    /// A proposal the app cannot draw as asked is refused whole, with every reason, beside the layout's: a property the
    /// app or its hover does not have, a value of the wrong kind, a name this build does not know, a delay outside
    /// Settings' range or between its steps, a read-only language that is not as listed, and an appearance while the
    /// windows match their screen.
    func testAnAppTheAppCannotDrawIsRefusedWholeWithEveryReason() throws {
        let sentence = try refused(["order": .array([.string("Zed")]), "app": .object([
            "theme": .string("dark"), "delayMs": .integer(300), "botLogos": .string("off"),
            "appearance": .string("sepia"), "language": .string("en"), "writtenIn": .string("zh-Hans"),
            "hover": .object(["speed": .integer(2), "delayMs": .integer(425), "collapseDelayMs": .integer(1_600),
                              "openAtTopEdge": .integer(1)]),
        ])])
        for reason in [
            "No block has the key \"Zed\"",
            "app has no property \"theme\"; the app takes appearance, matchScreen, botLogos, showMenuBarIcon and hover, "
                + "and language and writtenIn as get_hud lists them.",
            "app has no property \"delayMs\"; the app takes appearance, matchScreen, botLogos, showMenuBarIcon and hover, "
                + "and language and writtenIn as get_hud lists them. Hover behavior's delayMs is app.hover.delayMs.",
            "app.botLogos is true or false.",
            "app.appearance is one of \"system\", \"dark\", \"light\".",
            "app.hover has no property \"speed\"; the hover takes pressOptionToOpen, openAtTopEdge, delayMs and "
                + "collapseDelayMs.",
            "app.hover.openAtTopEdge is true or false.",
            "app.hover.delayMs: delayMs is a whole number of milliseconds from 0 to 1500 in steps of 50, as Settings sets "
                + "it, which 425 is not.",
            "app.hover.collapseDelayMs: collapseDelayMs is a whole number of milliseconds from 0 to 1500 in steps of 50, "
                + "as Settings sets it, which 1600 is not.",
            "app.language is read only: it is the Language chosen in Settings, which only Settings changes, \"system\" as "
                + "get_hud lists it. Leave it out, or give it as listed.",
            "app.writtenIn is read only: it is the language the panel's names are written in now, \"en\" as get_hud "
                + "lists it. Leave it out, or give it as listed.",
        ] {
            XCTAssertTrue(sentence.contains(reason), "\(reason)\n\(sentence)")
        }
        XCTAssertTrue(try refused(["app": .string("dark")]).hasSuffix("app is an object."))
        XCTAssertTrue(try refused(["app": .object(["hover": .bool(true)])]).hasSuffix("app.hover is an object."))
        XCTAssertTrue(try refused(["app": .object(["hover": .object(["delayMs": .number(37.5)])])])
            .hasSuffix("app.hover.delayMs is a whole number of milliseconds."))
        XCTAssertTrue(try refused(["app": .object(["hover": .object(["delayMs": .integer(-50)])])])
            .contains("which -50 is not."))
        XCTAssertTrue(try refused(["from": .string("default"), "app": .object(["botLogos": .integer(0)])])
            .hasSuffix("app.botLogos is true or false."), "beside from too")
    }

    /// With the windows matching their screen, as Match screen keeps them or the proposal would, the Appearance is not
    /// drawn, as Settings greys it out; turning Match screen off beside it is taken, and a repeat of what is saved is no
    /// change.
    func testAnAppearanceWhileTheWindowsMatchTheirScreenIsRefused() throws {
        let reason = "app.appearance: appearance sets the app's windows and the menu-bar menu, and with matchScreen on, "
            + "as it would be, each takes the look of the HUD on its screen instead, which is why Settings greys the "
            + "Appearance out. Leave it out, or set matchScreen to false."
        XCTAssertTrue(try refused(["app": .object(["appearance": .string("dark"), "matchScreen": .bool(true)])])
            .hasSuffix(reason))
        settings.update { $0.appearanceFollowsScreen = true }
        changes = 0
        XCTAssertTrue(try refused(["app": .object(["appearance": .string("light")])]).hasSuffix(reason))
        _ = try call(["app": .object(["appearance": .string("system")])])
        XCTAssertNil(layout.appDraft, "the Appearance as saved asks nothing")
        _ = try call(["app": .object(["appearance": .string("light"), "matchScreen": .bool(false)])])
        XCTAssertEqual(layout.appDraft, AppDraft(appearance: .light, matchScreen: false))
        XCTAssertEqual(changes, 0)
    }

    /// The settings read a delay as it was written, so one saved outside Settings' range or between its steps, as get_hud
    /// lists it, can be sent back and changes nothing.
    func testADelaySavedOffItsStepsCanBeSentBack() throws {
        settings.update {
            $0.hoverDelayMs = 425
            $0.collapseDelayMs = 2_000
        }
        let listed = try XCTUnwrap(try call(MCPTools.getHUD, [:])["app"])
        XCTAssertEqual(listed["hover"]["delayMs"], .integer(425))
        XCTAssertEqual(listed["hover"]["collapseDelayMs"], .integer(2_000))
        _ = try call(["app": listed])
        XCTAssertNil(layout.appDraft)
        _ = try call(["app": .object(["hover": .object(["delayMs": .integer(450), "collapseDelayMs": .integer(2_000)])])])
        XCTAssertEqual(layout.appDraft, AppDraft(delayMs: 450))
    }

    // MARK: A person's choice

    /// Save in any panel's bar writes the app's draft into the settings, with whatever else that panel shows unsaved, and
    /// Discard drops it; a relaunch drops it too, as it drops every draft.
    func testSaveWritesTheAppsDraftAndDiscardDropsIt() throws {
        _ = try call(["screen": .string(key), "hud": .object(["logoSize": .integer(30)]),
                      "app": .object(["botLogos": .bool(false), "hover": .object(["collapseDelayMs": .integer(800)])])])
        changes = 0
        layout.savePanel(on: key)
        XCTAssertNil(layout.appDraft)
        XCTAssertNil(layout.styleDraft(of: key))
        XCTAssertFalse(settings.settings.botLogos, "written into the settings")
        XCTAssertEqual(settings.settings.collapseDelayMs, 800)
        XCTAssertEqual(settings.settings.screens[key]?.logoSize, 30, "with the screen's style")
        XCTAssertEqual(changes, 2, "the style in one change and the app's settings in one")
        XCTAssertFalse(layout.hasDraft(on: key))

        _ = try call(["app": .object(["showMenuBarIcon": .bool(false)])])
        changes = 0
        layout.discardPanel(on: builtIn.key)
        XCTAssertNil(layout.appDraft, "another screen's Discard drops it too, since every bar shows it")
        XCTAssertTrue(settings.settings.showMenuBarIcon)
        XCTAssertTrue(settings.effectiveSettings.showMenuBarIcon)
        XCTAssertEqual(changes, 0)

        _ = try call(["app": .object(["showMenuBarIcon": .bool(false)])])
        layout.savePanel(on: nil)
        XCTAssertFalse(settings.settings.showMenuBarIcon, "the shared panel's Save keeps it too")

        _ = try call(["app": .object(["botLogos": .bool(true)])])
        let relaunched = SettingsStore(defaults: UserDefaults(suiteName: suite)!, defaultAgents: DemoData.agents)
        XCTAssertNil(relaunched.panelLayout.appDraft, "a draft is dropped on relaunch")
        XCTAssertFalse(relaunched.effectiveSettings.botLogos, "and the app draws what was saved")
    }

    /// A person who changes one of the settings in Settings wins: that field is dropped from the draft and the change is
    /// made to the settings as saved; the draft's other fields wait as they were. A setting no field sets leaves the draft.
    func testAPersonsEditInSettingsDropsThatFieldAlone() throws {
        _ = try call(["app": .object(["botLogos": .bool(false), "hover": .object(["delayMs": .integer(800)])])])
        settings.updateSetting(\.glowOutwardOnly) { $0.glowOutwardOnly.toggle() }
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false, delayMs: 800), "a setting the draft does not set")

        settings.updateSetting(\.hoverDelayMs) { $0.hoverDelayMs = 100 }
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        XCTAssertEqual(settings.settings.hoverDelayMs, 100)
        XCTAssertEqual(settings.effectiveSettings.hoverDelayMs, 100)

        // Setting it to what is saved still drops the field: the person chose what the app draws.
        settings.updateSetting(\.botLogos) { $0.botLogos = true }
        XCTAssertNil(layout.appDraft)
        XCTAssertTrue(settings.effectiveSettings.botLogos)
        XCTAssertFalse(layout.hasDraft(on: key))
        XCTAssertEqual(AppDraft.Field(setting: \Settings.appearanceFollowsScreen), .matchScreen)
        XCTAssertNil(AppDraft.Field(setting: \Settings.language), "the language is no field")
    }

    /// A person who turns Match screen on leaves the windows to their screen's look, which Settings greys the Appearance
    /// out for, so the draft's appearance goes with it: get_hud never lists an app that a proposal would refuse, and the
    /// app listed, sent back, is taken.
    func testMatchScreenTurnedOnInSettingsDropsTheDraftsAppearance() throws {
        _ = try call(["app": .object(["appearance": .string("dark"), "botLogos": .bool(false)])])
        settings.updateSetting(\.appearanceFollowsScreen) { $0.appearanceFollowsScreen = true }
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false))
        let listed = try XCTUnwrap(try call(MCPTools.getHUD, [:])["app"])
        XCTAssertEqual(listed["appearance"], .string("system"))
        XCTAssertEqual(listed["matchScreen"], .bool(true))
        _ = try call(["app": listed])
        XCTAssertEqual(layout.appDraft, AppDraft(botLogos: false), "the app listed, sent back, keeps the draft as it is")

        // A draft that turns Match screen off allows an appearance; the person's switch drops both.
        _ = try call(["app": .object(["appearance": .string("light"), "matchScreen": .bool(false)])])
        settings.updateSetting(\.appearanceFollowsScreen) { $0.appearanceFollowsScreen = true }
        XCTAssertNil(layout.appDraft)
        XCTAssertTrue(settings.effectiveSettings.appearanceFollowsScreen)
    }

    // MARK: Holds

    /// A call with only app opens nothing, and leaves a hold as it is: it neither starts one, renews one nor pauses one,
    /// since nothing it changes lies under the panel and the bar of a panel already open shows its draft. Beside a layout
    /// it holds as a layout edit does, and beside a style alone it pauses as a style alone does.
    func testAnAppAloneLeavesTheHoldAsItIs() throws {
        let holds = layout.holds
        _ = try call(["app": .object(["botLogos": .bool(false)])])
        _ = try call(["screen": .string(key), "app": .object(["botLogos": .bool(false)])])
        XCTAssertTrue(holds.screens.isEmpty, "nothing is opened")

        _ = try call(["screen": .string(key), "hiddenSections": .array([.string("tokens")])])
        let given = try XCTUnwrap(holds.hold(on: key))
        layout.holds.clock = { [unowned self] in now.addingTimeInterval(30) }
        let panel = try call(["screen": .string(key), "revision": .integer(Int64(layout.revision)),
                              "app": .object(["showMenuBarIcon": .bool(false)])])
        XCTAssertEqual(holds.hold(on: key), given, "not renewed and not paused")
        XCTAssertEqual(panel["screens"]?.arrayValue?.last?["holdPaused"], .bool(false))
        XCTAssertEqual(layout.draft(of: key), .own(PanelLayout(hiddenSections: [.tokens])), "the layout draft as it was")

        _ = try call(["screen": .string(key), "hud": .object(["logoSize": .integer(30)]),
                      "app": .object(["showMenuBarIcon": .bool(false)])])
        XCTAssertEqual(holds.hold(on: key)?.paused, true, "a style alone beside an app pauses the hold")
        _ = try call(["screen": .string(key), "hiddenSections": .array([.string("sessions")]),
                      "app": .object(["showMenuBarIcon": .bool(false)])])
        XCTAssertEqual(holds.hold(on: key)?.paused, false, "a layout beside an app holds the panel again")
        XCTAssertEqual(holds.hold(on: key)?.until, now.addingTimeInterval(30 + PanelHolds.duration))
    }

    // MARK: The store

    /// The store holds the app's draft in memory, counts a revision for each change of it and none for a repeat, and
    /// without settings to write into, Save drops it alone.
    func testTheStoreCountsTheAppsDraftAsADraft() throws {
        let suite = "app.agenthud.tests.app-draft.\(UUID().uuidString)"
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let bare = PanelLayoutStore(defaults: UserDefaults(suiteName: suite)!)
        XCTAssertFalse(bare.hasDraft(on: nil))
        XCTAssertTrue(bare.proposeApp(AppDraft(botLogos: false)))
        XCTAssertEqual(bare.revision, 1)
        XCTAssertTrue(bare.proposeApp(AppDraft(botLogos: false)))
        XCTAssertEqual(bare.revision, 1, "a repeat changes nothing")
        XCTAssertTrue(bare.hasDraft(on: nil))
        XCTAssertNil(bare.draftScope(on: "A"), "no layout waits")
        bare.discardApp(.showMenuBarIcon)
        XCTAssertEqual(bare.revision, 1, "a field it does not set")
        bare.discardApp(.botLogos)
        XCTAssertNil(bare.appDraft)
        XCTAssertEqual(bare.revision, 2)
        XCTAssertFalse(bare.proposeApp(AppDraft()), "an empty change is no draft")
        bare.proposeApp(AppDraft(delayMs: 0))
        bare.savePanel(on: "A")
        XCTAssertNil(bare.appDraft)
        XCTAssertNil(UserDefaults(suiteName: suite)!.data(forKey: SettingsStore.Keys.panelLayout), "a draft is never stored")
    }
}
