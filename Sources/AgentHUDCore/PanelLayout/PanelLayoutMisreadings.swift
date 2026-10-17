import AgentHUDSupport
import Foundation

extension PanelLayout {
    /// What reading a layout's JSON would forgive, as sentences for the agent that sent it: a value of the wrong type,
    /// a name this build does not know, a span's and a period's among them, columns outside 2 to 6, a width outside 480
    /// to 720, a session limit outside 1 to 10, a hud style value outside its range, a property a block, the style, the
    /// sessions, the tokens, the hud or its glow does not have.
    /// Stored layouts are read forgivingly so that a newer build's layout still loads; an agent's proposal is told
    /// instead, since a forgiven name is a change that silently did not happen.
    static func misreadings(of json: JSONValue) -> [String] {
        guard let fields = json.objectValue else { return ["The layout is a JSON object."] }
        var found = names(fields["order"], at: "order", among: nil)
        found += names(fields["sections"], at: "sections", among: PanelSection.allCases.map(\.rawValue))
        found += names(fields["hiddenSections"], at: "hiddenSections", among: PanelSection.allCases.map(\.rawValue))
        if let countdown = fields["countdown"], countdown != .null, countdown.boolValue == nil {
            found.append("countdown is true or false.")
        }
        found += misreadings(ofStyle: fields["style"] ?? .null)
        found += misreadings(ofHUD: fields["hud"] ?? .null)
        found += misreadings(ofSessions: fields["sessions"] ?? .null, tokens: fields["tokens"] ?? .null)
        guard let blocks = fields["blocks"], blocks != .null else { return found }
        guard let blocks = blocks.objectValue else { return found + ["blocks is an object keyed by block key."] }
        for key in blocks.keys.sorted() {
            found += misreadings(ofBlock: blocks[key] ?? .null, at: "blocks.\(key)")
        }
        return found
    }

    private static func misreadings(ofBlock value: JSONValue, at path: String) -> [String] {
        guard let block = value.objectValue else { return ["\(path) is an object."] }
        var found: [String] = []
        for property in block.keys.sorted() {
            let value = block[property] ?? .null
            switch property {
            case "widget":
                found += misnamed(value, at: "\(path).widget", among: PanelWidget.allCases.map(\.rawValue))
            case "measures":
                found += names(value, at: "\(path).measures", among: PanelMeasure.allCases.map(\.rawValue))
            case "hidden", "countdown":
                if value != .null, value.boolValue == nil { found.append("\(path).\(property) is true or false.") }
            case "hiddenRows", "rows":
                if value != .null, value.arrayValue?.allSatisfy({ $0.stringValue != nil }) != true {
                    found.append("\(path).\(property) is a list of row keys.")
                }
            case "accounts":
                if value != .null, value.arrayValue?.allSatisfy({ $0.stringValue != nil }) != true {
                    found.append("\(path).accounts is a list of account ids from get_hud.")
                }
            case "columns":
                if value != .null, value.countValue.map({ Block.columnRange.contains($0) }) != true {
                    found.append("\(path).columns is a whole number from 2 to 6.")
                }
            case "ringSize":
                found += misnamed(value, at: "\(path).ringSize", among: PanelRingSize.allCases.map(\.rawValue))
            case "span":
                found += misnamed(value, at: "\(path).span", among: PanelSpan.allCases.map(\.rawValue))
            case "period":
                found += misnamed(value, at: "\(path).period", among: PanelTokenPeriod.allCases.map(\.rawValue))
            default:
                found.append("\(path) has no property \"\(property)\"; a block takes widget, measures, hidden, hiddenRows, "
                             + "rows, accounts, countdown, columns, ringSize, span and period.")
            }
        }
        return found
    }

    private static func misreadings(ofStyle value: JSONValue) -> [String] {
        if value == .null { return [] }
        guard let style = value.objectValue else { return ["style is an object."] }
        var found: [String] = []
        for property in style.keys.sorted() {
            let value = style[property] ?? .null
            switch property {
            case "spacing":
                found += misnamed(value, at: "style.spacing", among: PanelSpacing.allCases.map(\.rawValue))
            case "separators":
                if value != .null, value.boolValue == nil { found.append("style.separators is true or false.") }
            case "width":
                if value != .null, value.countValue.map({ PanelStyle.widthRange.contains($0) }) != true {
                    found.append("style.width is a whole number from \(PanelStyle.widthRange.lowerBound) to "
                                 + "\(PanelStyle.widthRange.upperBound).")
                }
            case "appearance", "material", "transparency":
                // The panel's style is no place for the HUD's look, which is one screen's, beside its other style fields.
                found.append("style has no property \"\(property)\"; the style takes spacing, separators and width. "
                             + "The HUD's \(property) is hud.\(property), for one screen.")
            default:
                found.append("style has no property \"\(property)\"; the style takes spacing, separators and width.")
            }
        }
        return found
    }

    /// What is wrong with a hud: its agents, as below, and its style fields (`HUDStyle.misreadings(ofField:_:)`).
    private static func misreadings(ofHUD value: JSONValue) -> [String] {
        if value == .null { return [] }
        guard let hud = value.objectValue else { return ["hud is an object."] }
        let kept = HUDStyle.listed(HUDStyle.KeptTransparency.allCases.map(\.rawValue))
        var found: [String] = []
        for property in hud.keys.sorted() {
            let value = hud[property] ?? .null
            guard property == "agents" else {
                found += HUDStyle.misreadings(ofField: property, value)
                    ?? ["hud has no property \"\(property)\"; the hud takes "
                        + "\(HUDStyle.listed(["agents"] + HUDStyle.names + [HUDStyle.resetName])), and \(kept) as "
                        + "get_hud lists them."]
                continue
            }
            found += names(value, at: "hud.agents", among: nil)
            for (index, item) in (value.arrayValue ?? []).enumerated() where item.stringValue?.isEmpty == true {
                found.append("hud.agents[\(index)] is an agent's name from get_hud's screens.")
            }
        }
        return found
    }

    /// What is wrong with a list of names: not a list, an entry that is not a string, or one that is not `allowed`.
    static func names(_ value: JSONValue?, at path: String, among allowed: [String]?) -> [String] {
        guard let value, value != .null else { return [] }
        guard let items = value.arrayValue else { return ["\(path) is a list."] }
        var found: [String] = []
        for (index, item) in items.enumerated() {
            guard let name = item.stringValue else { found.append("\(path)[\(index)] is a string."); continue }
            if let allowed, !allowed.contains(name) {
                found.append("\(path)[\(index)]: \"\(name)\" is not one of \(quoted(allowed)).")
            }
        }
        return found
    }

    static func misnamed(_ value: JSONValue, at path: String, among allowed: [String]) -> [String] {
        if value == .null { return [] }
        guard let name = value.stringValue, allowed.contains(name) else {
            return ["\(path) is one of \(quoted(allowed))."]
        }
        return []
    }
}
