import SwiftUI
import AgentHUDCore

/// The collapsed HUD on a screen in Simple mode: a small floating panel with no glow behind it. Each agent
/// is its mark, with its tightest window drawn as a ring round the mark, written as a percentage beside it,
/// or both. The ring and the number take the window's level colour. Nothing moves unless bot logos are switched
/// on: a working agent's mark is then its bot, and an idle one visits as a bot now and then.
struct SimpleMeterView: View {
    let config: SimpleMeterConfig
    /// The island draws the panel behind the figures, so that the same frame can grow into the usage panel.
    /// Left off, the view draws its own fill and hairline, as a preview with no island does.
    var onIsland = false
    /// The panel's glass is drawn with the line that grows out of it, as one piece (`JoinedGlass`), on a dock that
    /// separates its agents. The view then draws its figures alone, as on the island.
    var glassJoined = false
    /// Whether the view draws the panel's surface behind its figures itself.
    var drawsSurface: Bool { !onIsland && !glassJoined }
    /// The look of the HUD the panel belongs to: the island's round it, the standard one in a preview with none.
    @Environment(\.hudLook) private var look
    /// Glass drawn as its stand-in, in a preview or a picture of the panel.
    @Environment(\.drawsGlassStandIn) private var glassStandIn
    @Environment(\.botLogos) private var botLogos

    private var theme: Theme { look.theme }

    var body: some View {
        let panel = config.panelSize
        let insets = config.insets
        let shape = RoundedRectangle(cornerRadius: config.cornerRadius, style: .continuous)
        let layout = config.edge.isHorizontal
            ? AnyLayout(HStackLayout(spacing: config.itemGap))
            : AnyLayout(VStackLayout(alignment: .leading, spacing: config.itemGap))
        layout {
            ForEach(config.items) { cell($0) }
        }
        .padding(EdgeInsets(top: insets.top, leading: insets.left, bottom: insets.bottom, trailing: insets.right))
        .frame(width: panel.width, height: panel.height, alignment: .leading)
        .background {
            if drawsSurface {
                HUDSurface(shape, look: look, fill: theme.islandFill, contact: nil)
                    .overlay { shape.strokeBorder(look.outline(theme.islandOutline, standIn: glassStandIn), lineWidth: 1) }
            }
        }
        .padding(margin, SimpleMeterConfig.edgeMargin)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: alignment)
        // The marks take the artwork made for the panel's surface, also in a window set to the other scheme.
        .environment(\.colorScheme, look.colorScheme)
        .accessibilityElement(children: .combine)
    }

    /// The ring takes the level's colour and the number, which is text, the level's text colour.
    private func cell(_ item: SimpleMeterItem) -> some View {
        let tint = item.level.map { theme.status($0) } ?? theme.idle
        let ink = item.level.map { theme.statusText($0) } ?? theme.idleText
        return HStack(spacing: config.labelSpacing) {
            ZStack {
                if config.showsRings {
                    Circle().strokeBorder(theme.meterTrack, lineWidth: config.ringWidth)
                    Circle().inset(by: config.ringWidth / 2)
                        .trim(from: 0, to: min(1, max(0, (item.usedPct ?? 0) / 100)))
                        .stroke(tint, style: StrokeStyle(lineWidth: config.ringWidth, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                }
                // With bots off the mark is its logo, wearing a waiting request's or a badge's sign still.
                BotMark(vendor: item.vendor, size: config.logo, state: item.shown, cue: item.cue, staysBot: false)
                    .environment(\.botLogos, botLogos && config.bots)
            }
            .frame(width: config.mark, height: config.mark)
            if config.showsNumbers {
                Text(item.label)
                    .font(.tabular(config.fontSize, .regular))
                    .foregroundStyle(ink)
                    .lineLimit(1)
                    .fixedSize()
                    .frame(width: config.edge.isHorizontal ? config.labelWidth(item) : config.labelColumn, alignment: .trailing)
            }
        }
        .accessibilityLabel("\(item.vendor) \(item.label)")
    }

    /// The side the margin is on, and the corner the panel is held to: against its edge, centred along it.
    private var margin: Edge.Set {
        switch config.edge {
        case .top: return .top
        case .bottom: return .bottom
        case .left: return .leading
        case .right: return .trailing
        }
    }

    private var alignment: Alignment {
        switch config.edge {
        case .top: return .top
        case .bottom: return .bottom
        case .left: return .leading
        case .right: return .trailing
        }
    }
}
