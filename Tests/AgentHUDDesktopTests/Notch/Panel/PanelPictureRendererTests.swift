import AgentHUDSupport
import AppKit
import SwiftUI
import Vision
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// The hover panel drawn off screen for an agent: its size against the panel's own measure, the card it is drawn on,
/// what it hides, and the scale a tall panel is drawn at. What the picture shows is read back from its pixels with
/// on-device text recognition, which reads the same text a client's model would.
@MainActor
final class PanelPictureRendererTests: XCTestCase {
    /// The sample's store over settings of its own, made when a test first reaches for either.
    private lazy var store: UsageStore = {
        _ = NSApplication.shared
        let suite = "app.agenthud.tests.panel-picture.\(UUID().uuidString)"
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: UserDefaults(suiteName: suite)!, defaultAgents: DemoData.agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings, clock: { DemoClock.now })
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: DemoClock.now))
        return store
    }()

    private let display = PanelPictureRenderer.Display(insets: HoverPanelView.notchInsets, maximumHeight: 2_000,
                                                       look: .standard)
    private var renderer: PanelPictureRenderer { PanelPictureRenderer(store: store, display: display) }

    override func setUp() {
        super.setUp()
        L10n.setLanguage(.en)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    private func bitmap(_ picture: PanelPicture, file: StaticString = #filePath, line: UInt = #line) throws -> NSBitmapImageRep {
        try XCTUnwrap(NSBitmapImageRep(data: picture.png), "the picture is a PNG", file: file, line: line)
    }

    /// The lines of text recognized in `image`.
    private func text(in image: CGImage) throws -> [String] {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["en-US"]
        request.usesLanguageCorrection = false
        try VNImageRequestHandler(cgImage: image).perform([request])
        return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
    }

    private func text(of picture: PanelPicture) throws -> String {
        try text(in: try XCTUnwrap(try bitmap(picture).cgImage)).joined(separator: "\n")
    }

    /// The panel as the island hosts it, with nothing set in its environment, drawn at twice its size.
    private func islandPanelText() throws -> String {
        try text(drawing: HoverPanelView(store: store, onOpenStats: {}).frame(width: IslandController.expandedWidth)
            .background(Color.black))
    }

    /// The text read in `view`, hosted alone at the size it takes and drawn at twice it.
    private func text(drawing view: some View) throws -> String {
        let hosting = NSHostingView(rootView: view)
        hosting.frame = CGRect(origin: .zero, size: hosting.fittingSize)
        hosting.layoutSubtreeIfNeeded()
        let rep = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(hosting.frame.width * 2),
                                                 pixelsHigh: Int(hosting.frame.height * 2), bitsPerSample: 8, samplesPerPixel: 4,
                                                 hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
        rep.size = hosting.frame.size
        hosting.cacheDisplay(in: hosting.bounds, to: rep)
        return try text(in: try XCTUnwrap(rep.cgImage)).joined(separator: "\n")
    }

    /// What the sample names that a picture must not: its accounts' labels, and its listed sessions' titles and
    /// projects as their rows write them.
    private var names: [String] {
        let labels = (store.report?.accounts ?? [:]).values.flatMap { $0 }.map(\.displayName)
        let sessions = HoverPanelView.sessionRows(store).shown
        return Array(Set(labels + sessions.map { HoverPanelView.shortTask($0.task) } + sessions.compactMap(\.terminal)))
    }

    func testThePictureIsThePanelAtTwiceItsMeasuredHeightOnTheIslandsCard() throws {
        let fit = renderer.fit()
        // The island's own measure of its open panel at the notch, which it takes before opening.
        let root = IslandRootView(store: store, isOpen: true, collapsedSize: CGSize(width: 216, height: 32), collapsedTopRadius: 16,
                                  collapsedBottomRadius: 12, look: .standard, lightBorder: false, onOpenStats: {})
        let island = IslandWindowController(frame: CGRect(x: -20_000, y: -20_000, width: 600, height: 400), rootView: root)
        XCTAssertEqual(fit.height, Double(max(80, island.contentHeight(for: root).rounded())),
                       "measured as the island measures the panel before it opens")
        XCTAssertGreaterThan(fit.height, 300)
        XCTAssertEqual(fit.maximumHeight, 2_000)
        XCTAssertFalse(fit.scrolls)

        let picture = try XCTUnwrap(renderer.picture())
        XCTAssertEqual(picture.width, 540)
        XCTAssertEqual(picture.fit, fit)
        XCTAssertEqual(picture.scale, 2)
        XCTAssertFalse(picture.isScaledDown)
        let rep = try bitmap(picture)
        XCTAssertEqual(rep.pixelsWide, 1_080)
        XCTAssertEqual(rep.pixelsHigh, Int(fit.height * 2))
        XCTAssertEqual(rep.colorAt(x: 0, y: 0)?.alphaComponent ?? 1, 0, accuracy: 0.01, "the card's corner is round")
        XCTAssertEqual(rep.colorAt(x: rep.pixelsWide - 1, y: rep.pixelsHigh - 1)?.alphaComponent ?? 1, 0, accuracy: 0.01)
        let inside = try XCTUnwrap(rep.colorAt(x: rep.pixelsWide / 2, y: 8)?.usingColorSpace(.deviceRGB))
        XCTAssertEqual(inside.alphaComponent, 1, accuracy: 0.01, "the card is black and opaque")
        XCTAssertLessThan(inside.redComponent + inside.greenComponent + inside.blueComponent, 0.05)
    }

    func testThePictureMasksAccountsAndLeavesOutSessionTitles() throws {
        let names = names
        XCTAssertTrue(names.contains("Personal"), "the sample names its accounts: \(names)")
        // The panel a person sees is unchanged: with nothing set it names every account and session, and so does the
        // renderer's own drawing with masking off.
        let island = try islandPanelText()
        let unmasked = try text(of: try XCTUnwrap(renderer.draw(masked: false)))
        // Each name is read as it is written, its capitals too; a masked name must not be read in any case.
        for name in names {
            XCTAssertTrue(island.contains(name), "the island's panel shows \(name):\n\(island)")
            XCTAssertTrue(unmasked.contains(name), "unmasked, the picture shows \(name):\n\(unmasked)")
        }

        let masked = try XCTUnwrap(renderer.picture())
        let read = try text(of: masked)
        for name in names {
            XCTAssertFalse(read.localizedCaseInsensitiveContains(name), "the picture shows \(name):\n\(read)")
        }
        XCTAssertFalse(read.contains("@"))
        for position in 1...HoverPanelView.sessionRows(store).shown.count {
            XCTAssertTrue(read.contains("Session \(position)"), "row \(position) is named by its place:\n\(read)")
        }
        for kept in ["Claude", "Codex", "Sessions", "Current session", "tok"] {
            XCTAssertTrue(read.contains(kept), "the rest of the panel is drawn: \(kept)")
        }
        XCTAssertEqual(try bitmap(masked).pixelsHigh, Int(renderer.fit().height * 2), "masking moves nothing")
    }

    /// A failed refresh marks every reading as failed, which the picture shows where the panel does, with the reason
    /// as placeholder bars; the line above the content that says the refresh failed is the island's, and the picture
    /// leaves it out.
    func testThePictureLeavesOutTheLineThatSaysTheRefreshFailed() throws {
        store.lastError = "The network connection was lost."
        func panel(showingError: Bool) -> CGFloat {
            IslandWindowController.naturalHeight(of: HoverPanelView(store: store, onOpenStats: {}, showsRefreshError: showingError)
                .frame(width: IslandController.expandedWidth, alignment: .top))
        }
        XCTAssertGreaterThan(panel(showingError: true), panel(showingError: false) + 5, "the island's panel says it above its content")
        XCTAssertEqual(renderer.fit().height, Double(max(80, panel(showingError: false).rounded())))
        let unmasked = try text(of: try XCTUnwrap(renderer.draw(masked: false)))
        XCTAssertFalse(unmasked.contains("Refresh failed"), unmasked)
        XCTAssertTrue(unmasked.contains("The network connection was lost"), "the readings it failed say why:\n\(unmasked)")
        let read = try text(of: try XCTUnwrap(renderer.picture()))
        XCTAssertFalse(read.contains("Refresh failed"), read)
        XCTAssertFalse(read.contains("network connection"), "the reason is a placeholder in the picture:\n\(read)")
    }

    /// Accounts named by emails and workspaces, a client's notice and a reading's reason that quote a path and an
    /// email, and sessions with titles and projects: the picture shows none of them, where the panel a person sees
    /// shows every one, and masking moves nothing.
    func testThePictureNamesNoAccountSessionProjectOrPath() throws {
        let now = DemoClock.now, demo = try XCTUnwrap(store.report)
        let failure = "Codex: could not read /Users/jazzen/.codex/auth.json for dev@acme.io, so the plan's windows are the last read"
        let accounts: [String: [AccountObservation]] = [
            "Claude": [AccountObservation(account: DemoData.claudeAccount, label: "jazzenus@gmail.com", plan: "max_20x", observedAt: now,
                                          sourceInfo: "Read from /Users/jazzen/.claude")],
            "Codex": [AccountObservation(account: DemoData.codexAccount, label: "Acme Platform Workspace", plan: "pro", observedAt: now,
                                         quotaNotice: failure, readingIssue: .readFailed(failure),
                                         resetCredits: DemoData.codexResetCredits(now: now))],
        ]
        let billing = demo.billing.map {
            APIBilling(vendor: $0.vendor, balances: $0.balances, isAvailable: $0.isAvailable, updatedAt: $0.updatedAt, costs: $0.costs,
                       sessionCosts: $0.sessionCosts, notice: "Balance read failed at /Users/jazzen/.deepseek/key")
        }
        let sessions = demo.sessions.enumerated().map { index, session in
            LiveSession(id: session.id, agentId: session.agentId, task: ["Rotate keys for ops@acme.io", "Fix billing for Globex"][index % 2],
                        terminal: ["globex-payments", "acme-secret-ios"][index % 2], startedAt: session.startedAt,
                        endedAt: session.endedAt, pctOfWindow: session.pctOfWindow, tokensIn: session.tokensIn,
                        tokensOut: session.tokensOut, client: session.client, observedAt: session.observedAt,
                        workingDirectory: "/Users/jazzen/work/acme-secret-ios", lastActivityAt: session.lastActivityAt)
        }
        store.replace(report: UsageReport(generatedAt: demo.generatedAt, snapshots: demo.snapshots, sessions: sessions,
                                          discoveredAgents: demo.discoveredAgents, consumers: demo.consumers, usage: demo.usage,
                                          insightsByAgent: demo.insightsByAgent, subscriptions: demo.subscriptions,
                                          sourceNotices: ["Claude": "Engine said: EACCES /Users/jazzen/.claude.json"],
                                          quotaNotices: [:], readingIssues: [:], consumerIdsByQuota: demo.consumerIdsByQuota,
                                          billing: billing, accounts: accounts, sessionUsage: demo.sessionUsage, periods: demo.periods))
        let named = ["jazzenus", "Acme Platform", "Rotate keys", "Fix billing", "globex-payments", "acme-secret",
                     "/Users/", "jazzen/", "acme.io", "EACCES", "Balance read failed"]

        let island = try islandPanelText(), unmasked = try text(of: try XCTUnwrap(renderer.draw(masked: false)))
        for name in named {
            XCTAssertTrue(island.contains(name), "the island's panel shows \(name):\n\(island)")
            XCTAssertTrue(unmasked.contains(name), "unmasked, the picture shows \(name):\n\(unmasked)")
        }
        let masked = try XCTUnwrap(renderer.picture())
        let read = try text(of: masked)
        for name in named { XCTAssertFalse(read.contains(name), "the picture shows \(name):\n\(read)") }
        XCTAssertTrue(read.contains("gmail.com"), "an email keeps its domain:\n\(read)")
        for kept in ["Session 1", "Session 5", "Current session", "tok", "Account balance"] {
            XCTAssertTrue(read.contains(kept), "the rest of the panel is drawn: \(kept)\n\(read)")
        }
        XCTAssertEqual(IslandWindowController.naturalHeight(of: renderer.panel(masked: true)),
                       IslandWindowController.naturalHeight(of: renderer.panel(masked: false)), "masking moves nothing")
        XCTAssertEqual(try bitmap(masked).pixelsHigh, Int(renderer.fit().height * 2))
    }

    /// A draft is drawn as the panel draws it while it waits: in its layout, with the bar that says it is unsaved.
    func testThePictureDrawsAWaitingDraftWithItsBar() throws {
        let saved = try XCTUnwrap(renderer.picture())
        XCTAssertFalse(try text(of: saved).contains("Unsaved layout"))
        store.settings.panelLayout.propose(PanelLayout(blocks: ["Codex": PanelLayout.Block(hidden: true)]))
        XCTAssertTrue(store.settings.panelLayout.hasDraft)
        let draft = try XCTUnwrap(renderer.picture())
        let read = try text(of: draft)
        XCTAssertTrue(read.contains("Unsaved layout"), read)
        XCTAssertFalse(read.contains("Spark"), "the draft hides Codex's block:\n\(read)")
        XCTAssertEqual(draft.fit, renderer.fit())
        XCTAssertNotEqual(draft.fit.height, saved.fit.height)
        XCTAssertEqual(try bitmap(draft).pixelsHigh, Int(draft.fit.height * 2))
    }

    /// A screen with a layout of its own is drawn in it, and one that uses the shared layout in the shared one: the
    /// renderer draws the screen it is given.
    func testThePictureOfAScreenIsDrawnInThatScreensLayout() throws {
        let layout = store.settings.panelLayout
        layout.propose(.own(PanelLayout(blocks: ["Codex": PanelLayout.Block(hidden: true)])), for: "screen-b")
        layout.save(.screen("screen-b"))
        var own = renderer
        own.screen = "screen-b"
        var shared = renderer
        shared.screen = "screen-a"
        XCTAssertEqual(shared.fit(), renderer.fit(), "a screen without a layout of its own draws the shared one")
        XCTAssertLessThan(own.fit().height, renderer.fit().height - 20, "the screen's own layout hides Codex's block")
        let read = try text(of: try XCTUnwrap(own.picture()))
        XCTAssertFalse(read.contains("Spark"), read)
        XCTAssertFalse(read.contains("Unsaved layout"), "its saved layout has no bar:\n\(read)")

        layout.propose(.shared, for: "screen-b")
        let back = try text(of: try XCTUnwrap(own.picture()))
        XCTAssertTrue(back.contains("Spark"), "a draft to use the shared layout draws the shared layout:\n\(back)")
        XCTAssertTrue(back.contains("This display only"), "with the bar of the screen's own draft:\n\(back)")
    }

    /// The picture and the measure take the width of the layout drawn, the screen's own or the shared one, a draft
    /// included: at 480 and at 720 the panel is that wide, and a narrower panel is never shorter.
    func testThePictureIsDrawnAtTheLayoutsWidth() throws {
        let layout = store.settings.panelLayout
        XCTAssertEqual(renderer.width, 540)
        layout.propose(PanelLayout(style: PanelStyle(width: 720)))
        let wide = try XCTUnwrap(renderer.picture())
        XCTAssertEqual(wide.width, 720)
        XCTAssertEqual(try bitmap(wide).pixelsWide, 1_440)
        layout.propose(.own(PanelLayout(style: PanelStyle(width: 480))), for: "screen-narrow")
        var narrow = renderer
        narrow.screen = "screen-narrow"
        let picture = try XCTUnwrap(narrow.picture())
        XCTAssertEqual(picture.width, 480)
        XCTAssertEqual(try bitmap(picture).pixelsWide, 960)
        XCTAssertGreaterThanOrEqual(narrow.fit().height, renderer.fit().height - 0.5)
        XCTAssertTrue(try text(of: picture).contains("This display only"), "its own draft's bar fits the narrow panel")
    }

    /// A dock on a side edge keeps its strip clear inside the panel; at the narrowest width, beside the widest strip a
    /// Simple panel has, the panel is widened to keep its content at the narrowest any panel has, and nothing in it is
    /// cut: Codex's earned resets name their first expiry, whole or as the date alone.
    func testASideDockAtTheNarrowestWidthCutsNothing() throws {
        store.settings.panelLayout.propose(PanelLayout(style: PanelStyle(width: PanelStyle.widthRange.lowerBound)))
        let insets = EdgeInsets(top: 14, leading: 155, bottom: 14, trailing: 18)
        let side = PanelPictureRenderer(store: store, display: .init(insets: insets, maximumHeight: 2_000, look: .standard))
        XCTAssertEqual(side.width, HoverPanelView.narrowestContentWidth + 155 + 18)
        let read = try text(of: try XCTUnwrap(side.picture()))
        let expiry = CodexResetCreditsView(resets: DemoData.codexResetCredits(now: DemoClock.now), showExpiry: true).nextExpiryLabel
        XCTAssertTrue(read.contains(expiry.short), read)
        // A word cut short ends in an ellipsis, as "First ex…" did; the chart's bars read as dots between letters.
        let cut = read.split(separator: "\n").filter { $0.range(of: #"\p{L}{2,}(…|\.\.\.)$"#, options: .regularExpression) != nil }
        XCTAssertEqual(cut, [], "nothing is cut: \(read)")
    }

    func testAPanelTallerThanTheDisplayIsDrawnWholeAndSaidToScroll() throws {
        let short = PanelPictureRenderer(store: store, display: .init(insets: HoverPanelView.notchInsets, maximumHeight: 400,
                                                                      look: .standard))
        let picture = try XCTUnwrap(short.picture())
        XCTAssertTrue(picture.fit.scrolls)
        XCTAssertEqual(picture.fit.maximumHeight, 400)
        XCTAssertEqual(try bitmap(picture).pixelsHigh, Int(picture.fit.height * 2), "the picture shows all of it")
    }

    func testATallPictureIsDrawnAtTheScaleThatKeepsItWithinTheLimit() throws {
        XCTAssertEqual(renderer.pixelLimit, 2_400)
        var limited = renderer
        limited.pixelLimit = 1_000
        let picture = try XCTUnwrap(limited.picture())
        let height = picture.fit.height
        XCTAssertGreaterThan(height * 2, 1_000, "taller than the limit at twice its size")
        XCTAssertEqual(picture.scale, 1_000 / height, accuracy: 0.000_001)
        XCTAssertTrue(picture.isScaledDown)
        let rep = try bitmap(picture)
        XCTAssertEqual(rep.pixelsHigh, 1_000)
        XCTAssertEqual(rep.pixelsWide, Int((540 * 1_000 / height).rounded()))
        XCTAssertEqual(rep.size.height, CGFloat(height), accuracy: 0.5, "still the panel's size in points")
    }

    // MARK: One agent's own panel

    /// One agent's own panel is drawn as the card a click on its mark opens beside it on a dock that separates its agents:
    /// measured as the island measures that card before it opens, at the layout's `agentPanelWidth` inside a detached
    /// card's insets, whatever the display's dock keeps clear, on a card with the rounded corners of an agent's card,
    /// and showing that agent alone, its account's name masked.
    func testAnAgentsOwnPanelIsDrawnAsTheCardItsMarkOpens() throws {
        var claude = renderer
        claude.agent = "Claude"
        XCTAssertEqual(claude.width, 440)
        XCTAssertEqual(claude.insets, IslandRootView.detachedInsets)
        let fit = claude.fit()
        // The island's own measure of Claude's whole card on a dock that separates its agents, after a click on its mark.
        var root = IslandRootView(store: store, isOpen: true, collapsedSize: CGSize(width: 120, height: 36), collapsedTopRadius: 0,
                                  collapsedBottomRadius: 0, look: .standard, lightBorder: false, onOpenStats: {})
        root.hidesSilhouette = true
        root.separatesAgents = true
        root.focusVendor = "Claude"
        root.pinned = true
        let island = IslandWindowController(frame: CGRect(x: -20_000, y: -20_000, width: 600, height: 400), rootView: root)
        XCTAssertEqual(fit.height, Double(max(80, island.contentHeight(for: root).rounded())),
                       "measured as the island measures an agent's card before it opens")
        XCTAssertEqual(fit.contentWidth, Double(HoverPanelView.narrowestContentWidth))
        XCTAssertEqual(fit.maximumHeight, 2_000)
        XCTAssertLessThan(fit.height, renderer.fit().height - 20, "Claude's own panel is shorter than every agent's")
        let side = PanelPictureRenderer(store: store, display: .init(insets: EdgeInsets(top: 14, leading: 155, bottom: 14, trailing: 18),
                                                                     maximumHeight: 2_000, look: .standard), agent: "Claude")
        XCTAssertEqual(side.width, 440, "the card floats beside the mark and keeps no strip clear")
        XCTAssertEqual(side.fit(), fit)

        let picture = try XCTUnwrap(claude.picture())
        XCTAssertEqual(picture.width, 440)
        XCTAssertEqual(picture.fit, fit)
        let rep = try bitmap(picture)
        XCTAssertEqual(rep.pixelsWide, 880)
        XCTAssertEqual(rep.pixelsHigh, Int(fit.height * 2))
        XCTAssertEqual(rep.colorAt(x: 0, y: 0)?.alphaComponent ?? 1, 0, accuracy: 0.01, "the card's corner is round")
        XCTAssertEqual(rep.colorAt(x: rep.pixelsWide - 1, y: rep.pixelsHigh - 1)?.alphaComponent ?? 1, 0, accuracy: 0.01)
        let read = try text(of: picture)
        XCTAssertTrue(read.contains("Claude"), read)
        XCTAssertFalse(read.contains("Spark"), "nothing of Codex:\n\(read)")
        XCTAssertFalse(read.contains("Personal"), "Claude's account is masked:\n\(read)")
        XCTAssertTrue(try text(of: try XCTUnwrap(claude.draw(masked: false))).contains("Personal"), "and named unmasked")
    }

    /// An agent's own panel takes the width the layout drawn gives it, the screen's own or the shared one, a draft
    /// included, and shows that layout's draft with its bar, which says This display only for a screen's own.
    func testAnAgentsOwnPanelIsDrawnInTheLayoutAndWidthOfItsScreen() throws {
        let layout = store.settings.panelLayout
        layout.propose(PanelLayout(style: PanelStyle(width: 700)))
        var claude = renderer
        claude.agent = "Claude"
        XCTAssertEqual(claude.width, 600, "the draft's width less 100")
        XCTAssertEqual(try bitmap(try XCTUnwrap(claude.picture())).pixelsWide, 1_200)

        layout.propose(.own(PanelLayout(blocks: ["Claude": PanelLayout.Block(widget: .ring)], style: PanelStyle(width: 480))),
                       for: "screen-b")
        claude.screen = "screen-b"
        XCTAssertEqual(claude.width, 440, "never below 440")
        let read = try text(of: try XCTUnwrap(claude.picture()))
        XCTAssertTrue(read.contains("This display only"), "with the bar of the screen's own draft:\n\(read)")
        XCTAssertFalse(read.contains("Spark"), read)
    }

    // MARK: In a look

    /// The picture is drawn in the display's look. In the light one the card is the light surface where the standard
    /// one's is black, and the panel on it takes the light tokens from its environment: its text is dark ink that reads
    /// on that card. A look changes colours only, so the panel is measured as in the standard one.
    func testThePictureIsDrawnInTheDisplaysLook() throws {
        let light = PanelPictureRenderer(store: store, display: .init(insets: HoverPanelView.notchInsets, maximumHeight: 2_000,
                                                                      look: HUDLook(appearance: .light)))
        XCTAssertEqual(light.fit(), renderer.fit(), "the look moves nothing")
        let picture = try XCTUnwrap(light.draw(masked: false))
        let rep = try bitmap(picture)
        XCTAssertEqual(rep.pixelsHigh, Int(picture.fit.height * 2))
        let inside = try XCTUnwrap(rep.colorAt(x: rep.pixelsWide / 2, y: 8)?.usingColorSpace(.deviceRGB))
        XCTAssertEqual(inside.alphaComponent, 1, accuracy: 0.01, "the card is opaque")
        XCTAssertGreaterThan(inside.redComponent + inside.greenComponent + inside.blueComponent, 2.7, "and light, not black")
        let read = try text(of: picture)
        for kept in ["Claude", "Codex", "Sessions", "Current session", "tok"] {
            XCTAssertTrue(read.contains(kept), "the panel's text reads on the light card: \(kept)\n\(read)")
        }
    }

    /// The island measures what it holds before it opens, in a hosting view of its own, and that content carries the
    /// screen's look wherever it is hosted. In the light look it is measured as tall as in the standard one, and drawn
    /// on the light surface its text is the light look's dark ink, where the standard look's light ink does not read.
    func testTheIslandMeasuresWhatItHoldsInItsLook() throws {
        func root(_ look: HUDLook) -> IslandRootView {
            IslandRootView(store: store, isOpen: true, collapsedSize: CGSize(width: 216, height: 32), collapsedTopRadius: 16,
                           collapsedBottomRadius: 12, look: look, lightBorder: false, onOpenStats: {})
        }
        let light = HUDLook(appearance: .light)
        let island = IslandWindowController(frame: CGRect(x: -20_000, y: -20_000, width: 600, height: 400), rootView: root(light))
        let measured = island.contentHeight(for: root(light))
        XCTAssertEqual(measured, island.contentHeight(for: root(.standard)))
        XCTAssertGreaterThan(measured, 300)

        let read = try text(drawing: root(light).content.background(light.theme.islandFill))
        for kept in ["Claude", "Codex", "Sessions"] {
            XCTAssertTrue(read.contains(kept), "the content is drawn in the light look: \(kept)\n\(read)")
        }
        let standard = try text(drawing: root(.standard).content.background(light.theme.islandFill))
        for name in ["Claude", "Codex"] {
            XCTAssertFalse(standard.contains(name), "the standard look's ink is light: \(name)\n\(standard)")
        }
    }

    // MARK: To a client

    /// The picture travels as a client sees it: a tool call from the `--mcp` process reaches the application's tools
    /// over the socket, which draw it, and the answer is the text that says what it shows and the PNG as an image block.
    func testAPictureOfThePanelReachesAClientAsAnImageBlock() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("pp-" + UUID().uuidString.prefix(8))
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        let renderer = renderer
        let tools = HUDTools(store: store, picture: { _ in renderer.picture() }, fit: { _ in renderer.fit() })
        let channel = MCPAppChannel(directory: directory) { tools.answer($0) }
        XCTAssertTrue(channel.start())
        addTeardownBlock { await channel.stop() }
        let path = channel.path
        let line = #"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_hud_screenshot","arguments":{}}}"#

        var content: [JSONValue] = []
        for _ in 0..<300 {
            let lines = await Task.detached {
                MCPServer(version: "0", call: { MCPAppClient.call($0, socket: path, timeout: 10) }).lines(for: line)
            }.value
            let answer = try JSONValue.parse(Data(try XCTUnwrap(lines.first).utf8))
            content = answer["result"]["content"].arrayValue ?? []
            XCTAssertEqual(answer["id"], .integer(3))
            // The listener comes up on the main queue, so the first calls may find nobody serving yet.
            guard content.first?["text"].stringValue?.contains("not running") == true else { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(content.count, 2)
        let said = try XCTUnwrap(JSONValue.parse(Data(try XCTUnwrap(content.first?["text"].stringValue).utf8)).objectValue)
        let fit = renderer.fit()
        XCTAssertEqual(said["height"], .integer(Int64(fit.height)))
        XCTAssertEqual(said["width"], .integer(540))
        XCTAssertEqual(said["unsaved"], .bool(false))
        let image = try XCTUnwrap(content.last)
        XCTAssertEqual(image["type"], .string("image"))
        XCTAssertEqual(image["mimeType"], .string("image/png"))
        let png = try XCTUnwrap(Data(base64Encoded: try XCTUnwrap(image["data"].stringValue)))
        let rep = try XCTUnwrap(NSBitmapImageRep(data: png))
        XCTAssertEqual(rep.pixelsWide, 1_080)
        XCTAssertEqual(rep.pixelsHigh, Int(fit.height * 2))
    }
}
