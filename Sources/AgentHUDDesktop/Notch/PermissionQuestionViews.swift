import AgentHUDCore
import AppKit
import SwiftUI

/// What the user has chosen so far on a question card, one entry per question.
///
/// It lives apart from the card: the island redraws its cards as the pointer comes and goes, and neither is a reason
/// to lose an answer half given. It goes when its request leaves the HUD.
@MainActor
@Observable
final class QuestionDraft {
    /// The question on screen.
    var page = 0
    /// The offered answers picked, by question.
    var picks: [Int: Set<Int>] = [:]
    /// The user's own words, by question, and the questions where those words are the answer rather than a draft.
    var own: [Int: String] = [:]
    var ownPicked: Set<Int> = []

    private static var drafts: [String: QuestionDraft] = [:]

    static func draft(for id: String) -> QuestionDraft {
        if let draft = drafts[id] { return draft }
        let draft = QuestionDraft()
        drafts[id] = draft
        return draft
    }

    /// Keeps the drafts of the requests still waiting and forgets the rest.
    static func keep(_ ids: Set<String>) {
        drafts = drafts.filter { ids.contains($0.key) }
    }

    /// Picks an offered answer. A question with one answer takes this one instead of whatever it had; one with
    /// several adds or removes it.
    func pick(_ option: Int, of index: Int, in question: PermissionQuestion) {
        if question.multiSelect {
            picks[index, default: []].formSymmetricDifference([option])
        } else {
            picks[index] = [option]
            ownPicked.remove(index)
        }
    }

    /// Going into the field is choosing to answer in one's own words; with a single answer that sets the offered
    /// ones aside.
    func chooseOwn(of index: Int, in question: PermissionQuestion) {
        ownPicked.insert(index)
        if !question.multiSelect { picks[index] = [] }
    }

    func type(_ text: String, of index: Int, in question: PermissionQuestion) {
        own[index] = text
        chooseOwn(of: index, in: question)
    }

    /// Leaves a question without an answer. What was typed stays, in case the user comes back to it.
    func skip(_ index: Int) {
        picks[index] = []
        ownPicked.remove(index)
    }

    /// The answer as the client files it: an offered label, several in the order they were offered, the user's own
    /// words — or nothing yet.
    func answer(_ index: Int, of question: PermissionQuestion) -> String? {
        var parts = question.options.indices.filter { picks[index]?.contains($0) == true }.map { question.options[$0].label }
        if ownPicked.contains(index), let text = own[index]?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty {
            parts.append(text)
        }
        return parts.isEmpty ? nil : parts.joined(separator: ", ")
    }

    /// The answers given, each filed under its question. A question skipped is simply not among them, and the client
    /// reads that as the user leaving it open.
    func answers(for questions: [PermissionQuestion]) -> [String: String] {
        var answers: [String: String] = [:]
        for (index, question) in questions.enumerated() {
            if let answer = answer(index, of: question) { answers[question.question] = answer }
        }
        return answers
    }
}

/// Whether the user is typing on the island, so it stays open under their hands whatever the pointer does.
struct IslandTypingKey: EnvironmentKey {
    static let defaultValue: @MainActor (Bool) -> Void = { _ in }
}

extension EnvironmentValues {
    var islandTyping: @MainActor (Bool) -> Void {
        get { self[IslandTypingKey.self] }
        set { self[IslandTypingKey.self] = newValue }
    }
}

/// A question a client put to its user, answered the way the client's own dialog answers it: one of the offered
/// answers, several where the question allows it, or the user's own words. Several questions are answered one after
/// the other and sent together. Any of them can be skipped: the client hears which were left open, which is a gentler
/// thing to be told than a refusal.
struct PermissionQuestionCard: View {
    let request: PermissionRequest
    let onDecide: (PermissionDecision) -> Void
    @Environment(\.islandTyping) private var islandTyping

    private var draft: QuestionDraft { QuestionDraft.draft(for: request.id) }
    private var questions: [PermissionQuestion] { request.questions }
    private var page: Int { min(max(0, draft.page), questions.count - 1) }
    private var question: PermissionQuestion { questions[page] }
    private var isLast: Bool { page == questions.count - 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 7) {
                if questions.count > 1 {
                    Text("\(page + 1)/\(questions.count)")
                        .font(.tabular(10, .semibold)).foregroundStyle(PermissionColor.signal)
                        .padding(.horizontal, 5).padding(.vertical, 1)
                        .background(PermissionColor.signal.opacity(0.14), in: RoundedRectangle(cornerRadius: 4))
                }
                Text(question.question)
                    .font(.ui(12, .semibold)).foregroundStyle(PermissionColor.text)
                    .lineLimit(5).fixedSize(horizontal: false, vertical: true)
            }
            if question.multiSelect {
                Text(L10n.text("可多选", "Choose any")).font(.ui(10)).foregroundStyle(PermissionColor.tertiary)
            }
            VStack(spacing: 3) {
                ForEach(Array(question.options.enumerated()), id: \.offset) { index, option in
                    optionRow(index, option)
                }
                ownRow
            }
            buttons
        }
        .id("\(request.id)-\(page)")
    }

    private func optionRow(_ index: Int, _ option: PermissionQuestion.Option) -> some View {
        let picked = draft.picks[page]?.contains(index) == true
        return Button { draft.pick(index, of: page, in: question) } label: {
            HStack(alignment: .top, spacing: 8) {
                marker(picked).padding(.top, 1)
                VStack(alignment: .leading, spacing: 2) {
                    Text(option.label).font(.ui(11, .medium)).foregroundStyle(PermissionColor.text)
                        .lineLimit(2).fixedSize(horizontal: false, vertical: true)
                    if let description = option.description {
                        Text(description).font(.ui(10)).foregroundStyle(PermissionColor.secondary)
                            .lineLimit(2).fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 9).padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(picked ? PermissionColor.highlight : PermissionColor.inset, in: RoundedRectangle(cornerRadius: 7))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("island-question-option-\(index)")
        .accessibilityAddTraits(picked ? .isSelected : [])
    }

    /// The user's own words, laid out like the offered answers above it: a name, and the field where a description
    /// would be.
    private var ownRow: some View {
        let picked = draft.ownPicked.contains(page)
        return HStack(alignment: .top, spacing: 8) {
            marker(picked).padding(.top, 1)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.text("其他", "Other")).font(.ui(11, .medium)).foregroundStyle(PermissionColor.text)
                IslandTextField(
                    text: draft.own[page] ?? "",
                    placeholder: L10n.text("自己输入回答", "Type your own answer"),
                    onFocus: { draft.chooseOwn(of: page, in: question) },
                    onChange: { draft.type($0, of: page, in: question) },
                    onSubmit: advance,
                    onTyping: islandTyping
                )
                .frame(height: 15)
                .accessibilityIdentifier("island-question-own")
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 9).padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(picked ? PermissionColor.highlight : PermissionColor.inset, in: RoundedRectangle(cornerRadius: 7))
        .contentShape(Rectangle())
        .onTapGesture { draft.chooseOwn(of: page, in: question) }
    }

    /// A ring for an answer that stands alone, a box for one of several; filled once chosen.
    private func marker(_ picked: Bool) -> some View {
        let symbol = question.multiSelect ? (picked ? "checkmark.square.fill" : "square")
                                          : (picked ? "largecircle.fill.circle" : "circle")
        return Image(systemName: symbol)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(picked ? PermissionColor.text : PermissionColor.tertiary)
            .frame(width: 12)
    }

    private var buttons: some View {
        HStack(spacing: 6) {
            QuestionButton(title: L10n.text("跳过", "Skip"), symbol: nil, identifier: "island-question-skip",
                           fill: .clear, stroke: PermissionColor.border, foreground: PermissionColor.secondary) {
                draft.skip(page)
                next()
            }
            Spacer(minLength: 8)
            if page > 0 {
                QuestionButton(title: L10n.text("返回", "Back"), symbol: "chevron.left", identifier: "island-question-back",
                               fill: .clear, stroke: PermissionColor.border, foreground: PermissionColor.secondary) {
                    draft.page = page - 1
                }
            }
            let ready = draft.answer(page, of: question) != nil
            QuestionButton(title: isLast ? L10n.text("提交", "Submit") : L10n.text("继续", "Next"),
                           symbol: isLast ? "checkmark" : "chevron.right", identifier: "island-question-next",
                           fill: ready ? PermissionColor.allow : PermissionColor.highlight, stroke: .clear,
                           foreground: ready ? .black : PermissionColor.tertiary, trailing: !isLast, action: advance)
                .disabled(!ready)
        }
    }

    /// On to the next question once this one has an answer.
    private func advance() {
        guard draft.answer(page, of: question) != nil else { return }
        next()
    }

    /// The next question, or, after the last one, the answers given to the client.
    private func next() {
        if isLast { onDecide(.answer(draft.answers(for: questions))) } else { draft.page = page + 1 }
    }
}

private struct QuestionButton: View {
    let title: String
    let symbol: String?
    let identifier: String
    let fill: Color
    let stroke: Color
    let foreground: Color
    /// A step forward reads with its arrow after the words.
    var trailing = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let symbol, !trailing { Image(systemName: symbol).font(.system(size: 9, weight: .bold)) }
                Text(title).font(.ui(11, .medium)).lineLimit(1)
                if let symbol, trailing { Image(systemName: symbol).font(.system(size: 9, weight: .bold)) }
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

/// A one-line field on the island.
///
/// The island never takes the keyboard on its own: a card arriving while the user types elsewhere must not catch
/// their keys. Clicking into the field is what hands the keyboard to the island, and it goes back the moment
/// typing ends — the answer is sent, Escape is pressed, or the user clicks somewhere else. Return while an input
/// method is still composing belongs to the input method; only a finished line is submitted.
struct IslandTextField: NSViewRepresentable {
    let text: String
    let placeholder: String
    var onFocus: () -> Void = {}
    let onChange: (String) -> Void
    let onSubmit: () -> Void
    let onTyping: @MainActor (Bool) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> KeyboardField {
        let field = KeyboardField()
        field.delegate = context.coordinator
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: 11)
        field.textColor = NSColor(PermissionColor.text)
        field.lineBreakMode = .byTruncatingTail
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: [
            .foregroundColor: NSColor(PermissionColor.tertiary), .font: NSFont.systemFont(ofSize: 11),
        ])
        field.stringValue = text
        field.onFocus = { [coordinator = context.coordinator] in coordinator.focused() }
        field.onBlur = { [coordinator = context.coordinator] field in coordinator.blurred(field) }
        return field
    }

    func updateNSView(_ field: KeyboardField, context: Context) {
        context.coordinator.parent = self
        // The field owns its text while it is being edited; a redraw must not move the caret or break a composition.
        if field.currentEditor() == nil, field.stringValue != text { field.stringValue = text }
    }

    static func dismantleNSView(_ field: KeyboardField, coordinator: Coordinator) {
        if coordinator.typing { coordinator.parent.onTyping(false) }
        (field.window as? OverlayPanel)?.releaseKeyboard()
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: IslandTextField
        var typing = false

        init(_ parent: IslandTextField) { self.parent = parent }

        /// Typing starts the moment the field has the caret, not at the first key: the island has to stay open
        /// for someone who clicked in and is still thinking.
        func focused() {
            parent.onFocus()
            guard !typing else { return }
            typing = true
            parent.onTyping(true)
        }

        func blurred(_ field: NSTextField) {
            guard typing else { return }
            typing = false
            parent.onTyping(false)
            (field.window as? OverlayPanel)?.releaseKeyboard()
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.onChange(field.stringValue)
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            switch selector {
            case #selector(NSResponder.insertNewline(_:)):
                guard !textView.hasMarkedText() else { return false }
                parent.onChange(textView.string)
                parent.onSubmit()
                return true
            case #selector(NSResponder.cancelOperation(_:)):
                control.window?.makeFirstResponder(nil)
                return true
            default:
                return false
            }
        }
    }
}

/// The field that asks the island for the keyboard when it is clicked, and says when it gets the caret and loses it.
final class KeyboardField: NSTextField {
    var onFocus: () -> Void = {}
    var onBlur: (NSTextField) -> Void = { _ in }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        (window as? OverlayPanel)?.takeKeyboard()
        super.mouseDown(with: event)
    }

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became { onFocus() }
        return became
    }

    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        onBlur(self)
    }
}
