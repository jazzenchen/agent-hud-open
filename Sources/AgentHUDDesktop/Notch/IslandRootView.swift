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
    var showsAlertDetails = false
    var presentationSize: CGSize? = nil
    var animatesGeometry = true
    var onContentHeight: (CGFloat) -> Void = { _ in }
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let expandedTopRadius: CGFloat = NotchGeometry.expandedTopRadius
    static let expandedBottomRadius: CGFloat = NotchController.expandedRadius

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
            let shape = NotchShape(
                topRadius: isOpen ? Self.expandedTopRadius : collapsedTopRadius,
                bottomRadius: isOpen ? Self.expandedBottomRadius : max(collapsedBottomRadius, alert == nil ? 0 : 14)
            )
            ZStack(alignment: .top) {
                shape.fill(.black)
                    .overlay {
                        if lightBorder && alert == nil { shape.stroke(.white.opacity(0.18), lineWidth: 1) }
                    }
                    .frame(width: size.width, height: size.height)
                content
                    .mask(alignment: .top) { shape.frame(width: size.width, height: size.height) }
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
            IslandAlertDetailView(alert: alert, onOpen: onOpenAlert)
                .padding(.horizontal, 24)
                .padding(.top, collapsedSize.height + 16)
                .padding(.bottom, 22)
                .frame(width: NotchController.alertDetailWidth)
                .fixedSize(horizontal: false, vertical: true)
                .background(GeometryReader { proxy in Color.clear.preference(key: PanelHeightKey.self, value: proxy.size.height) })
                .onPreferenceChange(PanelHeightKey.self, perform: onContentHeight)
                .transition(detailTransition)
        } else if isOpen, let store {
            HoverPanelView(store: store, onOpenStats: onOpenStats, onOpenSettings: onOpenSettings,
                           alert: alert, onOpenAlert: onOpenAlert)
                .frame(width: NotchController.expandedWidth, alignment: .top)
                .fixedSize(horizontal: false, vertical: true)
                .onPreferenceChange(PanelHeightKey.self, perform: onContentHeight)
                .transition(detailTransition)
        } else if let alert {
            IslandAlertCompactView(alert: alert,
                                  cameraWidth: collapsedSize.width - collapsedTopRadius * 2,
                                  height: max(38, collapsedSize.height), onOpen: onOpenAlert)
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
