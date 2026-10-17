import AppKit
import AgentHUDCore

/// One agent on the Simple HUD: its mark, and how full its tightest quota window is.
struct SimpleMeterItem: Identifiable, Equatable {
    let vendor: String
    /// Share of the tightest window already used, 0…100; nil until one of the vendor's windows has a reading
    /// that shows a level.
    let usedPct: Double?
    /// That window's level, which colours the ring and the number.
    let level: StatusLevel?
    /// Any agent that shares this mark has a session running.
    var isWorking = false
    /// What the mark shows of its agents, as the director says (`MarkDirector`): plain work while any of them works,
    /// with no director.
    var state = BotState()
    /// The newest event of its agents, for the mark to play once if it hears of it in time.
    var cue: BotCue? = nil
    /// Whether this agent's card is the one being looked at.
    var attention = MarkAttention.none
    /// An agent billed by use has no window to be a share of: its balance stands where the percentage would.
    var balance: String? = nil
    var id: String { vendor }

    /// What the mark is told: its agents' state with its card's attention on top.
    var shown: BotState { attention.state(of: state) }

    /// What the number says: whole percent used, a balance, or a dash before the first reading.
    var label: String { usedPct.map { "\(Int($0.rounded()))%" } ?? balance ?? "–" }

    /// A balance as the panel is told it: whose it is, how it reads, and its level. No text before its first reading.
    struct Balance: Equatable {
        let vendor: String
        let text: String?
        let level: StatusLevel?
    }

    /// The balances of the agents billed by use, as the store reads them now: the first currency of each.
    @MainActor
    static func balances(_ store: UsageStore) -> [Balance] {
        store.enabledBilling.map { billing in
            Balance(vendor: billing.vendor,
                    text: billing.balances.first.map { MoneyFormat.amount($0.total, currency: $0.currency) },
                    level: store.view.level(of: billing))
        }
    }

    /// One item per mark, in row order. Windows that share artwork share an item — two Claude windows are
    /// one Claude, Codex and ChatGPT one OpenAI — and it reports the most consumed of them. Only a reading
    /// that shows a level counts, the rule the menu bar's figure follows. An agent with a balance and no window
    /// follows them, as balances follow windows in the menu: its mark, an empty ring and the balance.
    ///
    /// - working: the vendors with a session running, as a logo queue is told them.
    /// - directions: what the director says each mark shows, by mark key, which a dock's marks are told the same.
    @MainActor
    static func items(rows: [AgentRow], balances: [Balance] = [], working: [String] = [],
                      directions: [String: MarkDirector.Direction] = [:]) -> [SimpleMeterItem] {
        let busy = Set(working.map(AgentArtwork.markKey))
        var order: [String] = []
        var vendors: [String: String] = [:]
        var tightest: [String: (used: Double, level: StatusLevel)] = [:]
        for row in rows {
            let key = AgentArtwork.markKey(row.agent.vendor)
            if vendors[key] == nil {
                order.append(key)
                vendors[key] = row.agent.vendor
            }
            guard row.assessment.showsLevel, let used = row.usedPct, let level = row.level,
                  used > (tightest[key]?.used ?? -1) else { continue }
            tightest[key] = (used, level)
        }
        func state(_ key: String) -> BotState { directions[key]?.state ?? BotState(look: busy.contains(key) ? .plain : .idle) }
        var items = order.map {
            SimpleMeterItem(vendor: vendors[$0] ?? $0, usedPct: tightest[$0]?.used, level: tightest[$0]?.level,
                            isWorking: busy.contains($0), state: state($0), cue: directions[$0]?.cue)
        }
        for balance in balances {
            let key = AgentArtwork.markKey(balance.vendor)
            guard vendors[key] == nil else { continue }
            vendors[key] = balance.vendor
            items.append(SimpleMeterItem(vendor: balance.vendor, usedPct: nil, level: balance.level,
                                         isWorking: busy.contains(key), state: state(key), cue: directions[key]?.cue,
                                         balance: balance.text))
        }
        return items
    }
}

/// The Simple HUD resolved for one screen: what to draw and how big. Every length follows from `size`, so
/// the one slider scales the whole panel; the ring's stroke is the only length set on its own.
struct SimpleMeterConfig: Equatable {
    /// Clear space between the panel and the screen edge it is parked on.
    static let edgeMargin: CGFloat = 3
    /// The run an empty panel keeps, so a screen with no quota row still has something to grab.
    static let emptyRun: CGFloat = 32

    var items: [SimpleMeterItem]
    var edge: HUDEdge
    /// Diameter of one ring, in points; without rings, the height the marks and numbers are sized from.
    var size: CGFloat
    var ringWidth: CGFloat
    var showsRings: Bool
    var showsNumbers: Bool
    /// Between two agents, as a share of a ring (`ScreenPlacement.simpleGapScale`).
    var gapScale: CGFloat
    /// Marks turn into bots while their agents work, as a logo queue's do: Settings' `botLogos`, on screen and in an
    /// agent's picture of the resting HUD (`HUDPictureRenderer`). Left out, it is off, for a panel that is only measured
    /// or drawn still in a snapshot.
    var bots: Bool

    init(items: [SimpleMeterItem], placement: ScreenPlacement, bots: Bool = false) {
        self.items = items
        edge = placement.edge
        size = placement.simpleSize
        ringWidth = placement.ringWidth
        showsRings = placement.showsRings
        showsNumbers = placement.showsNumbers
        gapScale = placement.simpleGapScale
        self.bots = bots
    }

    /// Side of one mark: inside its ring, or a little larger when it stands alone.
    var logo: CGFloat { (size * (showsRings ? 0.55 : 0.68)).rounded() }
    /// The square a mark takes, ring included.
    var mark: CGFloat { showsRings ? size : logo }
    /// The numbers are written as everything the dock grows out of itself is, a reminder's words and the hover line:
    /// `IslandRootView.dockText` at the panel's scale, so the three read at one size beside any ring.
    var fontSize: CGFloat { IslandRootView.dockText(scale: size / IslandRootView.dockCompactHeight) }
    /// Between a mark and its number.
    var labelSpacing: CGFloat { (size * 0.23).rounded() }
    /// Between two agents along the panel.
    var itemGap: CGFloat { (size * gapScale * (showsNumbers || !edge.isHorizontal ? 1 : 0.7)).rounded() }

    private var font: NSFont { .monospacedDigitSystemFont(ofSize: fontSize, weight: .regular) }

    /// Width of one number as drawn, whole points up.
    func labelWidth(_ item: SimpleMeterItem) -> CGFloat {
        ceil((item.label as NSString).size(withAttributes: [.font: font]).width)
    }

    /// On a side edge the numbers share one column, as wide as the widest, so they line up on the right.
    var labelColumn: CGFloat { items.map(labelWidth).max() ?? 0 }

    /// Padding inside the panel. A ring sits close to the panel's own curve; a number needs more room
    /// before the panel's end than a ring does.
    var insets: NSEdgeInsets {
        let tight = max(3, (size * 0.18).rounded())
        let loose = (size * 0.45).rounded()
        let leading = showsRings ? tight + 1 : loose
        let trailing = showsNumbers ? loose : leading
        return edge.isHorizontal
            ? NSEdgeInsets(top: tight, left: leading, bottom: tight, right: trailing)
            : NSEdgeInsets(top: tight + 3, left: leading, bottom: tight + 3, right: trailing)
    }

    private func cellWidth(label: CGFloat) -> CGFloat {
        mark + (showsNumbers ? labelSpacing + label : 0)
    }

    /// Each mark's centre along the panel, measured from its start: left on a horizontal edge, top on a side.
    var markCentres: [(vendor: String, along: CGFloat)] {
        var along = edge.isHorizontal ? insets.left : insets.top
        return items.map { item in
            let centre = along + mark / 2
            along += (edge.isHorizontal ? cellWidth(label: labelWidth(item)) : mark) + itemGap
            return (item.vendor, centre)
        }
    }

    /// The panel itself.
    var panelSize: CGSize {
        let insets = insets
        let count = CGFloat(items.count)
        guard !items.isEmpty else {
            let thick = mark + (edge.isHorizontal ? insets.top + insets.bottom : insets.left + insets.right)
            return edge.isHorizontal ? CGSize(width: Self.emptyRun, height: thick) : CGSize(width: thick, height: Self.emptyRun)
        }
        if edge.isHorizontal {
            let cells = items.reduce(0) { $0 + cellWidth(label: labelWidth($1)) }
            return CGSize(width: insets.left + cells + itemGap * (count - 1) + insets.right,
                          height: insets.top + mark + insets.bottom)
        }
        return CGSize(width: insets.left + cellWidth(label: labelColumn) + insets.right,
                      height: insets.top + mark * count + itemGap * (count - 1) + insets.bottom)
    }

    /// What the screen reserves for the panel: its own size and the margin that keeps it off the edge.
    var stripSize: CGSize {
        let panel = panelSize
        return edge.isHorizontal
            ? CGSize(width: panel.width, height: panel.height + Self.edgeMargin)
            : CGSize(width: panel.width + Self.edgeMargin, height: panel.height)
    }

    /// Fully round while the panel is a single line of rings; a wider column keeps a softer corner.
    var cornerRadius: CGFloat {
        let panel = panelSize
        return min((size * 0.73).rounded(), min(panel.width, panel.height) / 2)
    }
}

extension SimpleMeterItem {
    /// Stand-ins for a preview on a Mac with no quota row to show.
    static let samples = [
        SimpleMeterItem(vendor: "Claude", usedPct: 62, level: .ok),
        SimpleMeterItem(vendor: "Codex", usedPct: 18, level: .ok),
    ]
}
