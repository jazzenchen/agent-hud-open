import AgentHUDSupport
import Foundation

/// A change an agent proposed to the app's own settings, those that hold for the whole app rather than one screen: the
/// Appearance and Match screen in Settings → General, and Bot logos, Show menu bar icon and the four settings of Hover
/// behavior in Settings → Display, each nil where it leaves Settings as they are. Settings stay the saved truth. It waits
/// as the app's draft, held in memory beside the screens' drafts (`PanelLayoutStore.appDraft`) and drawn over the
/// settings wherever the app reads them (`applied(to:)`, `SettingsStore.effectiveSettings`), until a person keeps it with
/// the Save in a panel's draft bar or drops it with its Discard, and a person's own change of one of them in Settings
/// drops that field first. A field is set only when the person asks an agent for that setting, never as a side effect of
/// arranging the panel, which the tools' descriptions tell an agent.
///
/// It is written in an agent's words, as `get_hud` reads it under `app` and `draft_hud` takes it: `matchScreen`
/// is `appearanceFollowsScreen`, and under `hover`, `pressOptionToOpen` is `requiresOptionToOpen`, `openAtTopEdge`
/// `openImmediatelyAtTop` and `delayMs` `hoverDelayMs`, while `appearance`, `botLogos`, `showMenuBarIcon` and
/// `collapseDelayMs` keep their names. The language is none of them: it is changed in Settings alone.
public struct AppDraft: Hashable, Sendable {
    /// The app's windows' and the menu-bar menu's appearance, unless they match their screen.
    public var appearance: AppearanceMode?
    /// Whether each window and the menu-bar menu take the look of the HUD on the screen they are on.
    public var matchScreen: Bool?
    /// Whether marks turn into bots on a Dynamic dock, a Simple panel and reminders.
    public var botLogos: Bool?
    public var showMenuBarIcon: Bool?
    /// Whether hovering opens the panel only with Option pressed.
    public var pressOptionToOpen: Bool?
    /// Whether reaching the screen's top edge above the HUD opens the panel without the hover delay.
    public var openAtTopEdge: Bool?
    /// The hover delay and the collapse delay, in milliseconds.
    public var delayMs: Int?
    public var collapseDelayMs: Int?

    public init(appearance: AppearanceMode? = nil, matchScreen: Bool? = nil, botLogos: Bool? = nil,
                showMenuBarIcon: Bool? = nil, pressOptionToOpen: Bool? = nil, openAtTopEdge: Bool? = nil,
                delayMs: Int? = nil, collapseDelayMs: Int? = nil) {
        self.appearance = appearance
        self.matchScreen = matchScreen
        self.botLogos = botLogos
        self.showMenuBarIcon = showMenuBarIcon
        self.pressOptionToOpen = pressOptionToOpen
        self.openAtTopEdge = openAtTopEdge
        self.delayMs = delayMs
        self.collapseDelayMs = collapseDelayMs
    }

    /// Whether the draft changes nothing.
    var isEmpty: Bool { self == AppDraft() }

    /// `settings` with the fields this draft sets in place of their own.
    public func applied(to settings: Settings) -> Settings {
        settings.with { next in
            if let appearance { next.appearance = appearance }
            if let matchScreen { next.appearanceFollowsScreen = matchScreen }
            if let botLogos { next.botLogos = botLogos }
            if let showMenuBarIcon { next.showMenuBarIcon = showMenuBarIcon }
            if let pressOptionToOpen { next.requiresOptionToOpen = pressOptionToOpen }
            if let openAtTopEdge { next.openImmediatelyAtTop = openAtTopEdge }
            if let delayMs { next.hoverDelayMs = delayMs }
            if let collapseDelayMs { next.collapseDelayMs = collapseDelayMs }
        }
    }

    /// Every field an agent can set, as `settings` has it: the app's settings in effect, as `get_hud` lists them.
    init(every settings: Settings) {
        self.init(appearance: settings.appearance, matchScreen: settings.appearanceFollowsScreen,
                  botLogos: settings.botLogos, showMenuBarIcon: settings.showMenuBarIcon,
                  pressOptionToOpen: settings.requiresOptionToOpen, openAtTopEdge: settings.openImmediatelyAtTop,
                  delayMs: settings.hoverDelayMs, collapseDelayMs: settings.collapseDelayMs)
    }

    /// The draft reduced to what it changes about the app saved with `settings`: a field set to the value saved is left
    /// out, so a draft that spells out what is saved is empty.
    func reduced(against settings: Settings) -> AppDraft {
        let saved = AppDraft(every: settings)
        var kept = self
        for field in Field.allCases where kept.value(field) == saved.value(field) { kept.set(field, .null) }
        return kept
    }

    /// The draft without `field`, which then leaves that setting as saved.
    func leaving(_ field: Field) -> AppDraft {
        var next = self
        next.set(field, .null)
        return next
    }
}

// MARK: Fields

extension AppDraft {
    /// A setting of the app an agent can set, by the name it uses for it.
    public enum Field: String, CaseIterable, Sendable {
        case appearance, matchScreen, botLogos, showMenuBarIcon, pressOptionToOpen, openAtTopEdge, delayMs, collapseDelayMs

        /// Whether it is one of Hover behavior's, which `app` holds under `hover`.
        var isHover: Bool {
            switch self {
            case .pressOptionToOpen, .openAtTopEdge, .delayMs, .collapseDelayMs: true
            case .appearance, .matchScreen, .botLogos, .showMenuBarIcon: false
            }
        }

        /// Where `draft_hud` takes it and `get_hud` lists it, as a sentence names it: `app.botLogos`,
        /// `app.hover.delayMs`.
        var path: String { "app." + (isHover ? "hover." : "") + rawValue }

        /// Whether it is a delay, a whole number of milliseconds Settings sets in `Settings.hoverDelayRange`, on the step
        /// its slider moves by, `Settings.hoverDelayStep`.
        var isDelay: Bool { self == .delayMs || self == .collapseDelayMs }

        /// The setting it sets.
        var setting: PartialKeyPath<Settings> {
            switch self {
            case .appearance: return \Settings.appearance
            case .matchScreen: return \Settings.appearanceFollowsScreen
            case .botLogos: return \Settings.botLogos
            case .showMenuBarIcon: return \Settings.showMenuBarIcon
            case .pressOptionToOpen: return \Settings.requiresOptionToOpen
            case .openAtTopEdge: return \Settings.openImmediatelyAtTop
            case .delayMs: return \Settings.hoverDelayMs
            case .collapseDelayMs: return \Settings.collapseDelayMs
            }
        }

        /// The field that sets `setting`; nil for a setting no field sets.
        public init?(setting: PartialKeyPath<Settings>) {
            guard let field = Self.allCases.first(where: { $0.setting == setting }) else { return nil }
            self = field
        }
    }

    /// What `get_hud` lists under `app` beside the fields, which no proposal sets: the language chosen and the one the
    /// panel's names are written in. A proposal may give each as it was listed, so the app listed can be sent back whole.
    static let readOnly = ["language", "writtenIn"]

    /// A field's value as an agent writes it; nil while the draft leaves it.
    func value(_ field: Field) -> JSONValue? {
        switch field {
        case .appearance: appearance.map { .string($0.rawValue) }
        case .matchScreen: matchScreen.map(JSONValue.bool)
        case .botLogos: botLogos.map(JSONValue.bool)
        case .showMenuBarIcon: showMenuBarIcon.map(JSONValue.bool)
        case .pressOptionToOpen: pressOptionToOpen.map(JSONValue.bool)
        case .openAtTopEdge: openAtTopEdge.map(JSONValue.bool)
        case .delayMs: delayMs.map { .integer(Int64($0)) }
        case .collapseDelayMs: collapseDelayMs.map { .integer(Int64($0)) }
        }
    }

    /// Sets a field from what an agent wrote: a value of the field's kind, or nothing for any other. A delay outside
    /// Settings' range or between its steps is read, and refused as the app's issue (`issues(against:)`), so that one
    /// repeating what is saved, which the settings read as written, changes nothing.
    mutating func set(_ field: Field, _ json: JSONValue) {
        switch field {
        case .appearance: appearance = json.stringValue.flatMap(AppearanceMode.init(rawValue:))
        case .matchScreen: matchScreen = json.boolValue
        case .botLogos: botLogos = json.boolValue
        case .showMenuBarIcon: showMenuBarIcon = json.boolValue
        case .pressOptionToOpen: pressOptionToOpen = json.boolValue
        case .openAtTopEdge: openAtTopEdge = json.boolValue
        case .delayMs: delayMs = Self.whole(json)
        case .collapseDelayMs: collapseDelayMs = Self.whole(json)
        }
    }

    /// The draft as an agent writes it: the fields it sets, Hover behavior's under `hover`, and none it leaves.
    var json: [String: JSONValue] {
        var fields: [String: JSONValue] = [:], hover: [String: JSONValue] = [:]
        for field in Field.allCases {
            guard let value = value(field) else { continue }
            if field.isHover { hover[field.rawValue] = value } else { fields[field.rawValue] = value }
        }
        if !hover.isEmpty { fields["hover"] = .object(hover) }
        return fields
    }

    /// The fields of an app as an agent wrote them, each one a value of its kind; the read-only language and keys that
    /// name no field are passed over. A proposal is checked first (`misreadings(of:)`), so nothing it asks for is lost
    /// here.
    init(json fields: [String: JSONValue]) {
        self.init()
        for field in Field.allCases {
            let value = field.isHover ? fields["hover"]?.objectValue?[field.rawValue] : fields[field.rawValue]
            if let value { set(field, value) }
        }
    }

    /// A number written whole, of any sign; nil for anything else.
    private static func whole(_ json: JSONValue) -> Int? {
        switch json {
        case .integer(let value): Int(exactly: value)
        case .number(let value) where value.isFinite && value.rounded() == value: Int(exactly: value)
        default: nil
        }
    }
}

// MARK: Rules

extension AppDraft {
    /// What reading an app as an agent sent it would forgive, as sentences: a value of the wrong kind, an appearance this
    /// build does not know, or a property the app or its hover does not have. Whether the read-only language repeats
    /// what is listed is the proposal's to tell (`HUDTools.AppProposal`). A value given as null is not given.
    static func misreadings(of value: JSONValue) -> [String] {
        if value == .null { return [] }
        guard let app = value.objectValue else { return ["app is an object."] }
        let named = Field.allCases.filter { !$0.isHover }.map(\.rawValue) + ["hover"]
        var found: [String] = []
        for property in app.keys.sorted() {
            let value = app[property] ?? .null
            if property == "hover" {
                found += hoverMisreadings(value)
                continue
            }
            if readOnly.contains(property) { continue }
            guard let field = Field(rawValue: property), !field.isHover else {
                var sentence = "app has no property \"\(property)\"; the app takes \(HUDStyle.listed(named)), and "
                    + "\(HUDStyle.listed(readOnly)) as get_hud lists them."
                if let field = Field(rawValue: property) { sentence += " Hover behavior's \(property) is \(field.path)." }
                found.append(sentence)
                continue
            }
            found += misreadings(of: field, value)
        }
        return found
    }

    private static func hoverMisreadings(_ value: JSONValue) -> [String] {
        if value == .null { return [] }
        guard let hover = value.objectValue else { return ["app.hover is an object."] }
        let named = Field.allCases.filter(\.isHover).map(\.rawValue)
        var found: [String] = []
        for property in hover.keys.sorted() {
            guard let field = Field(rawValue: property), field.isHover else {
                found.append("app.hover has no property \"\(property)\"; the hover takes \(HUDStyle.listed(named)).")
                continue
            }
            found += misreadings(of: field, hover[property] ?? .null)
        }
        return found
    }

    private static func misreadings(of field: Field, _ value: JSONValue) -> [String] {
        if value == .null { return [] }
        switch field {
        case .appearance:
            guard AppearanceMode(rawValue: value.stringValue ?? "") == nil else { return [] }
            return ["\(field.path) is one of \(PanelLayout.quoted(AppearanceMode.allCases.map(\.rawValue)))."]
        case .matchScreen, .botLogos, .showMenuBarIcon, .pressOptionToOpen, .openAtTopEdge:
            return value.boolValue == nil ? ["\(field.path) is true or false."] : []
        case .delayMs, .collapseDelayMs:
            return whole(value) == nil ? ["\(field.path) is a whole number of milliseconds."] : []
        }
    }

    /// Every field the draft sets that the app would not draw as asked, by path, worded for the agent: a delay outside
    /// the milliseconds Settings sets it in or between the steps its slider moves by, and an appearance while the app's
    /// windows and menu would match their screen, which Settings greys the Appearance out for. It is asked of a draft
    /// reduced to what it changes (`reduced(against:)`), so a field that only repeats what is saved is never one, a
    /// delay saved outside the range included.
    /// - settings: what the app has saved, which the draft is drawn over.
    func issues(against settings: Settings) -> [PanelLayoutIssue] {
        var issues: [PanelLayoutIssue] = []
        let range = Settings.hoverDelayRange, step = Settings.hoverDelayStep
        for field in Field.allCases where field.isDelay {
            guard let value = field == .delayMs ? delayMs : collapseDelayMs,
                  !range.contains(value) || (value - range.lowerBound) % step != 0 else { continue }
            issues.append(PanelLayoutIssue(path: field.path, message: "\(field.rawValue) is a whole number of milliseconds "
                                           + "from \(range.lowerBound) to \(range.upperBound) in steps of \(step), as "
                                           + "Settings sets it, which \(value) is not."))
        }
        if appearance != nil, matchScreen ?? settings.appearanceFollowsScreen {
            issues.append(PanelLayoutIssue(path: Field.appearance.path, message: "appearance sets the app's windows and the "
                                           + "menu-bar menu, and with matchScreen on, as it would be, each takes the look of "
                                           + "the HUD on its screen instead, which is why Settings greys the Appearance "
                                           + "out. Leave it out, or set matchScreen to false."))
        }
        return issues
    }
}
