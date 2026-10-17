import AgentHUDCore
import SwiftUI

/// Every colour a request card is allowed to use, in one place.
///
/// Three roles, and nothing outside them: the island's own event colour marks that something is waiting, the product's
/// status palette says what an answer means, and the theme of the HUD the card is in supplies every surface and shade
/// of text that is neither. A request is not a quota reading, so it never borrows a level colour to say how it feels.
struct PermissionColor {
    private let theme: Theme

    /// A request card's colours on a HUD drawn with `theme`.
    init(_ theme: Theme) { self.theme = theme }

    /// Waiting for the user — the same warm the island already uses for an event that needs them.
    var signal: Color { Color(IslandAlert.warningAccent) }
    /// Letting the call run.
    var allow: Color { Color(StatusPalette.ok) }
    /// Refusing it.
    var deny: Color { Color(StatusPalette.critical) }
    /// The two as an outline or a faint fill, an outlined button's edge or Deny's ground: the theme's level fills, the
    /// bright hues on a dark HUD and the light HUD's deeper ones on a light one, where the bright hues at a fraction
    /// all but vanish on near-white and the button reads as loose text.
    var allowEdge: Color { theme.status(.ok) }
    var denyEdge: Color { theme.status(.critical) }
    /// Ink on a fill of `allow`, which stays bright whatever the theme.
    var onAllow: Color { theme.onAccent }
    /// The three as text and thin glyphs: themselves on a dark HUD, darker on a light one, where the bright fills would
    /// not be read.
    var signalText: Color { theme.alertWarningText }
    var allowText: Color { theme.statusText(.ok) }
    var denyText: Color { theme.statusText(.critical) }

    var text: Color { theme.text }
    var secondary: Color { theme.secondary }
    var tertiary: Color { theme.tertiary }
    /// The card being decided, and the boxes inside it.
    var surface: Color { theme.card }
    var inset: Color { theme.insetSurface }
    var border: Color { theme.cardBorder }
    /// A row the pointer is over.
    var highlight: Color { theme.segmentBackground }
}

extension EnvironmentValues {
    /// The colours of a request card in the HUD it is drawn in.
    var permissionColor: PermissionColor { PermissionColor(hudTheme) }
}

/// A tool call waiting for its user, in the island's three sizes.
///
/// The collapsed form is the whole reminder: it stays until the request is answered or its client takes it back, so it
/// says who is asking and that something is waiting, and nothing else. Hovering is what turns it into a decision, and
/// moving away puts it back. Clicking the collapsed form does nothing on purpose — approving is not something to do by
/// brushing past the notch.
struct PermissionAlertCompactView: View {
    let request: PermissionRequest
    let cameraWidth: CGFloat
    let height: CGFloat
    /// Every request waiting for this user, on any screen. One is the card itself and needs no number.
    var waiting = 1
    @Environment(\.stripScale) private var scale
    @Environment(\.compactWings) private var wings
    @Environment(\.permissionColor) private var color

    /// What a request's reminder says beside its project: a question, a plan, or a call waiting to be allowed.
    static func status(of request: PermissionRequest) -> String {
        request.isQuestion ? L10n.text("待回答", "Question")
            : request.isPlan ? L10n.text("计划待审", "Plan") : L10n.text("待批准", "Waiting")
    }

    /// A length at the scale of the dock this reminder grows from.
    private func s(_ value: CGFloat) -> CGFloat { (value * scale).rounded() }
    /// The size of its name or status, from their size beside the notch: on a dock, the size of every word a dock writes.
    private func w(_ value: CGFloat) -> CGFloat { s(CompactWings.words(value, onDock: wings != nil)) }
    /// The size of the symbol beside its status, from its size beside the notch: a size smaller on a dock.
    private func t(_ value: CGFloat) -> CGFloat { s(CompactWings.size(value, onDock: wings != nil)) }

    var body: some View {
        HStack(spacing: 0) {
            HStack(spacing: s(8)) {
                ReminderMarkView(vendor: request.vendor, size: s(17), mark: .request(request))
                Text(request.project ?? request.vendor)
                    .font(.ui(w(13), .semibold)).lineLimit(1).minimumScaleFactor(0.78)
            }.frame(width: wings?.leading ?? s(IslandController.alertWingWidth), alignment: .leading)
            Color.clear.frame(width: wings?.gap ?? cameraWidth)
            HStack(spacing: s(7)) {
                PermissionSymbol(requestID: request.id, size: t(14))
                Text(Self.status(of: request))
                    .font(.ui(w(12), .medium))
                WaitingCount(waiting: waiting, scale: scale)
            }.frame(width: wings?.trailing ?? s(IslandController.alertWingWidth), alignment: .trailing)
        }
        .foregroundStyle(color.text)
        .padding(.horizontal, wings?.padding ?? s(IslandController.alertSidePadding)).frame(height: height)
        .accessibilityIdentifier("island-alert-permissionRequest")
        .accessibilityLabel("\(request.vendor) · \(request.isQuestion ? L10n.text("在问你问题", "Asks a question") : request.isPlan ? L10n.text("有计划待审", "Has a plan to review") : L10n.text("等待批准", "Needs approval")) · \(request.summary)"
                            + (waiting > 1 ? " · " + L10n.text("共 \(waiting) 个", "\(waiting) waiting") : ""))
    }
}

/// The queue, one card open and the rest a line each. Opening another is a click on its line, which is why the closed
/// lines carry what it takes to choose: whose session, what kind of call, on what, and how long it has waited.
struct PermissionAlertDetailView: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    var all: [PermissionRequest] = []
    var onSelect: (String) -> Void = { _ in }
    @Environment(\.permissionColor) private var color

    private var queue: [PermissionRequest] { all.isEmpty ? [request] : all }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            VStack(spacing: 2) {
                ForEach(queue) { item in
                    if item.id == request.id {
                        PermissionOpenRow(request: item, onDecide: onDecide)
                    } else {
                        PermissionClosedRow(request: item, onSelect: { onSelect(item.id) })
                    }
                }
            }
        }.foregroundStyle(color.text)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(L10n.text("待审批", "Waiting")).font(.ui(11, .semibold)).foregroundStyle(color.secondary)
            Spacer()
            Text("\(queue.count)")
                .font(.tabular(10, .semibold)).foregroundStyle(color.signalText)
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(color.signal.opacity(0.16), in: Capsule())
        }.padding(.horizontal, 4).padding(.bottom, 8)
    }
}

/// The request being decided: everything the other lines leave out, and the answers.
private struct PermissionOpenRow: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    @Environment(\.permissionColor) private var color

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                PermissionRowHead(request: request, open: true)
                if let context = request.context {
                    Text(context).font(.tabular(11)).foregroundStyle(color.tertiary)
                        .lineLimit(1).truncationMode(.middle)
                }
            }
            if request.answersQuestion {
                PermissionQuestionCard(request: request, onDecide: onDecide)
            } else if request.isQuestion {
                ClientQuestionCard(request: request, onDecide: onDecide)
            } else if request.answersPlan {
                PermissionPlanCard(request: request, onDecide: onDecide)
            } else if request.removed != nil || request.added != nil {
                PermissionDiff(removed: request.removed, added: request.added)
            } else if let detail = request.detail {
                PermissionDetail(text: detail, lines: 6)
            }
            if request.isPlan {
                if !request.answersPlan { PlanNotice(request: request, onDecide: onDecide) }
            } else if !request.isQuestion {
                if let rule = request.alwaysAllowRule { PermissionRuleNote(rule: rule) }
                PermissionButtons(request: request, onDecide: onDecide)
            }
        }
        .padding(10)
        .background(color.surface, in: RoundedRectangle(cornerRadius: 10))
    }
}

/// One line of the queue: enough to decide whether to open it, and a click to do so.
struct PermissionClosedRow: View {
    let request: PermissionRequest
    let onSelect: () -> Void
    @State private var hovering = false
    @Environment(\.permissionColor) private var color

    var body: some View {
        Button(action: onSelect) {
            PermissionRowHead(request: request, open: false)
                .padding(.horizontal, 10).padding(.vertical, 9)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(hovering ? color.highlight : .clear, in: RoundedRectangle(cornerRadius: 8))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityLabel(L10n.text("打开 \(request.vendor) 的请求：\(request.summary)",
                                      "Open \(request.vendor) request: \(request.summary)"))
    }
}

/// The line every request shows, open or not: whose session, what kind of call, on what, and how long it has waited.
private struct PermissionRowHead: View {
    let request: PermissionRequest
    let open: Bool
    @Environment(\.permissionColor) private var color

    var body: some View {
        HStack(spacing: 8) {
            // Two clients can wait on the same project at once, so the row says whose request it is before it says
            // anything else. The tool rides inside its own badge; one mark each, neither repeating the other.
            AgentLogo(vendor: request.vendor, size: 13).opacity(open ? 1 : 0.8)
            Text(request.project ?? request.vendor)
                .font(.ui(12, .medium)).lineLimit(1).minimumScaleFactor(0.8)
            HStack(spacing: 3) {
                Image(systemName: request.symbol).font(.system(size: 8, weight: .bold))
                Text(request.badge).font(.tabular(10, .bold))
            }
                .foregroundStyle(color.signalText)
                .padding(.horizontal, 4).padding(.vertical, 1)
                .background(color.signal.opacity(0.14), in: RoundedRectangle(cornerRadius: 3))
            // An open question shows itself in full below, one question at a time, and so does a command that came
            // without a description of its own.
            if !(open && (request.isQuestion || request.showsDetail && request.detail == request.summary)) {
                Text(request.summary)
                    .font(.ui(11)).foregroundStyle(open ? color.text.opacity(0.85) : color.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 6)
            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(PermissionRowHead.waited(request.at, now: context.date))
                    .font(.tabular(11)).foregroundStyle(color.tertiary)
            }
        }
    }

    /// How long the client has been waiting (`Countdown.waited`).
    static func waited(_ since: Date, now: Date) -> String { Countdown.waited(now.timeIntervalSince(since)) }
}

private extension PermissionRequest {
    /// Whether an open card shows `detail` in a box of its own: a question shows its questions and an edit its diff.
    var showsDetail: Bool { !isQuestion && removed == nil && added == nil && detail != nil }
}

/// What the call would run — the command, the file, the tool's input — as it will be run, cut to `lines` lines.
private struct PermissionDetail: View {
    let text: String
    let lines: Int
    @Environment(\.permissionColor) private var color

    var body: some View {
        Text(text)
            .font(.tabular(11)).foregroundStyle(color.text.opacity(0.8))
            .lineLimit(lines).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            .padding(.horizontal, 9).padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(color.inset, in: RoundedRectangle(cornerRadius: 7))
    }
}

/// What Always allow adds, there to read before it is given: the client's own rule, as its settings will hold it.
private struct PermissionRuleNote: View {
    let rule: String
    @Environment(\.permissionColor) private var color

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 5) {
            Image(systemName: "checkmark.seal").font(.system(size: 9, weight: .bold))
                .foregroundStyle(color.allowText.opacity(0.8))
            Text(L10n.text("总是允许会添加", "Always allow adds")).font(.ui(11)).foregroundStyle(color.secondary)
                .fixedSize()
            Text(rule).font(.tabular(11)).foregroundStyle(color.text.opacity(0.85))
                .lineLimit(2).truncationMode(.middle).textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The two sides of an edit, in the shape a diff is read in.
private struct PermissionDiff: View {
    let removed: String?
    let added: String?
    @Environment(\.permissionColor) private var color

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            side(removed, sign: "−", tint: color.denyText)
            side(added, sign: "+", tint: color.allowText)
        }
        .padding(9).frame(maxWidth: .infinity, alignment: .leading)
        .background(color.inset, in: RoundedRectangle(cornerRadius: 7))
    }

    @ViewBuilder
    private func side(_ text: String?, sign: String, tint: Color) -> some View {
        if let text {
            ForEach(Array(text.split(separator: "\n", omittingEmptySubsequences: false).enumerated()), id: \.offset) { _, line in
                HStack(alignment: .top, spacing: 6) {
                    Text(sign).font(.tabular(11, .bold)).foregroundStyle(tint.opacity(0.8))
                    Text(String(line)).font(.tabular(11)).foregroundStyle(tint).lineLimit(1).truncationMode(.tail)
                    Spacer(minLength: 0)
                }
            }
        }
    }
}

/// A request that arrived while the usage panel was open, as a row at its top. Its answers are right there, so what
/// they would let run is too: the command, the file or the tool's input, and the rule Always allow would add.
struct PermissionAlertInlineView: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    var waiting = 1
    @Environment(\.permissionColor) private var color

    /// A plan answered in its client has nothing here to read before allowing it.
    private var subject: String? { request.isPlan ? nil : request.detail }

    var body: some View {
        if request.isQuestion || request.answersPlan {
            // A question cannot be answered from one line, nor a plan read in one, so inside the panel each is the
            // whole card.
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 6) {
                    PermissionSymbol(requestID: request.id)
                    Text(request.isQuestion ? L10n.text("\(request.vendor) 在问你", "\(request.vendor) asks")
                                            : L10n.text("\(request.vendor) 有计划待审", "\(request.vendor) has a plan to review"))
                        .font(.ui(12, .medium))
                    WaitingCount(waiting: waiting)
                }
                if request.answersQuestion {
                    PermissionQuestionCard(request: request, onDecide: onDecide)
                } else if request.isQuestion {
                    ClientQuestionCard(request: request, onDecide: onDecide)
                } else {
                    PermissionPlanCard(request: request, onDecide: onDecide, planHeight: 120)
                }
            }.foregroundStyle(color.text).padding(.vertical, 8)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 10) {
                    PermissionSymbol(requestID: request.id)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 6) {
                            Text(request.isPlan ? L10n.text("\(request.vendor) 有计划待审", "\(request.vendor) has a plan to review")
                                                : L10n.text("\(request.vendor) 等待批准", "\(request.vendor) needs approval"))
                                .font(.ui(12, .medium))
                            WaitingCount(waiting: waiting)
                        }
                        // A command without a description of its own is its own summary, and it is shown in full below.
                        if request.summary != subject {
                            Text(request.summary).font(.ui(11)).foregroundStyle(color.secondary).lineLimit(1)
                        }
                    }
                    Spacer(minLength: 10)
                    if request.isPlan {
                        PlanNotice.dismiss(onDecide, color: color)
                    } else {
                        PermissionButtons(request: request, onDecide: onDecide, compact: true)
                    }
                }
                if let subject { PermissionDetail(text: subject, lines: 3) }
                if !request.isPlan, let rule = request.alwaysAllowRule { PermissionRuleNote(rule: rule) }
            }.foregroundStyle(color.text).padding(.vertical, 8)
        }
    }
}

/// A plan from a client that takes no answer to it from a hook is approved where it was written. The card says where
/// to go and can be put away; putting it away answers nothing, and the client's own dialog is there either way.
private struct PlanNotice: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    @Environment(\.permissionColor) private var color

    var body: some View {
        HStack(spacing: 8) {
            Text(L10n.text("请到 \(request.vendor) 里审批这份计划", "Review this plan in \(request.vendor)"))
                .font(.ui(11)).foregroundStyle(color.secondary).lineLimit(1)
            Spacer(minLength: 8)
            Self.dismiss(onDecide, color: color)
        }
    }

    /// The button that puts the card away, in the colours of the card it is on.
    static func dismiss(_ onDecide: @escaping (PermissionDecision) -> Void, color: PermissionColor) -> some View {
        Button { onDecide(.leave) } label: {
            Text(L10n.text("知道了", "Dismiss")).font(.ui(11, .medium)).lineLimit(1)
                .foregroundStyle(color.secondary)
                .padding(.horizontal, 9).frame(height: 22)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(color.border, lineWidth: 1))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-permission-dismiss")
    }
}

/// How many requests are waiting in all, shown only when this card is not the whole story.
private struct WaitingCount: View {
    let waiting: Int
    var scale: CGFloat = 1
    @Environment(\.permissionColor) private var color

    var body: some View {
        if waiting > 1 {
            Text("×\(waiting)")
                .font(.tabular((10 * scale).rounded(), .semibold)).foregroundStyle(color.secondary)
                .padding(.horizontal, (5 * scale).rounded()).padding(.vertical, (2 * scale).rounded())
                .background(color.highlight, in: RoundedRectangle(cornerRadius: 4))
                .accessibilityLabel(L10n.text("共 \(waiting) 个待批准", "\(waiting) requests waiting"))
        }
    }
}

/// The answers, each in the colour of what it means: green lets the call run, red refuses it, and the middle one is
/// the same green held back — it allows this call and every call like it, which is a bigger thing to do by accident.
/// It appears only when the client offered a rule of its own, and it is that offer, echoed back untouched and written
/// out beside the answers (`PermissionRuleNote`). Saying nothing is also an answer, and the way to give it is to do
/// nothing: the client keeps waiting and its own prompt is still there in the terminal.
private struct PermissionButtons: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    var compact = false
    @Environment(\.permissionColor) private var color

    var body: some View {
        HStack(spacing: 6) {
            button(L10n.text("拒绝", "Deny"), symbol: "xmark", identifier: "island-permission-deny",
                   fill: color.denyEdge.opacity(0.14), stroke: color.denyEdge.opacity(0.3),
                   foreground: color.denyText) { onDecide(.deny) }
            Spacer(minLength: 8)
            if let always = request.alwaysAllow {
                // A seal, not a tick: this answer outlives the call it was given for.
                button(L10n.text("总是允许", "Always allow"), symbol: "checkmark.seal",
                       identifier: "island-permission-always",
                       fill: .clear, stroke: color.allowEdge.opacity(0.4),
                       foreground: color.allowText) { onDecide(.allowAlways(always)) }
            }
            button(request.allowLabel, symbol: "checkmark", identifier: "island-permission-approve",
                   fill: color.allow, stroke: .clear, foreground: color.onAllow) { onDecide(.allow) }
        }
    }

    private func button(_ title: String, symbol: String, identifier: String, fill: Color, stroke: Color,
                        foreground: Color, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: symbol).font(.system(size: 9, weight: .bold))
                Text(title).font(.ui(11, .medium)).lineLimit(1)
            }
                .foregroundStyle(foreground)
                .padding(.horizontal, 9).frame(height: 22)
                .background(fill, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(stroke, lineWidth: 1))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(identifier)
    }
}

/// Keeps drawing attention for as long as the request is unanswered, which is the difference between a reminder that
/// holds and news that has been read.
private struct PermissionSymbol: View {
    let requestID: String
    var size: CGFloat = 14
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.permissionColor) private var color

    var body: some View {
        Image(systemName: "hand.raised.fill")
            .font(.system(size: size, weight: .semibold)).foregroundStyle(color.signalText)
            .symbolEffect(.pulse, options: .repeating, value: requestID)
            .symbolEffectsRemoved(reduceMotion)
    }
}
