import SwiftUI
import AgentHUDCore

struct QuotaForecastDetails: View {
    let agent: AgentDescriptor
    let hint: String
    private let theme = Theme.island

    var body: some View {
        Text(hint)
            .font(.ui(12, .medium))
            .monospacedDigit()
            .foregroundStyle(theme.text)
            .lineLimit(1)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(RoundedRectangle(cornerRadius: 8).fill(theme.windowBackground))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(theme.cardBorder))
            .fixedSize()
            .accessibilityLabel("\(agent.displayName), \(hint)")
    }
}
