import SwiftUI
import AgentHUDCore

struct GeneralPane: View {
    let settings: SettingsStore
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            SettingsSection(title: L10n.text("外观与语言", "Appearance & language"), theme: theme) {
                SettingRow(label: L10n.text("外观", "Appearance")) {
                    Picker(L10n.text("外观", "Appearance"), selection: settings.binding(\.appearance)) {
                        Text(L10n.text("跟随系统", "System")).tag(AppearanceMode.system)
                        Text(L10n.text("浅色", "Light")).tag(AppearanceMode.light)
                        Text(L10n.text("深色", "Dark")).tag(AppearanceMode.dark)
                    }
                    .labelsHidden().pickerStyle(.segmented)
                }
                SettingsDivider(theme: theme)
                SettingRow(label: L10n.text("语言", "Language")) {
                    Picker(L10n.text("语言", "Language"), selection: settings.binding(\.language)) {
                        Text(L10n.text("跟随系统", "System")).tag(AppLanguage.system)
                        Text("中文").tag(AppLanguage.zhHans)
                        Text("English").tag(AppLanguage.en)
                    }
                    .labelsHidden().pickerStyle(.menu)
                }
            }
            SettingsSection(title: L10n.text("启动", "Startup"), theme: theme) {
                SettingsToggleRow(
                    label: L10n.text("登录时启动", "Launch at login"),
                    subtitle: L10n.text("登录 Mac 后，自动开始监测用量。", "Keep an eye on usage whenever you start your Mac."),
                    isOn: settings.binding(\.launchAtLogin)
                )
            }
        }
    }
}
