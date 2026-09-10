import AppKit
import SwiftUI
import XCTest
@testable import AgentHUDDesktop

final class SliderRowTests: XCTestCase {
    @MainActor
    func testSliderDebouncesInputAndSavesPendingValueWhenRemoved() async throws {
        _ = NSApplication.shared
        var value = 20.0
        var commits: [Double] = []
        let hosting = NSHostingView(rootView: AnyView(SliderRow(label: "Brightness",
            value: Binding(get: { value }, set: { value = $0; commits.append($0) }),
            range: 20...100, step: 5, format: { "\(Int($0))%" }, theme: .light)))
        let window = NSWindow(contentRect: CGRect(x: -10000, y: -10000, width: 600, height: 80),
            styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = hosting
        window.orderBack(nil)
        defer { window.orderOut(nil) }
        try await Task.sleep(for: .milliseconds(50))
        hosting.layoutSubtreeIfNeeded()
        let slider = try XCTUnwrap(findSlider(in: hosting))

        func position(for value: Double) -> Double {
            slider.minValue + (value - 20) / 80 * (slider.maxValue - slider.minValue)
        }

        for next in [40.0, 60.0, 80.0] {
            slider.doubleValue = position(for: next)
            slider.sendAction(slider.action, to: slider.target)
            try await Task.sleep(for: .milliseconds(100))
            XCTAssertEqual(slider.doubleValue, position(for: next), accuracy: 0.001, "The slider must follow input immediately")
            XCTAssertEqual(value, 20, "Settings must stay unchanged while input continues")
            XCTAssertTrue(commits.isEmpty)
        }

        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(value, 80)
        XCTAssertEqual(commits, [80], "Only the latest value should reach settings")

        slider.doubleValue = position(for: 90)
        slider.sendAction(slider.action, to: slider.target)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(value, 80)
        hosting.rootView = AnyView(EmptyView())
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(commits, [80, 90], "Leaving the pane must save the pending adjustment")
    }

    @MainActor
    private func findSlider(in view: NSView) -> NSSlider? {
        if let slider = view as? NSSlider { return slider }
        return view.subviews.lazy.compactMap { self.findSlider(in: $0) }.first
    }
}
