import SwiftUI
import AgentHUDCore

struct SubscriptionBadge: View {
    let plan: String
    let theme: Theme

    var body: some View {
        Text(plan)
            .font(.tabular(10, .medium))
            .foregroundStyle(theme.secondary)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(theme.inputBackground, in: Capsule())
            .overlay(Capsule().stroke(theme.cardBorder, lineWidth: 1))
            .fixedSize()
            .accessibilityLabel(L10n.text("订阅", "Subscription") + " " + plan)
    }
}
