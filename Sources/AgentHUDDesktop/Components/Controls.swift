import AppKit
import SwiftUI
import AgentHUDCore

/// Small activity ring with an explicit color for both window and island backgrounds.
struct LoadingSpinner: View {
    let color: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Circle()
            .trim(from: 0, to: 0.75)
            .stroke(color, style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
            .padding(1)
            .keyframeAnimator(initialValue: 0.0, repeating: !reduceMotion) { content, angle in
                content.rotationEffect(.degrees(angle))
            } keyframes: { _ in
                LinearKeyframe(360, duration: 0.9)
            }
            .frame(width: 12, height: 12)
    }
}

struct SegmentOption<Value: Hashable>: Identifiable {
    let value: Value
    let label: String
    var id: Value { value }
}

/// Pill segmented control: 12 px labels, selected segment gets the raised background.
struct SegmentedPills<Value: Hashable>: View {
    let options: [SegmentOption<Value>]
    @Binding var selection: Value
    let theme: Theme

    var body: some View {
        HStack(spacing: 0) {
            ForEach(options) { option in
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { selection = option.value }
                } label: {
                    Text(option.label)
                        .font(.ui(12))
                        .padding(.vertical, 4)
                        .padding(.horizontal, 12)
                        .background(
                            RoundedRectangle(cornerRadius: 5)
                                .fill(option.value == selection ? theme.segmentSelected : Color.clear)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(option.value == selection ? .isSelected : [])
            }
        }
        .padding(2)
        .background(RoundedRectangle(cornerRadius: 7).fill(theme.segmentBackground))
        .foregroundStyle(theme.text)
    }
}

/// A native popup with an explicit size and leading text alignment.
struct SelectionMenu<Value: Hashable>: NSViewRepresentable {
    let title: String
    let options: [SegmentOption<Value>]
    @Binding var selection: Value
    let theme: Theme
    var width: CGFloat = 135

    func makeNSView(context: Context) -> NSPopUpButton {
        let button = NSPopUpButton(frame: .zero, pullsDown: false)
        button.controlSize = .small
        button.font = .systemFont(ofSize: 11)
        button.alignment = .left
        button.target = context.coordinator
        button.action = #selector(Coordinator.select(_:))
        return button
    }

    func updateNSView(_ button: NSPopUpButton, context: Context) {
        context.coordinator.parent = self
        let labels = options.map(\.label)
        if button.itemTitles != labels {
            button.removeAllItems()
            button.addItems(withTitles: labels)
        }
        if let index = options.firstIndex(where: { $0.value == selection }), button.indexOfSelectedItem != index {
            button.selectItem(at: index)
        }
        button.contentTintColor = NSColor(theme.text)
        button.setAccessibilityLabel(title)
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSPopUpButton, context: Context) -> CGSize? {
        CGSize(width: width, height: 22)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject {
        var parent: SelectionMenu
        init(_ parent: SelectionMenu) { self.parent = parent }

        @objc func select(_ sender: NSPopUpButton) {
            let index = sender.indexOfSelectedItem
            guard parent.options.indices.contains(index) else { return }
            parent.selection = parent.options[index].value
        }
    }
}

/// Spacious settings row with trailing native controls.
struct SettingRow<Control: View>: View {
    let label: String
    var subtitle: String? = nil
    @ViewBuilder let control: () -> Control

    var body: some View {
        HStack(spacing: 24) {
            VStack(alignment: .leading, spacing: 4) {
                Text(label)
                if let subtitle {
                    Text(subtitle).font(.ui(11)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 0)
            control().fixedSize()
        }
        .frame(minHeight: 30)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
    }
}

/// Native slider and value, aligned with the other grouped settings rows.
struct SliderRow: View {
    let label: String
    @Binding var value: Double
    let range: ClosedRange<Double>
    let step: Double
    let format: (Double) -> String
    let theme: Theme

    var body: some View {
        HStack(spacing: 12) {
            Text(label).frame(width: 150, alignment: .leading)
            Slider(value: $value, in: range, step: step)
                .tint(Color.accentColor)
                .controlSize(.small)
                .accessibilityLabel(label)
                .accessibilityValue(format(value))
            Text(format(value))
                .font(.tabular(13))
                .foregroundStyle(theme.secondary)
                .frame(width: 60, alignment: .trailing)
        }
        .frame(minHeight: 30)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
    }
}

/// Filled blue action button ("开始使用").
struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.ui(13, .semibold))
            .foregroundStyle(Color.white)
            .padding(.vertical, 7)
            .padding(.horizontal, 18)
            .background(RoundedRectangle(cornerRadius: 7).fill(Color(hex: 0x0a84ff)))
            .opacity(configuration.isPressed ? 0.8 : 1)
    }
}

struct CardModifier: ViewModifier {
    let theme: Theme
    var padding = EdgeInsets(top: 14, leading: 16, bottom: 14, trailing: 16)
    var radius: CGFloat = 10

    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background(RoundedRectangle(cornerRadius: radius).fill(theme.card))
            .overlay(RoundedRectangle(cornerRadius: radius).stroke(theme.cardBorder, lineWidth: 1))
    }
}

extension View {
    func card(_ theme: Theme, padding: EdgeInsets = EdgeInsets(top: 14, leading: 16, bottom: 14, trailing: 16), radius: CGFloat = 10) -> some View {
        modifier(CardModifier(theme: theme, padding: padding, radius: radius))
    }

    /// 1 px line along the top edge (CSS `border-top`).
    func topDivider(_ color: Color) -> some View {
        overlay(alignment: .top) { Rectangle().fill(color).frame(height: 1) }
    }
}

/// Naming rules shared by the stats window and menus.
enum AgentNaming {
    /// "Claude · Opus" when the vendor has several models, otherwise just the vendor.
    static func compact(_ agent: AgentDescriptor, among agents: [AgentDescriptor]) -> String {
        sharesVendor(agent, among: agents) ? "\(agent.displayVendor) · \(L10n.shortModelLabel(agent.model))" : agent.displayVendor
    }

    /// Legend label: "Opus" / "Sonnet" / "ChatGPT".
    static func legend(_ agent: AgentDescriptor, among agents: [AgentDescriptor]) -> String {
        sharesVendor(agent, among: agents) ? L10n.shortModelLabel(agent.model) : agent.displayVendor
    }

    private static func sharesVendor(_ agent: AgentDescriptor, among agents: [AgentDescriptor]) -> Bool {
        agents.filter { $0.displayVendor == agent.displayVendor }.count > 1
    }
}

extension SettingsStore {
    /// Two-way binding into one `Settings` field.
    func binding<T: Equatable>(_ keyPath: WritableKeyPath<AgentHUDCore.Settings, T>) -> Binding<T> {
        Binding(
            get: { self.settings[keyPath: keyPath] },
            set: { value in self.update { $0[keyPath: keyPath] = value } }
        )
    }

    /// Binding for integer settings driven by a `Slider`.
    func doubleBinding(_ keyPath: WritableKeyPath<AgentHUDCore.Settings, Int>) -> Binding<Double> {
        Binding(
            get: { Double(self.settings[keyPath: keyPath]) },
            set: { value in self.update { $0[keyPath: keyPath] = Int(value.rounded()) } }
        )
    }
}
