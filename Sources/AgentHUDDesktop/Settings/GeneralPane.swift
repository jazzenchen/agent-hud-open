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
            SettingsSection(title: L10n.text("客户端", "Clients"), theme: theme) {
                ClientHooksSetting(settings: settings)
            }
        }
    }
}

/// Removing the hooks costs features the user may not connect with them, so switching off names each one first.
private struct ClientHooksSetting: View {
    let settings: SettingsStore
    @State private var confirming = false

    var body: some View {
        SettingRow(label: L10n.text("客户端回调", "Client hooks"),
                   subtitle: L10n.text("写入各客户端的设置，用来在 HUD 上回答权限请求、显示完成与等待批准。",
                                       "Added to each client's settings so the HUD can answer permission requests and show completions and waiting.")) {
            Toggle(L10n.text("客户端回调", "Client hooks"), isOn: Binding(get: {
                settings.settings.clientHooks && !confirming
            }, set: { value in
                if value { settings.update { $0.clientHooks = true } } else { confirming = true }
            }))
            .labelsHidden().toggleStyle(.switch).controlSize(.small)
            .accessibilityIdentifier("general-client-hooks")
        }
        .alert(L10n.text("移除客户端回调？", "Remove client hooks?"), isPresented: $confirming) {
            Button(L10n.text("取消", "Cancel"), role: .cancel) {}
            Button(L10n.text("移除", "Remove"), role: .destructive) { settings.update { $0.clientHooks = false } }
        } message: {
            Text(L10n.text(
                """
                Agent HUD 会从各客户端的设置中移除自己添加的回调，之后也不再写回。关闭后将失去：
                · 在 HUD 上回答 Claude Code、Codex、CodeBuddy、WorkBuddy、ZCode、Qwen Code 与 Qoder 的权限请求
                · Antigravity、Cursor、GitHub Copilot CLI、CodeBuddy 与 Qwen Code 的完成提醒
                · Claude Code 会话的等待批准状态
                · Pi 的运行状态与完成提醒
                用量、额度与会话统计不受影响。重新开启后，Codex 需要在 /hooks 中再次信任。
                """,
                """
                Agent HUD removes the hooks it added to each client's settings and stops adding them back. You lose:
                • Answering permission requests from Claude Code, Codex, CodeBuddy, WorkBuddy, ZCode, Qwen Code and Qoder on the HUD
                • Completion reminders for Antigravity, Cursor, GitHub Copilot CLI, CodeBuddy and Qwen Code
                • The waiting-for-approval state of Claude Code sessions
                • Running status and completion reminders for Pi
                Usage, quota and session statistics are unaffected. Turned back on, Codex asks you to trust its hook again in /hooks.
                """))
        }
    }
}
