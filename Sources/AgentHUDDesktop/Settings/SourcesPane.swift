import SwiftUI
import UniformTypeIdentifiers
import AgentHUDCore

struct SourcesPane: View {
    let settings: SettingsStore
    let store: UsageStore
    let theme: Theme
    var sources: [SourceStatus]? = nil
    @State private var expanded: Set<String>
    @State private var dragging: AgentOrderDrag?

    init(settings: SettingsStore, store: UsageStore, theme: Theme, sources: [SourceStatus]? = nil,
         initiallyExpanded: Set<String> = []) {
        self.settings = settings; self.store = store; self.theme = theme; self.sources = sources
        _expanded = State(initialValue: initiallyExpanded)
    }

    var body: some View {
        let detected = sources ?? SourceDetector.resolve(SourceDetector.detect(), report: store.report)
        let groups = AgentSettingsGroup.make(sources: detected, agents: settings.agents, report: store.report)
        VStack(alignment: .leading, spacing: 24) {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(groups) { group in
                    AgentSettingsCard(group: group, settings: settings, theme: theme,
                        isExpanded: Binding(get: { expanded.contains(group.id) }, set: { value in
                            if value { expanded.insert(group.id) } else { expanded.remove(group.id) }
                        }), dragging: $dragging)
                }
                Text(L10n.text("拖动分组或窗口，调整光晕和面板中的顺序。", "Drag groups or windows to reorder the glow and panel."))
                    .font(.ui(11)).foregroundStyle(theme.secondary).padding(.top, 4)
            }
        }
    }
}

struct AgentSettingsCard: View {
    let group: AgentSettingsGroup
    let settings: SettingsStore
    let theme: Theme
    @Binding var isExpanded: Bool
    @Binding var dragging: AgentOrderDrag?
    var body: some View {
        VStack(spacing: 0) {
            header
                .onDrop(of: [UTType.text], delegate: dropDelegate(.group(group.id)))
            if isExpanded && !group.agents.isEmpty {
                SettingsDivider(theme: theme)
                ForEach(group.agents) { agent in
                    AgentOrderRow(agent: agent, theme: theme) { settings.setAgent(id: agent.id, enabled: $0) }
                        .onDrag {
                            dragging = .model(agent.id)
                            return NSItemProvider(object: agent.id as NSString)
                        }
                        .onDrop(of: [UTType.text], delegate: dropDelegate(.model(agent.id)))
                    if agent.id != group.agents.last?.id {
                        SettingsDivider(theme: theme)
                    }
                }
                Color.clear.frame(height: 4)
            }
        }
        .background(theme.card, in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(theme.cardBorder, lineWidth: 1))
    }

    private var header: some View {
        HStack(spacing: 8) {
            if !group.agents.isEmpty {
                OrderDragHandle(theme: theme)
                    .padding(.vertical, 18)
                    .contentShape(Rectangle())
                    .onDrag {
                        dragging = .group(group.id)
                        return NSItemProvider(object: group.id as NSString)
                    }
                    .help(L10n.text("拖动以调整分组顺序", "Drag to reorder this group"))
            } else {
                Color.clear.frame(width: 16)
            }
            Button {
                if !group.agents.isEmpty {
                    withAnimation(.easeInOut(duration: 0.18)) { isExpanded.toggle() }
                }
            } label: {
                HStack(spacing: 12) {
                    AgentLogo(vendor: group.id, size: 26)
                    VStack(alignment: .leading, spacing: 5) {
                        HStack(spacing: 7) {
                            Text(group.id).font(.ui(14, .semibold))
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        ForEach(group.plans, id: \.self) { plan in
                            SubscriptionBadge(plan: plan, theme: theme)
                        }
                        if !group.apiProviders.isEmpty {
                            Text("API · " + group.apiProviders.joined(separator: ", "))
                                .font(.ui(10)).foregroundStyle(theme.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    Spacer(minLength: 4)
                    if !group.agents.isEmpty {
                        Text(L10n.text("显示 \(group.displayedCount)/\(group.agents.count)", "Showing \(group.displayedCount)/\(group.agents.count)"))
                            .font(.tabular(11)).foregroundStyle(theme.secondary)
                            .fixedSize()
                        Image(systemName: "chevron.right")
                            .font(.ui(10, .semibold)).foregroundStyle(theme.tertiary)
                            .rotationEffect(.degrees(isExpanded ? 90 : 0))
                            .frame(width: 12)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
                .padding(.vertical, 12)
                .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .foregroundStyle(theme.text)
            .accessibilityIdentifier("agent-group-\(group.id)")
            .accessibilityValue(isExpanded ? L10n.text("已展开", "Expanded") : L10n.text("已折叠", "Collapsed"))
        }
        .padding(.horizontal, 14)
    }

    private func dropDelegate(_ target: AgentOrderDrag) -> ReorderDropDelegate {
        ReorderDropDelegate(target: target, dragging: $dragging, settings: settings)
    }
}
