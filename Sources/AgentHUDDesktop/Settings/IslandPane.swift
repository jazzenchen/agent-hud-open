import SwiftUI
import AgentHUDCore

struct IslandPane: View {
    let settings: SettingsStore
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            SettingsSection(
                title: L10n.text("展开面板", "Expanded panel"),
                subtitle: L10n.text("每项展示一个简短示例，开关控制对应内容。", "A small sample for each part. Choose what appears in the panel."),
                theme: theme
            ) {
                SettingsToggleRow(label: L10n.text("额度明细", "Quota details"), isOn: settings.binding(\.showIslandQuota))
                IslandPartSample(height: 64, enabled: settings.settings.showIslandQuota) {
                    quotaSample
                }
                SettingsToggleRow(label: L10n.text("重置倒计时", "Reset countdown"), isOn: settings.binding(\.showResetCountdown))
                SettingsDivider(theme: theme)
                SettingsToggleRow(label: L10n.text("Token 消耗", "Token usage"), isOn: settings.binding(\.showIslandTokens))
                IslandPartSample(height: 92, enabled: settings.settings.showIslandTokens) {
                    tokenSample
                }
                SettingsDivider(theme: theme)
                SettingsToggleRow(label: L10n.text("活跃会话", "Active sessions"), isOn: settings.binding(\.showIslandSessions))
                IslandPartSample(height: 56, enabled: settings.settings.showIslandSessions) {
                    sessionSample
                }
            }
            SettingsSection(title: L10n.text("悬停交互", "Hover behavior"), theme: theme) {
                SliderRow(label: L10n.text("展开延迟", "Hover delay"), value: settings.doubleBinding(\.hoverDelayMs), range: 0...1500, step: 50, format: { "\(Int($0)) ms" }, theme: theme)
                SettingsDivider(theme: theme)
                SliderRow(label: L10n.text("收起延迟", "Collapse delay"), value: settings.doubleBinding(\.collapseDelayMs), range: 0...1500, step: 50, format: { "\(Int($0)) ms" }, theme: theme)
            }
        }
    }

    private var quotaSample: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                AgentLogo(vendor: "Claude", size: 12)
                Text("Claude").fontWeight(.semibold)
                Spacer()
                if settings.settings.showResetCountdown {
                    Text(L10n.text("重置", "Resets"))
                }
            }
            .font(.ui(11)).foregroundStyle(Theme.island.secondary)
            HStack(spacing: 10) {
                Circle().fill(Theme.island.status(.ok)).frame(width: 7, height: 7)
                Text(L10n.modelLabel(L10n.windowSession)).font(.ui(12, .semibold)).fixedSize()
                ProgressTrack(fraction: 0.28, fill: Theme.island.status(.ok), track: Theme.island.track)
                    .frame(height: 4)
                Text("28%").font(.tabular(12, .semibold)).foregroundStyle(Theme.island.status(.ok))
                if settings.settings.showResetCountdown {
                    Text(L10n.text("2 小时 14 分", "2h 14m"))
                        .font(.tabular(11)).foregroundStyle(Theme.island.secondary)
                        .frame(width: 78, alignment: .trailing)
                }
            }
        }
    }

    private var tokenSample: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 20) {
                ForEach(Array(IslandSampleData.consumers.enumerated()), id: \.element.id) { index, consumer in
                    HStack(spacing: 5) {
                        Circle().fill(IslandSampleData.colors[index]).frame(width: 6, height: 6)
                        AgentLogo(vendor: consumer.vendor, size: 12)
                        Text(consumer.model)
                    }
                }
            }
            .font(.ui(11)).foregroundStyle(Theme.island.secondary)
            TokenBarsChart(columns: IslandSampleData.columns, interval: IslandSampleData.interval,
                           colors: IslandSampleData.colors, consumers: IslandSampleData.consumers, theme: .island)
                .frame(height: 42)
        }
    }

    private var sessionSample: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Circle().fill(Theme.island.status(.ok)).frame(width: 6, height: 6)
                Text(L10n.text("1 个运行中", "1 running"))
                Spacer()
                Text("60k tok").foregroundStyle(Theme.island.secondary)
            }
            HStack(spacing: 6) {
                AgentLogo(vendor: "Claude", size: 12)
                Text(L10n.text("修复登录问题", "Fix sign-in issue"))
                Spacer()
                Text("Terminal")
            }
            .foregroundStyle(Theme.island.secondary)
        }
        .font(.ui(11))
    }
}

/// A fixed-size sample stays in place when its corresponding panel content is switched off.
private struct IslandPartSample<Content: View>: View {
    let height: CGFloat
    let enabled: Bool
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .foregroundStyle(Theme.island.text)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: height)
            .padding(.horizontal, 14)
            .opacity(enabled ? 1 : 0.25)
            .background(.black, in: RoundedRectangle(cornerRadius: 9))
            .padding(.horizontal, 12)
            .padding(.bottom, 12)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

/// Deliberately small, stable fixtures; settings samples never grow with the user's account history.
private enum IslandSampleData {
    static let consumers = [
        AgentDescriptor(id: "sample-claude", vendor: "Claude", model: "Opus", source: "", enabled: true),
        AgentDescriptor(id: "sample-codex", vendor: "Codex", model: "GPT", source: "", enabled: true),
    ]
    static let colors = [AgentPalette.swiftUIColor(index: 0), AgentPalette.swiftUIColor(index: 1)]
    static let now = Date(timeIntervalSince1970: 1_800_000_000)
    static let interval = StatsRange.hours24.interval(endingAt: now)
    static let columns: [TokenColumn] = {
        let amounts = [4, 8, 5, 11, 16, 9, 6, 13, 20, 14, 10, 7]
        let events = (0..<24).flatMap { hour in
            consumers.enumerated().map { index, consumer in
                let count = amounts[(hour + index * 4) % amounts.count] * 1000
                return TranscriptSession.UsageEvent(
                    timestamp: interval.start.addingTimeInterval(Double(hour) * 3600 + 1800),
                    agentId: consumer.id, tokensIn: count * 4 / 5, tokensOut: count / 5
                )
            }
        }
        return ChartData.tokenBars(usage: events, agentIds: consumers.map(\.id), range: .hours24, now: now)
    }()
}
