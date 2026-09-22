import AgentHUDCore
import SwiftUI

/// Every colour a request card is allowed to use, in one place.
///
/// Three roles, and nothing outside them: the island's own event colour marks that something is waiting, the product's
/// status palette says what an answer means, and the island's theme supplies every surface and shade of text that is
/// neither. A request is not a quota reading, so it never borrows a level colour to say how it feels.
enum PermissionColor {
    private static let theme = Theme.island

    /// Waiting for the user — the same warm the island already uses for an event that needs them.
    static let signal = Color(IslandAlert.warningAccent)
    /// Letting the call run.
    static let allow = Color(StatusPalette.ok)
    /// Refusing it.
    static let deny = Color(StatusPalette.critical)

    static let text = theme.text
    static let secondary = theme.secondary
    static let tertiary = theme.tertiary
    /// The card being decided, and the boxes inside it.
    static let surface = theme.card
    static let inset = theme.rowBackground
    static let border = theme.cardBorder
    /// A row the pointer is over.
    static let highlight = theme.segmentBackground
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

    var body: some View {
        HStack(spacing: 0) {
            HStack(spacing: 8) {
                AgentLogo(vendor: request.vendor, size: 17)
                Text(request.project ?? request.vendor)
                    .font(.ui(13, .semibold)).lineLimit(1).minimumScaleFactor(0.78)
            }.frame(width: IslandController.alertWingWidth, alignment: .leading)
            Color.clear.frame(width: cameraWidth)
            HStack(spacing: 7) {
                PermissionSymbol(requestID: request.id)
                Text(request.isQuestion ? L10n.text("待回答", "Question")
                     : request.isPlan ? L10n.text("计划待审", "Plan") : L10n.text("待批准", "Waiting"))
                    .font(.ui(12, .medium))
                WaitingCount(waiting: waiting)
            }.frame(width: IslandController.alertWingWidth, alignment: .trailing)
        }
        .foregroundStyle(PermissionColor.text)
        .padding(.horizontal, IslandController.alertSidePadding).frame(height: height)
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
        }.foregroundStyle(PermissionColor.text)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(L10n.text("待审批", "Waiting")).font(.ui(11, .semibold)).foregroundStyle(PermissionColor.secondary)
            Spacer()
            Text("\(queue.count)")
                .font(.tabular(10, .semibold)).foregroundStyle(PermissionColor.signal)
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(PermissionColor.signal.opacity(0.16), in: Capsule())
        }.padding(.horizontal, 4).padding(.bottom, 8)
    }
}

/// The request being decided: everything the other lines leave out, and the answers.
private struct PermissionOpenRow: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                PermissionRowHead(request: request, open: true)
                if let context = request.context {
                    Text(context).font(.tabular(11)).foregroundStyle(PermissionColor.tertiary)
                        .lineLimit(1).truncationMode(.middle)
                }
            }
            if request.isQuestion {
                PermissionQuestionCard(request: request, onDecide: onDecide)
            } else if request.removed != nil || request.added != nil {
                PermissionDiff(removed: request.removed, added: request.added)
            } else if let detail = request.detail {
                Text(detail)
                    .font(.tabular(11)).foregroundStyle(PermissionColor.text.opacity(0.8))
                    .lineLimit(6).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                    .padding(.horizontal, 9).padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(PermissionColor.inset, in: RoundedRectangle(cornerRadius: 7))
            }
            if request.isPlan {
                PlanNotice(request: request, onDecide: onDecide)
            } else if !request.isQuestion {
                PermissionButtons(request: request, onDecide: onDecide)
            }
        }
        .padding(10)
        .background(PermissionColor.surface, in: RoundedRectangle(cornerRadius: 10))
    }
}

/// One line of the queue: enough to decide whether to open it, and a click to do so.
private struct PermissionClosedRow: View {
    let request: PermissionRequest
    let onSelect: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: onSelect) {
            PermissionRowHead(request: request, open: false)
                .padding(.horizontal, 10).padding(.vertical, 9)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(hovering ? PermissionColor.highlight : .clear, in: RoundedRectangle(cornerRadius: 8))
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
                .foregroundStyle(PermissionColor.signal)
                .padding(.horizontal, 4).padding(.vertical, 1)
                .background(PermissionColor.signal.opacity(0.14), in: RoundedRectangle(cornerRadius: 3))
            // An open question shows itself in full below, one question at a time.
            if !(open && request.isQuestion) {
                Text(request.summary)
                    .font(.ui(11)).foregroundStyle(open ? PermissionColor.text.opacity(0.85) : PermissionColor.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 6)
            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(PermissionRowHead.waited(request.at, now: context.date))
                    .font(.tabular(11)).foregroundStyle(PermissionColor.tertiary)
            }
        }
    }

    /// How long the client has been waiting. Seconds while that is still the honest unit, then the shared format.
    static func waited(_ since: Date, now: Date) -> String {
        let seconds = max(0, now.timeIntervalSince(since))
        return seconds < 60 ? "\(Int(seconds))s" : Countdown.compact(seconds)
    }
}

/// The two sides of an edit, in the shape a diff is read in.
private struct PermissionDiff: View {
    let removed: String?
    let added: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            side(removed, sign: "−", tint: PermissionColor.deny)
            side(added, sign: "+", tint: PermissionColor.allow)
        }
        .padding(9).frame(maxWidth: .infinity, alignment: .leading)
        .background(PermissionColor.inset, in: RoundedRectangle(cornerRadius: 7))
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

struct PermissionAlertInlineView: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    var waiting = 1

    var body: some View {
        if request.isQuestion {
            // A question cannot be answered from one line, so inside the panel it is the whole card.
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 6) {
                    PermissionSymbol(requestID: request.id)
                    Text(L10n.text("\(request.vendor) 在问你", "\(request.vendor) asks")).font(.ui(12, .medium))
                    WaitingCount(waiting: waiting)
                }
                PermissionQuestionCard(request: request, onDecide: onDecide)
            }.foregroundStyle(PermissionColor.text).padding(.vertical, 8)
        } else {
            HStack(spacing: 10) {
                PermissionSymbol(requestID: request.id)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Text(request.isPlan ? L10n.text("\(request.vendor) 有计划待审", "\(request.vendor) has a plan to review")
                                            : L10n.text("\(request.vendor) 等待批准", "\(request.vendor) needs approval"))
                            .font(.ui(12, .medium))
                        WaitingCount(waiting: waiting)
                    }
                    Text(request.summary).font(.ui(10)).foregroundStyle(PermissionColor.secondary).lineLimit(1)
                }
                Spacer(minLength: 10)
                if request.isPlan {
                    PlanNotice.dismiss(onDecide)
                } else {
                    PermissionButtons(request: request, onDecide: onDecide, compact: true)
                }
            }.foregroundStyle(PermissionColor.text).padding(.vertical, 8)
        }
    }
}

/// A plan is approved where it was written: the client offers choices about how to go on that an allow cannot carry,
/// and a plan deserves more reading than a card can hold. The card says where to go and can be put away; putting it
/// away answers nothing, and the client's own dialog is there either way.
private struct PlanNotice: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void

    var body: some View {
        HStack(spacing: 8) {
            Text(L10n.text("请到 \(request.vendor) 里审批这份计划", "Review this plan in \(request.vendor)"))
                .font(.ui(11)).foregroundStyle(PermissionColor.secondary).lineLimit(1)
            Spacer(minLength: 8)
            Self.dismiss(onDecide)
        }
    }

    static func dismiss(_ onDecide: @escaping (PermissionDecision) -> Void) -> some View {
        Button { onDecide(.leave) } label: {
            Text(L10n.text("知道了", "Dismiss")).font(.ui(11, .medium)).lineLimit(1)
                .foregroundStyle(PermissionColor.secondary)
                .padding(.horizontal, 9).frame(height: 22)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(PermissionColor.border, lineWidth: 1))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-permission-dismiss")
    }
}

/// How many requests are waiting in all, shown only when this card is not the whole story.
private struct WaitingCount: View {
    let waiting: Int

    var body: some View {
        if waiting > 1 {
            Text("×\(waiting)")
                .font(.tabular(10, .semibold)).foregroundStyle(PermissionColor.secondary)
                .padding(.horizontal, 5).padding(.vertical, 2)
                .background(PermissionColor.highlight, in: RoundedRectangle(cornerRadius: 4))
                .accessibilityLabel(L10n.text("共 \(waiting) 个待批准", "\(waiting) requests waiting"))
        }
    }
}

/// The answers, each in the colour of what it means: green lets the call run, red refuses it, and the middle one is
/// the same green held back — it allows this call and every call like it, which is a bigger thing to do by accident.
/// It appears only when the client offered a rule of its own, and it is that offer, echoed back untouched. Saying
/// nothing is also an answer, and the way to give it is to do nothing: the client keeps waiting and its own prompt is
/// still there in the terminal.
private struct PermissionButtons: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    var compact = false

    var body: some View {
        HStack(spacing: 6) {
            button(L10n.text("拒绝", "Deny"), symbol: "xmark", identifier: "island-permission-deny",
                   fill: PermissionColor.deny.opacity(0.14), stroke: PermissionColor.deny.opacity(0.3),
                   foreground: PermissionColor.deny) { onDecide(.deny) }
            Spacer(minLength: 8)
            if let always = request.alwaysAllow {
                // A seal, not a tick: this answer outlives the call it was given for.
                button(L10n.text("总是允许", "Always allow"), symbol: "checkmark.seal",
                       identifier: "island-permission-always",
                       fill: .clear, stroke: PermissionColor.allow.opacity(0.4),
                       foreground: PermissionColor.allow) { onDecide(.allowAlways(always)) }
            }
            button(L10n.text("批准一次", "Allow once"), symbol: "checkmark", identifier: "island-permission-approve",
                   fill: PermissionColor.allow, stroke: .clear, foreground: .black) { onDecide(.allow) }
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
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(identifier)
    }
}

/// Keeps drawing attention for as long as the request is unanswered, which is the difference between a reminder that
/// holds and news that has been read.
private struct PermissionSymbol: View {
    let requestID: String
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Image(systemName: "hand.raised.fill")
            .font(.system(size: 14, weight: .semibold)).foregroundStyle(PermissionColor.signal)
            .symbolEffect(.pulse, options: .repeating, value: requestID)
            .symbolEffectsRemoved(reduceMotion)
    }
}
