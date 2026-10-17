import AppKit
import SwiftUI
import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

/// Settings → Experimental as drawn: its clients menu dimmed while the switch is off, unless a client has the server to
/// take out, and usable once it is on; the switch writes the setting; and a host's rows below Agent HUD's own.
@MainActor
final class ExperimentalPaneTests: XCTestCase {
    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    @MainActor
    private struct Page {
        let hosting: NSHostingView<AnyView>
        let window: NSWindow

        private func views(in view: NSView) -> [NSView] { [view] + view.subviews.flatMap { views(in: $0) } }
        private var visible: [NSView] { views(in: hosting).filter { !$0.isHiddenOrHasHiddenAncestor } }

        var toggle: NSSwitch? { visible.compactMap { $0 as? NSSwitch }.first }
        /// The clients menu, which AppKit draws as a pull-down button.
        var menu: NSPopUpButton? { visible.compactMap { $0 as? NSPopUpButton }.first }
    }

    private func page(_ settings: SettingsStore, _ setup: MCPClientSetup, sections: [DesktopSettingsSection] = []) throws -> Page {
        _ = NSApplication.shared
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        let size = SettingsWindowLayout.size
        let view = SettingsView(settings: settings, store: store, initialTab: .experimental, experimentalSections: sections,
                                clientSetup: setup).frame(width: size.width, height: size.height)
        let hosting = NSHostingView(rootView: AnyView(view))
        hosting.sizingOptions = []
        hosting.frame = NSRect(origin: .zero, size: size)
        let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: -20000, y: -20000), size: size), styleMask: [.borderless],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = hosting
        window.orderFrontRegardless()
        addTeardownBlock { @MainActor in window.orderOut(nil) }
        return Page(hosting: hosting, window: window)
    }

    /// SwiftUI applies a change on its own schedule, so wait for what it makes rather than for a fixed time.
    private func settle(_ page: Page, until reached: () -> Bool) async {
        let deadline = Date().addingTimeInterval(5)
        repeat {
            try? await Task.sleep(for: .milliseconds(50))
            page.hosting.layoutSubtreeIfNeeded()
        } while !reached() && Date() < deadline
    }

    private func settings() throws -> SettingsStore {
        let suite = "app.agenthud.tests.experimental.\(UUID().uuidString)"
        addTeardownBlock { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: try XCTUnwrap(UserDefaults(suiteName: suite)), defaultAgents: DemoData.agents)
        settings.update { $0.language = .en }
        L10n.setLanguage(.en)
        return settings
    }

    func testTheMenuIsDimmedWhileTheSwitchIsOffAndUsableOnceItIsOn() async throws {
        let settings = try settings()
        let mac = FakeClientMac(installed: [.claudeCode: URL(fileURLWithPath: "/usr/local/bin/claude")])
        let setup = MCPClientSetup(environment: mac.environment)
        let page = try page(settings, setup)
        await settle(page) { page.toggle != nil && page.menu != nil }
        let toggle = try XCTUnwrap(page.toggle, "the switch is drawn")
        let menu = try XCTUnwrap(page.menu, "the clients menu is drawn")
        XCTAssertEqual(toggle.state, .off, "off until a person turns it on")
        XCTAssertFalse(menu.isEnabled)
        // SwiftUI fills the menu only as it opens, so the model says what it offers.
        XCTAssertTrue(menu.pullsDown)
        XCTAssertEqual(setup.clients, [.claudeCode])
        XCTAssertEqual(setup.title, "Not added to a client yet")

        toggle.performClick(nil)
        await settle(page) { settings.settings.hudTools && page.menu?.isEnabled == true }
        XCTAssertTrue(settings.settings.hudTools, "the switch writes the setting")
        XCTAssertEqual(page.toggle?.state, .on)
        XCTAssertEqual(page.menu?.isEnabled, true)
        XCTAssertTrue(setup.canPick(.claudeCode, toolsOn: settings.settings.hudTools))

        settings.update { $0.hudTools = false }
        await settle(page) { page.menu?.isEnabled == false }
        XCTAssertEqual(page.menu?.isEnabled, false, "dimmed again with the switch")
        XCTAssertTrue(mac.runs.isEmpty, "nothing ran on its own")
    }

    func testAClientWithTheServerCanStillBeUncheckedWhileTheSwitchIsOff() async throws {
        let settings = try settings()
        let mac = FakeClientMac(installed: [.claudeCode: URL(fileURLWithPath: "/usr/local/bin/claude")], servers: [.claudeCode])
        let setup = MCPClientSetup(environment: mac.environment)
        let page = try page(settings, setup)
        await settle(page) { page.menu != nil }
        XCTAssertFalse(settings.settings.hudTools)
        XCTAssertEqual(page.menu?.isEnabled, true, "the menu its check mark is in")
        XCTAssertEqual(setup.title, "Added to Claude Code")
        XCTAssertEqual(setup.action(for: .claudeCode), .remove(.claudeCode))
        XCTAssertTrue(setup.canPick(.claudeCode, toolsOn: false))
    }

    func testAHostsRowsFollowAgentHUDsOwnAndAHostPageOfTheSameNameJoinsThem() throws {
        let settings = try settings()
        let mac = FakeClientMac(installed: [:])
        let host = DesktopSettingsPage(id: "experimental", title: { "Experimental" }, subtitle: { "" }, symbol: "flask", color: .orange) {
            Text("Remote sessions")
        }
        let view = SettingsView(settings: settings, store: UsageStore(provider: DemoUsageProvider(), settings: settings),
                                initialTab: .experimental, additionalPages: [host],
                                experimentalSections: [DesktopSettingsSection(id: "phone") { Text("Phone approvals") }],
                                clientSetup: MCPClientSetup(environment: mac.environment))
        XCTAssertTrue(view.additionalPages.isEmpty, "no second Experimental page in the sidebar")
        XCTAssertEqual(view.experimentalSections.map(\.id), ["phone", "experimental"], "its rows go on the built-in page instead")
        XCTAssertEqual(SettingsTab.leading, [.general, .sources, .display])
        XCTAssertEqual(SettingsTab.allCases.last, .experimental, "Experimental comes after the host's pages")
    }
}
