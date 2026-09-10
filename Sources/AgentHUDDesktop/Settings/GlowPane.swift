import SwiftUI
import AgentHUDCore

struct GlowPane: View {
    let settings: SettingsStore
    let store: UsageStore
    let theme: Theme

    var body: some View {
        SettingsSection(title: L10n.text("刘海光晕", "Notch glow"), theme: theme) {
            SettingsPreview {
                Color.clear.frame(height: 112)
                    .overlay(alignment: .top) {
                        GlowPreview(
                            appearance: store.glowAppearance(light: false), settings: settings.settings,
                            islandSize: CGSize(width: 240, height: 30), islandRadius: 13
                        )
                    }
            }
            SliderRow(label: L10n.text("光晕亮度", "Brightness"), value: percentBinding(\.glowBrightness), range: 20...100, step: 5, format: { "\(Int($0))%" }, theme: theme)
            SettingsDivider(theme: theme)
            SliderRow(label: L10n.text("光晕范围", "Glow range"), value: settings.binding(\.glowRange), range: AgentHUDCore.Settings.glowSizeRange, step: 1, format: { "\(Int($0)) px" }, theme: theme)
            SettingsDivider(theme: theme)
            SliderRow(label: L10n.text("羽化", "Feather"), value: settings.binding(\.glowBlur), range: AgentHUDCore.Settings.glowSizeRange, step: 1, format: { "\(Int($0)) px" }, theme: theme)
            SettingsDivider(theme: theme)
            SettingsToggleRow(
                label: L10n.text("仅向外扩散", "Outward only"),
                subtitle: L10n.text("贴近刘海的边缘更浓，向外逐渐变淡。", "Keep the rim defined and fade gently outward."),
                isOn: settings.binding(\.glowOutwardOnly)
            )
            SettingsDivider(theme: theme)
            SliderRow(label: L10n.text("呼吸周期", "Breath period"), value: settings.binding(\.breathSeconds), range: 1...8, step: 0.5, format: { String(format: L10n.text("%.1f 秒", "%.1f s"), $0) }, theme: theme)
            SettingsDivider(theme: theme)
            SliderRow(label: L10n.text("呼吸幅度", "Breath depth"), value: percentBinding(\.breathAmplitude), range: 0...100, step: 5, format: { "\(Int($0))%" }, theme: theme)
            Text(L10n.text("Agent 运行时自动呼吸，空闲时保持静态光晕。", "The glow breathes while an agent is running, and rests when it is idle."))
                .font(.ui(11)).foregroundStyle(theme.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16).padding(.bottom, 12)
        }
    }

    private func percentBinding(_ keyPath: WritableKeyPath<AgentHUDCore.Settings, Double>) -> Binding<Double> {
        Binding(
            get: { settings.settings[keyPath: keyPath] * 100 },
            set: { value in settings.update { $0[keyPath: keyPath] = value / 100 } }
        )
    }
}
