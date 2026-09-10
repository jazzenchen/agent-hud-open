import AppKit
import AgentHUDCore

struct MenuActions {
    var toggleGlow: () -> Void = {}
    var openSettings: () -> Void = {}
    var openStats: () -> Void = {}
    var additional: [DesktopMenuAction] = []
    var quit: () -> Void = {}
}

/// Menu bar item (notch silhouette + highest % used) and its menu.
@MainActor
final class StatusItemController: NSObject, NSMenuDelegate {
    private let item: NSStatusItem
    private let menu = NSMenu()
    private let store: UsageStore
    private let settings: SettingsStore
    var actions = MenuActions()

    private static let menuWidth: CGFloat = 250
    private static let agentMenuFont = NSFontManager.shared.convert(.menuFont(ofSize: 13), toHaveTrait: .boldFontMask)

    init(store: UsageStore, settings: SettingsStore) {
        self.store = store
        self.settings = settings
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        menu.delegate = self
        menu.autoenablesItems = false
        menu.minimumWidth = Self.menuWidth
        item.menu = menu
        item.button?.imagePosition = .imageLeading
        refreshButton()
        observeChanges({ [weak self] in
            guard let self else { return }
            _ = self.store.rows
            _ = self.store.pausedUntil
            _ = self.store.isAccessAllowed
            _ = self.settings.settings.showMenuBarIcon
            _ = self.settings.settings.language
        }, onChange: { [weak self] in
            self?.refreshButton()
        })
    }

    // MARK: Button

    func refreshButton() {
        item.isVisible = !store.isAccessAllowed || settings.settings.showMenuBarIcon
        guard let button = item.button else { return }
        let stops = store.isPaused || !store.isAccessAllowed ? GlowGradient.idleStops : GlowGradient.stops(levels: store.levels, light: SystemAppearance.isLight)
        button.image = StatusIconRenderer.image(stops: stops)
        let title = store.isAccessAllowed ? (store.maxUsedPct.map { " \(TokenFormat.percent($0))" } ?? " —") : " —"
        button.attributedTitle = NSAttributedString(string: title, attributes: [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .regular),
            .baselineOffset: 0,
        ])
        button.toolTip = AppResources.applicationName + L10n.text(" · 最高已用额度", " · highest usage")
    }

    // MARK: Menu

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let groups = store.rowGroups
        for (index, group) in groups.enumerated() {
            if index > 0 { menu.addItem(.separator()) }
            if groups.count > 1 || group.rows.count > 1 {
                let header = NSMenuItem(title: L10n.vendorLabel(group.vendor), action: nil, keyEquivalent: "")
                header.isEnabled = false
                header.view = MenuRowView(
                    title: header.title, image: AgentArtwork.image(for: group.vendor),
                    font: Self.agentMenuFont, minimumWidth: Self.menuWidth
                )
                menu.addItem(header)
            }
            for row in group.rows {
                menu.addItem(rowItem(row, showVendor: groups.count == 1 && group.rows.count == 1))
            }
        }
        for (index, billing) in store.enabledBilling.enumerated() {
            if !groups.isEmpty || index > 0 { menu.addItem(.separator()) }
            let balance = billing.balances.map { MoneyFormat.amount($0.total, currency: $0.currency) }.joined(separator: " / ")
            let title = (billing.billingPool == nil ? billing.vendor : billing.displayName) + L10n.text(" · 余额", " · Balance")
            let item = NSMenuItem(title: title, action: #selector(openStats), keyEquivalent: "")
            item.target = self
            item.view = MenuRowView(title: title, value: balance.isEmpty ? "—" : balance,
                                         image: AgentArtwork.image(for: billing.vendor), font: Self.agentMenuFont,
                                         valueColor: billing.isAvailable == false ? .systemRed : .secondaryLabelColor, minimumWidth: Self.menuWidth)
            let cost = billing.estimatedCost(currency: billing.currency, during: store.statsInterval)
                .map { MoneyFormat.amount($0, currency: billing.currency, estimated: true) } ?? "—"
            item.toolTip = (L10n.text("费用估算 · ", "Est. cost · ") + store.statsRange.recentLabel + ": " + cost)
            menu.addItem(item)
        }
        if store.rows.isEmpty && store.enabledBilling.isEmpty {
            let empty = NSMenuItem(title: L10n.text("没有启用的模型", "No agents enabled"), action: nil, keyEquivalent: "")
            empty.isEnabled = false
            empty.view = MenuRowView(title: empty.title, image: nil, font: .menuFont(ofSize: 13),
                                     titleColor: .secondaryLabelColor, minimumWidth: Self.menuWidth)
            menu.addItem(empty)
        }
        menu.addItem(.separator())
        menu.addItem(action(
            store.glowHidden ? L10n.text("显示灵动岛光晕", "Show notch glow") : L10n.text("隐藏灵动岛光晕", "Hide notch glow"),
            key: "h", modifiers: [.command, .option], selector: #selector(toggleGlow)
        ))
        menu.addItem(.separator())
        menu.addItem(action(L10n.text("设置…", "Settings…"), key: ",", modifiers: [.command], selector: #selector(openSettings)))
        for (index, entry) in actions.additional.enumerated() {
            let item = action(entry.title(), key: "", modifiers: [], selector: #selector(performAdditional(_:)))
            item.tag = index
            menu.addItem(item)
        }
        menu.addItem(action(L10n.text("退出", "Quit"), key: "q", modifiers: [.command], selector: #selector(quit)))
    }

    func menu(_ menu: NSMenu, willHighlight item: NSMenuItem?) {
        for item in menu.items { item.view?.needsDisplay = true }
    }


    private func rowItem(_ row: AgentRow, showVendor: Bool) -> NSMenuItem {
        let level = row.level ?? .ok
        let light = SystemAppearance.isLight
        let color = NSColor(StatusPalette.color(for: level, light: light))
        let label = L10n.modelLabel(row.agent.model)
        let name = showVendor ? "\(row.agent.displayVendor) · \(L10n.shortModelLabel(row.agent.model))" : label
        let value: String
        if let used = row.usedPct {
            value = "\(TokenFormat.percent(used)) · \(Countdown.resetLabelCompact(row.resetAt, now: store.now))"
        } else {
            value = row.missingQuotaLabel
        }
        let valueColor: NSColor = level == .critical ? NSColor(StatusPalette.textColor(for: .critical, light: light)) : .secondaryLabelColor
        let item = NSMenuItem(title: name, action: #selector(openStats), keyEquivalent: "")
        item.target = self
        item.view = MenuRowView(
            title: name, value: value,
            image: showVendor ? AgentArtwork.image(for: row.agent.vendor) : StatusIconRenderer.dot(color: row.level == nil ? .tertiaryLabelColor : color),
            font: showVendor ? Self.agentMenuFont : .menuFont(ofSize: 13), valueColor: valueColor, minimumWidth: Self.menuWidth
        )
        item.toolTip = store.quotaForecastHint(for: row.id)
        item.view?.toolTip = item.toolTip
        return item
    }

    private func action(_ title: String, key: String, modifiers: NSEvent.ModifierFlags, selector: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        item.target = self
        let modifierSymbols: [(NSEvent.ModifierFlags, String)] = [
            (.control, "⌃"), (.option, "⌥"), (.shift, "⇧"), (.command, "⌘"),
        ]
        let shortcut = modifierSymbols.filter { item.keyEquivalentModifierMask.contains($0.0) }
            .map(\.1).joined() + item.keyEquivalent.uppercased()
        item.view = MenuRowView(title: title, value: shortcut, image: nil,
                                font: .menuFont(ofSize: 13), minimumWidth: Self.menuWidth)
        return item
    }

    @objc private func performAdditional(_ sender: NSMenuItem) {
        actions.additional[sender.tag].action()
    }

    // MARK: Selectors

    @objc private func toggleGlow() { actions.toggleGlow() }
    @objc private func openSettings() { actions.openSettings() }
    @objc private func openStats() { actions.openStats() }
    @objc private func quit() { actions.quit() }
}

/// Shared columns and selection geometry for every non-separator menu item.
private final class MenuRowView: NSView {
    private let title: NSAttributedString
    private let value: NSAttributedString
    private let image: NSImage?
    private static let inset: CGFloat = 10
    private static let iconWidth: CGFloat = 16
    private static let titleX: CGFloat = inset + iconWidth + 4
    private static let selectionInset: CGFloat = 5
    private static let selectionRadius: CGFloat = 7

    init(title: String, value: String = "", image: NSImage?, font: NSFont, titleColor: NSColor = .labelColor, valueColor: NSColor = .secondaryLabelColor, minimumWidth: CGFloat) {
        self.title = NSAttributedString(string: title, attributes: [.font: font, .foregroundColor: titleColor])
        self.value = NSAttributedString(string: value, attributes: [
            .font: NSFont.monospacedDigitSystemFont(ofSize: font.pointSize, weight: .regular),
            .foregroundColor: valueColor,
        ])
        self.image = image
        let width = max(minimumWidth, Self.titleX + self.title.size().width + 20 + self.value.size().width + Self.inset)
        super.init(frame: NSRect(x: 0, y: 0, width: ceil(width), height: 24))
        autoresizingMask = [.width]
        setAccessibilityElement(true)
        setAccessibilityRole(.menuItem)
        setAccessibilityLabel(value.isEmpty ? title : "\(title), \(value)")
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func draw(_ dirtyRect: NSRect) {
        let highlighted = enclosingMenuItem?.isEnabled == true && enclosingMenuItem?.isHighlighted == true
        if highlighted {
            NSColor.selectedContentBackgroundColor.setFill()
            NSBezierPath(roundedRect: bounds.insetBy(dx: Self.selectionInset, dy: 0),
                         xRadius: Self.selectionRadius, yRadius: Self.selectionRadius).fill()
        }
        if let image {
            image.draw(in: NSRect(x: Self.inset + (Self.iconWidth - image.size.width) / 2, y: (bounds.height - image.size.height) / 2, width: image.size.width, height: image.size.height))
        }
        drawText(title, at: Self.titleX, highlighted: highlighted)
        drawText(value, at: bounds.width - Self.inset - value.size().width, highlighted: highlighted)
    }

    private func drawText(_ text: NSAttributedString, at x: CGFloat, highlighted: Bool) {
        let displayed = NSMutableAttributedString(attributedString: text)
        if highlighted {
            displayed.addAttribute(.foregroundColor, value: NSColor.selectedMenuItemTextColor, range: NSRange(location: 0, length: displayed.length))
        }
        displayed.draw(at: NSPoint(x: x, y: (bounds.height - text.size().height) / 2))
    }

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { _ = accessibilityPerformPress() }
    }

    override func isAccessibilityEnabled() -> Bool { enclosingMenuItem?.isEnabled == true }

    override func accessibilityPerformPress() -> Bool {
        guard let item = enclosingMenuItem, item.isEnabled, let menu = item.menu else { return false }
        menu.cancelTracking()
        menu.performActionForItem(at: menu.index(of: item))
        return true
    }
}
