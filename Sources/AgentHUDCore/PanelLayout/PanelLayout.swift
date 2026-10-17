import AgentHUDSupport
import Foundation

/// What draws a block of the hover panel: rows of bars, a ring, a number or a chart.
public enum PanelWidget: String, Codable, CaseIterable, Sendable {
    case bar, ring, number, chart

    /// Whether the widget draws a cell per window, side by side in lines: a ring or a number.
    var drawsCells: Bool { self == .ring || self == .number }
}

/// What a block shows, the reading its picker flips in place: a quota window's share, or an API account's balance
/// (`quota`), how fast that goes (`burnRate`, written `burn`), tokens, or cost.
public enum PanelMeasure: String, Codable, CaseIterable, Sendable, Identifiable {
    case quota, burnRate = "burn", tokens, cost

    public var id: Self { self }
}

/// How large the ring widget draws its rings; `medium` is the ring's default size.
public enum PanelRingSize: String, Codable, CaseIterable, Sendable { case small, medium, large }

/// How much of a line of the hover panel a block takes: the whole line, as by default, or half of it, beside the block
/// next to it in the panel's order where that one takes half too (`PanelLayoutPlan`).
public enum PanelSpan: String, Codable, CaseIterable, Sendable { case full, half }

/// How far apart the hover panel sets its sections, blocks, rows and cells; `regular` is the default view's spacing.
public enum PanelSpacing: String, Codable, CaseIterable, Sendable { case compact, regular, roomy }

/// A part of the hover panel, in the panel's default order: the agents' blocks, the token chart, the sessions.
public enum PanelSection: String, Codable, CaseIterable, Sendable { case blocks, tokens, sessions }

/// How the whole hover panel is set, whatever it shows. `PanelStyle()` is the default view's style and is not written.
public struct PanelStyle: Hashable, Codable, Sendable {
    /// The widths a layout can give the whole panel, in points.
    public static let widthRange = 480...720
    /// The whole panel's width where a layout gives none, the panel's width before layouts had one.
    public static let defaultWidth = 540
    /// The narrowest an agent's own panel on a dock that separates its agents is, in points.
    static let narrowestAgentWidth = 440
    /// How much narrower an agent's own panel is than the whole panel.
    static let agentWidthInset = 100

    /// The gaps between the panel's sections, its blocks, a block's heading and its rows, and its rows and cells.
    public var spacing: PanelSpacing
    /// Whether a divider line stands between two blocks and between two sections. Without one, its room stays.
    public var separators: Bool
    /// The whole panel's width in points, within `widthRange`; nil for `defaultWidth`.
    public var width: Int?

    public init(spacing: PanelSpacing = .regular, separators: Bool = true, width: Int? = nil) {
        self.spacing = spacing
        self.separators = separators
        self.width = width
    }

    /// The whole panel's width in effect: the style's, else 540. The type scale is the same at every width.
    public var panelWidth: Int { width.flatMap { Self.widthRange.contains($0) ? $0 : nil } ?? Self.defaultWidth }

    /// The width of one agent's own panel on a dock that separates its agents: 100 points narrower than the whole
    /// panel, and never below 440, which it is at 540 and below.
    public var agentPanelWidth: Int { max(Self.narrowestAgentWidth, panelWidth - Self.agentWidthInset) }

    /// Whether the style is the default view's: regular spacing with separators, at the default width.
    var isDefault: Bool { normalized() == PanelStyle() }

    /// The style written one way: a width outside `widthRange`, or the default width, is no width.
    func normalized() -> PanelStyle {
        PanelStyle(spacing: spacing, separators: separators, width: panelWidth == Self.defaultWidth ? nil : panelWidth)
    }

    private enum CodingKeys: String, CodingKey { case spacing, separators, width }

    /// Nil when the value is not an object. A spacing this build does not know is regular, and a width that is no whole
    /// number from 480 to 720 is not given.
    init?(json: JSONValue) {
        guard let fields = json.objectValue else { return nil }
        self.init(spacing: fields["spacing"]?.stringValue.flatMap(PanelSpacing.init(rawValue:)) ?? .regular,
                  separators: fields["separators"]?.boolValue ?? true,
                  width: fields["width"]?.countValue.flatMap { Self.widthRange.contains($0) ? $0 : nil })
    }

    public init(from decoder: Decoder) throws {
        guard let style = PanelStyle(json: try decoder.singleValueContainer().decode(JSONValue.self)) else {
            throw DecodingError.typeMismatch(PanelStyle.self, DecodingError.Context(
                codingPath: decoder.codingPath, debugDescription: "The style of a panel layout is a JSON object."))
        }
        self = style
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if spacing != .regular { try c.encode(spacing, forKey: .spacing) }
        if !separators { try c.encode(false, forKey: .separators) }
        if panelWidth != Self.defaultWidth { try c.encode(panelWidth, forKey: .width) }
    }
}

/// How a screen's hover panel differs from its default view, and which agents that screen's resting HUD shows. The
/// default view is a built-in rule, so a layout records only what was changed: `PanelLayout()` is the default view, every
/// agent on the resting HUD, and its JSON is `{}`. A Mac keeps a shared layout and, for any screen, one of its own
/// (`PanelLayoutStore`).
///
/// A block is one agent's quota windows or one API account's balance, under the key `PanelBlockInfo` gives it; a row of
/// a block is a quota window, under its `AgentDescriptor.windowKey`, and an account of a block is one of its sections,
/// under the id `PanelBlockInfo.Account` gives it. A key or an id that names nothing the panel has now stays in the
/// document and applies again when its block, row or account returns. The four switches of Settings → Display →
/// Expanded panel are fields of every layout (`PanelSwitches`): its hidden sections and its `countdown`.
///
/// The JSON is also what an agent sends, so reading forgives field by field: a widget, ring size, span, period, sort or
/// spacing name this build does not know leaves that field unset, a session period the build before named is the period
/// that covers it, a measure or section it does not know is left out of its list,
/// columns outside 2 to 6, a width outside 480 to 720 and a session limit outside 1 to 10 count as not given, a block,
/// a style, the sessions, the tokens or a hud that is not an object is left out, a value of the wrong type counts as not
/// given, and keys it does not know are ignored, the style
/// fields an agent sends in `hud` among them (`HUDStyle`), which are a screen's settings and no part of a layout. Only
/// JSON that is not an object fails.
/// Writing leaves out what is unset, empty or the default, so with sorted keys (`RecordCoding.encoder()`) equal layouts
/// are the same bytes.
public struct PanelLayout: Hashable, Codable, Sendable {
    /// What a layout changes about one block.
    public struct Block: Hashable, Codable, Sendable {
        /// The widget that draws the block; nil for its kind's default.
        public var widget: PanelWidget?
        /// The measures the block's picker offers, in that order, the first one shown when the panel opens; empty for
        /// every measure its widget offers. A block given one measure has no picker.
        public var measures: [PanelMeasure]
        /// Whether the block is left out of the panel.
        public var hidden: Bool
        /// The rows left out of the block, by row key.
        public var hiddenRows: [String]
        /// The order of the block's rows, by row key: those named come first, in that order, and the others keep their
        /// order after them. Empty for the rows' own order.
        public var rows: [String]
        /// The accounts whose rows or balances the block shows, by the id `get_hud` gives each; empty for every account.
        /// Rows that belong to no account, which no id names, are left out by any.
        public var accounts: [String]
        /// Whether a quota block shows its reset times; nil for the layout's `countdown`, which this overrides.
        public var countdown: Bool?
        /// How many cells of a ring or number widget stand on a line, within `columnRange`; nil for four.
        public var columns: Int?
        /// The size of a ring widget's rings; nil for medium.
        public var ringSize: PanelRingSize?
        /// How much of its line the block takes; nil for the whole line.
        public var span: PanelSpan?
        /// The period a quota block's chart draws, or the span an API balance's estimated cost covers; nil for the latest
        /// 24 hours on a chart, and on a balance for the period the layout's token chart starts from.
        public var period: PanelTokenPeriod?

        /// The columns a layout can give a block of cells.
        static let columnRange = 2...6
        /// The columns of a block of cells that a layout gives none.
        static let defaultColumns = 4

        public init(widget: PanelWidget? = nil, measures: [PanelMeasure] = [], hidden: Bool = false, hiddenRows: [String] = [],
                    rows: [String] = [], accounts: [String] = [], countdown: Bool? = nil, columns: Int? = nil,
                    ringSize: PanelRingSize? = nil, span: PanelSpan? = nil, period: PanelTokenPeriod? = nil) {
            self.widget = widget
            self.measures = measures
            self.hidden = hidden
            self.hiddenRows = hiddenRows
            self.rows = rows
            self.accounts = accounts
            self.countdown = countdown
            self.columns = columns
            self.ringSize = ringSize
            self.span = span
            self.period = period
        }

        /// Whether the block changes nothing: every field unset, empty or as the default view has it.
        var isEmpty: Bool {
            widget == nil && measures.isEmpty && !hidden && hiddenRows.isEmpty && rows.isEmpty && accounts.isEmpty
                && countdown == nil && columns == nil && ringSize == nil && span == nil && period == nil
        }

        /// The block with each measure named once, in the order first named; each hidden row named once, in key order,
        /// and no empty key among them, since which rows are hidden matters, not the order they were named in; each row
        /// of its order named once, the first time, in the order named; each account once, in id order, since a filter
        /// has no order, and no empty key or id among them; and no columns outside `columnRange`.
        func normalized() -> Block {
            Block(widget: widget, measures: measures.withoutRepeats, hidden: hidden,
                  hiddenRows: hiddenRows.filter { !$0.isEmpty }.withoutRepeats.sorted(),
                  rows: rows.filter { !$0.isEmpty }.withoutRepeats, accounts: accounts.filter { !$0.isEmpty }.withoutRepeats.sorted(),
                  countdown: countdown, columns: columns.flatMap { Self.columnRange.contains($0) ? $0 : nil }, ringSize: ringSize,
                  span: span, period: period)
        }

        private enum CodingKeys: String, CodingKey {
            case widget, measures, hidden, hiddenRows, rows, accounts, countdown, columns, ringSize, span, period
        }

        /// Nil when the value is not an object.
        init?(json: JSONValue) {
            guard let fields = json.objectValue else { return nil }
            self.init(widget: fields["widget"]?.stringValue.flatMap(PanelWidget.init(rawValue:)),
                      measures: PanelLayout.strings(fields["measures"]).compactMap(PanelMeasure.init(rawValue:)),
                      hidden: fields["hidden"]?.boolValue ?? false,
                      hiddenRows: PanelLayout.strings(fields["hiddenRows"]),
                      rows: PanelLayout.strings(fields["rows"]),
                      accounts: PanelLayout.strings(fields["accounts"]),
                      countdown: fields["countdown"]?.boolValue,
                      columns: fields["columns"]?.countValue.flatMap { Self.columnRange.contains($0) ? $0 : nil },
                      ringSize: fields["ringSize"]?.stringValue.flatMap(PanelRingSize.init(rawValue:)),
                      span: fields["span"]?.stringValue.flatMap(PanelSpan.init(rawValue:)),
                      period: fields["period"]?.stringValue.flatMap(PanelTokenPeriod.init(rawValue:)))
        }

        public init(from decoder: Decoder) throws {
            guard let block = Block(json: try decoder.singleValueContainer().decode(JSONValue.self)) else {
                throw DecodingError.typeMismatch(Block.self, DecodingError.Context(
                    codingPath: decoder.codingPath, debugDescription: "A block of a panel layout is a JSON object."))
            }
            self = block
        }

        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encodeIfPresent(widget, forKey: .widget)
            if !measures.isEmpty { try c.encode(measures, forKey: .measures) }
            if hidden { try c.encode(true, forKey: .hidden) }
            if !hiddenRows.isEmpty { try c.encode(hiddenRows, forKey: .hiddenRows) }
            if !rows.isEmpty { try c.encode(rows, forKey: .rows) }
            if !accounts.isEmpty { try c.encode(accounts, forKey: .accounts) }
            try c.encodeIfPresent(countdown, forKey: .countdown)
            try c.encodeIfPresent(columns, forKey: .columns)
            try c.encodeIfPresent(ringSize, forKey: .ringSize)
            try c.encodeIfPresent(span, forKey: .span)
            try c.encodeIfPresent(period, forKey: .period)
        }
    }

    /// What differs about each block, by block key.
    public var blocks: [String: Block]
    /// Block keys in the order their blocks come; blocks not named follow in default order.
    public var order: [String]
    /// Sections in the order they come; sections not named follow in default order.
    public var sections: [PanelSection]
    /// The sections left out of the panel.
    public var hiddenSections: [PanelSection]
    /// Whether the quota blocks show their reset times, where a block's own `Block.countdown` does not say otherwise.
    public var countdown: Bool
    /// How the whole panel is set.
    public var style: PanelStyle
    /// Which agents the resting HUD of a screen that draws this layout shows, and in what order.
    public var hud: HUD
    /// Which sessions the session rows list, and how many.
    public var sessions: Sessions
    /// The token chart's period and whose tokens it draws.
    public var tokens: Tokens

    public init(blocks: [String: Block] = [:], order: [String] = [], sections: [PanelSection] = [],
                hiddenSections: [PanelSection] = [], countdown: Bool = true, style: PanelStyle = PanelStyle(), hud: HUD = HUD(),
                sessions: Sessions = Sessions(), tokens: Tokens = Tokens()) {
        self.blocks = blocks
        self.order = order
        self.sections = sections
        self.hiddenSections = hiddenSections
        self.countdown = countdown
        self.style = style
        self.hud = hud
        self.sessions = sessions
        self.tokens = tokens
    }

    // MARK: The same layout

    /// The layout written one way, so that two layouts are the same layout when their normalized values are equal: no
    /// block that changes nothing; each key in `order` once, the first time it is named, and none empty; each block
    /// normalized (`Block.normalized()`); `sections` as the whole order it gives, or empty when that is the default
    /// order; and `hiddenSections` once each, in default order. A hidden section keeps its place in `sections`, which
    /// it takes again when it is shown. The style's width is left out at 540 (`PanelStyle.normalized()`), and the HUD's
    /// agents, the sessions and the tokens are normalized too (`HUD.normalized()`, `Sessions.normalized()`,
    /// `Tokens.normalized()`).
    func normalized() -> PanelLayout {
        var kept: [String: Block] = [:]
        for (key, block) in blocks {
            let block = block.normalized()
            if !block.isEmpty { kept[key] = block }
        }
        let sectionOrder = (sections + PanelSection.allCases).withoutRepeats
        return PanelLayout(blocks: kept, order: order.filter { !$0.isEmpty }.withoutRepeats,
                           sections: sectionOrder == PanelSection.allCases ? [] : sectionOrder,
                           hiddenSections: PanelSection.allCases.filter(hiddenSections.contains), countdown: countdown,
                           style: style.normalized(),
                           hud: hud.normalized(), sessions: sessions.normalized(), tokens: tokens.normalized())
    }

    /// Whether the layout is the default view: nothing is left of it once normalized.
    public var isDefault: Bool { normalized() == PanelLayout() }

    // MARK: Coding

    private enum CodingKeys: String, CodingKey {
        case blocks, order, sections, hiddenSections, countdown, style, hud, sessions, tokens
    }

    /// Nil when the value is not an object.
    init?(json: JSONValue) {
        guard let fields = json.objectValue else { return nil }
        var blocks: [String: Block] = [:]
        for (key, value) in fields["blocks"]?.objectValue ?? [:] {
            if let block = Block(json: value) { blocks[key] = block }
        }
        self.init(blocks: blocks, order: Self.strings(fields["order"]),
                  sections: Self.strings(fields["sections"]).compactMap(PanelSection.init(rawValue:)),
                  hiddenSections: Self.strings(fields["hiddenSections"]).compactMap(PanelSection.init(rawValue:)),
                  countdown: fields["countdown"]?.boolValue ?? true,
                  style: fields["style"].flatMap(PanelStyle.init(json:)) ?? PanelStyle(),
                  hud: fields["hud"].flatMap(HUD.init(json:)) ?? HUD(),
                  sessions: fields["sessions"].flatMap(Sessions.init(json:)) ?? Sessions(),
                  tokens: fields["tokens"].flatMap(Tokens.init(json:)) ?? Tokens())
    }

    public init(from decoder: Decoder) throws {
        guard let layout = PanelLayout(json: try decoder.singleValueContainer().decode(JSONValue.self)) else {
            throw DecodingError.typeMismatch(PanelLayout.self, DecodingError.Context(
                codingPath: decoder.codingPath, debugDescription: "A panel layout is a JSON object."))
        }
        self = layout
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if !blocks.isEmpty { try c.encode(blocks, forKey: .blocks) }
        if !order.isEmpty { try c.encode(order, forKey: .order) }
        if !sections.isEmpty { try c.encode(sections, forKey: .sections) }
        if !hiddenSections.isEmpty { try c.encode(hiddenSections, forKey: .hiddenSections) }
        if !countdown { try c.encode(false, forKey: .countdown) }
        if !style.isDefault { try c.encode(style, forKey: .style) }
        if !hud.isDefault { try c.encode(hud, forKey: .hud) }
        if !sessions.isDefault { try c.encode(sessions, forKey: .sessions) }
        if !tokens.isDefault { try c.encode(tokens, forKey: .tokens) }
    }

    /// The strings of a list, in its order; what is not a string is left out, and what is not a list gives none.
    static func strings(_ value: JSONValue?) -> [String] {
        (value?.arrayValue ?? []).compactMap(\.stringValue)
    }
}

extension Array where Element: Hashable {
    /// The elements in order, each kept the first time it comes.
    var withoutRepeats: [Element] {
        var seen = Set<Element>()
        return filter { seen.insert($0).inserted }
    }
}
