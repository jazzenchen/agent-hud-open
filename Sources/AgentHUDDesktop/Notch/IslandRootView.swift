import SwiftUI
import AgentHUDCore

/// A single silhouette: closed notch, lateral event wings, or a hovered detail surface.
struct IslandRootView: View {
    let store: UsageStore?
    let isOpen: Bool
    let collapsedSize: CGSize
    let collapsedTopRadius: CGFloat
    let collapsedBottomRadius: CGFloat
    let lightBorder: Bool
    let onOpenStats: () -> Void
    var onOpenSettings: () -> Void = {}
    var alert: IslandAlert? = nil
    var onOpenAlert: () -> Void = {}
    /// The user's answer to a request waiting on the island; the alert's own id says which request it answers.
    var onDecideAlert: (PermissionDecision) -> Void = { _ in }
    /// Every request waiting for this user, oldest first and across screens: the expanded card stacks the rest under
    /// the one being decided, and the collapsed island only counts them.
    var waitingRequests: [PermissionRequest] = []
    /// Brings one of the stacked requests to the front.
    var onSelectRequest: (String) -> Void = { _ in }
    /// The user started or stopped typing an answer on the island.
    var onTyping: @MainActor (Bool) -> Void = { _ in }
    var showsAlertDetails = false
    var presentationSize: CGSize? = nil
    var animatesGeometry = true
    var onContentHeight: (CGFloat) -> Void = { _ in }
    /// Set on a screen in logo mode: the marks ride on top of the silhouette, collapsed or open, so hovering
    /// never makes the agents disappear.
    var logoQueue: LogoQueueConfig? = nil
    /// Set on a screen whose HUD is a logo queue. Collapsed, such a HUD has no silhouette: it is the marks
    /// over whatever is behind them. Kept apart from `logoQueue`, which only says whether marks are drawn —
    /// hiding them leaves the backdrop alone and must not bring the black shape back.
    var hidesSilhouette = false
    /// Where the queue's strip sits inside the window, in points down from its top edge, and how tall it is.
    /// The window's own top edge moves between the collapsed and the expanded frame; the marks must not.
    var logoQueueInset: CGFloat = 0
    var logoQueueHeight: CGFloat = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let expandedTopRadius: CGFloat = NotchGeometry.expandedTopRadius
    static let expandedBottomRadius: CGFloat = IslandController.expandedRadius

    static var placeholder: IslandRootView {
        IslandRootView(store: nil, isOpen: false, collapsedSize: CGSize(width: 216, height: 32),
                       collapsedTopRadius: NotchGeometry.collapsedTopRadius, collapsedBottomRadius: 12,
                       lightBorder: false, onOpenStats: {})
    }

    var body: some View {
        GeometryReader { proxy in
            let bounds = proxy.size
            let visible = isOpen || alert != nil
            let size = visible ? (presentationSize ?? bounds) : collapsedSize
            let shape = IslandShape(
                topRadius: isOpen ? Self.expandedTopRadius : collapsedTopRadius,
                bottomRadius: isOpen ? Self.expandedBottomRadius : max(collapsedBottomRadius, alert == nil ? 0 : 14)
            )
            // A collapsed logo queue is the marks alone: no silhouette behind them, so they read as agents
            // sitting on the desktop rather than as a bar. The silhouette comes back the moment the panel
            // opens or an event needs somewhere to be shown — but never because the marks were hidden.
            let bare = hidesSilhouette && !visible
            ZStack(alignment: .top) {
                if !bare {
                    shape.fill(.black)
                        .overlay {
                            if lightBorder && alert == nil { shape.stroke(.white.opacity(0.18), lineWidth: 1) }
                        }
                        .frame(width: size.width, height: size.height)
                }
                content
                    .environment(\.islandTyping, onTyping)
                    .mask(alignment: .top) { shape.frame(width: size.width, height: size.height) }
                if let logoQueue, alert == nil {
                    LogoQueueView(config: logoQueue, light: lightBorder)
                        .frame(width: size.width, height: logoQueueHeight)
                        .padding(.top, logoQueueInset)
                }
            }
            .frame(width: bounds.width, height: bounds.height, alignment: .top)
            .animation(animatesGeometry && !reduceMotion ? IslandAnimation.curve : nil, value: size)
        }
        .ignoresSafeArea()
        .id(store?.settings.settings.language ?? .system)
    }

    @ViewBuilder
    var content: some View {
        if isOpen, showsAlertDetails, let alert {
            IslandAlertDetailView(alert: alert, onOpen: onOpenAlert, onDecide: onDecideAlert,
                                  waitingRequests: waitingRequests, onSelectRequest: onSelectRequest)
                .padding(alert.detailInsets.map {
                    // Never under the silhouette: a notch is 38 pt of hardware on some Macs, and a card narrower
                    // than the usage panel sits squarely in its shadow rather than beside it. A screenshot cannot
                    // show that, which is why the number has to come from the screen and not from the panel.
                    EdgeInsets(top: max($0.top, collapsedSize.height), leading: $0.leading,
                               bottom: $0.bottom, trailing: $0.trailing)
                } ?? EdgeInsets(top: collapsedSize.height + alert.detailTopInset, leading: 24, bottom: 22, trailing: 24))
                .frame(width: alert.detailWidth)
                .fixedSize(horizontal: false, vertical: true)
                .background(GeometryReader { proxy in Color.clear.preference(key: PanelHeightKey.self, value: proxy.size.height) })
                .onPreferenceChange(PanelHeightKey.self, perform: onContentHeight)
                .transition(detailTransition)
        } else if isOpen, let store {
            HoverPanelView(store: store, onOpenStats: onOpenStats, onOpenSettings: onOpenSettings,
                           alert: alert, onOpenAlert: onOpenAlert, onDecideAlert: onDecideAlert,
                           waitingRequests: waitingRequests)
                .frame(width: IslandController.expandedWidth, alignment: .top)
                .fixedSize(horizontal: false, vertical: true)
                .onPreferenceChange(PanelHeightKey.self, perform: onContentHeight)
                .transition(detailTransition)
        } else if let alert {
            IslandAlertCompactView(alert: alert,
                                  cameraWidth: collapsedSize.width - collapsedTopRadius * 2,
                                  height: max(38, collapsedSize.height), onOpen: onOpenAlert,
                                  waitingRequests: waitingRequests)
                .id(alert.id)
                .transition(.opacity.animation(.easeOut(duration: 0.2).delay(0.08)))
        }
    }

    private var detailTransition: AnyTransition {
        if reduceMotion { return .opacity }
        return .asymmetric(
            insertion: .opacity.combined(with: .offset(y: -5)).animation(.easeOut(duration: 0.22).delay(0.12)),
            removal: .opacity.animation(.easeOut(duration: 0.1))
        )
    }
}
