import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// The tools a client is offered, and what comes back from calling one.
final class MCPToolsTests: XCTestCase {
    private let probe = MCPServerProbe()

    private func call(_ params: String, id: String = "5") throws -> JSONValue {
        try probe.response(to: #"{"jsonrpc":"2.0","id":\#(id),"method":"tools/call","params":\#(params)}"#)
    }

    /// The one block of text a call's result holds.
    private func text(of response: JSONValue, file: StaticString = #filePath, line: UInt = #line) throws -> String {
        let content = try XCTUnwrap(response["result"]["content"].arrayValue, file: file, line: line)
        XCTAssertEqual(content.count, 1, file: file, line: line)
        XCTAssertEqual(content.first?["type"], .string("text"), file: file, line: line)
        return try XCTUnwrap(content.first?["text"].stringValue, file: file, line: line)
    }

    /// Checks that `schema` is a JSON Schema the clients take: a typed node whose parts are typed nodes, described on
    /// one line where the task of describing falls to it.
    private func assertSchema(_ schema: JSONValue, at path: String, file: StaticString = #filePath, line: UInt = #line) throws {
        let node = try XCTUnwrap(schema.objectValue, "\(path) is an object", file: file, line: line)
        let known: Set<String> = ["type", "description", "properties", "additionalProperties", "items", "enum"]
        XCTAssertTrue(Set(node.keys).isSubset(of: known), "\(path): \(node.keys.sorted())", file: file, line: line)
        let type = try XCTUnwrap(node["type"]?.stringValue, "\(path) has a type", file: file, line: line)
        XCTAssertTrue(["object", "array", "string", "boolean", "integer", "number"].contains(type), "\(path): \(type)", file: file, line: line)
        if let description = node["description"] {
            let sentence = try XCTUnwrap(description.stringValue, file: file, line: line)
            XCTAssertFalse(sentence.isEmpty || sentence.contains("\n"), "\(path) is described on one line", file: file, line: line)
        }
        if let choices = node["enum"] {
            XCTAssertEqual(type, "string", file: file, line: line)
            XCTAssertFalse(try XCTUnwrap(choices.arrayValue).isEmpty, file: file, line: line)
            XCTAssertTrue(choices.arrayValue?.allSatisfy { $0.stringValue != nil } == true, path, file: file, line: line)
        }
        for (name, property) in node["properties"]?.objectValue ?? [:] {
            XCTAssertNotNil(property["description"].stringValue, "\(path).\(name) says what it is", file: file, line: line)
            try assertSchema(property, at: "\(path).\(name)", file: file, line: line)
        }
        if type == "array" { try assertSchema(try XCTUnwrap(node["items"]), at: path + "[]", file: file, line: line) }
        if type == "object", let rest = node["additionalProperties"], rest != .bool(false) {
            try assertSchema(rest, at: path + ".*", file: file, line: line)
        }
    }

    func testTheListHoldsTheFiveToolsWithTheirSchemas() throws {
        // Codex 0.162.1 asks with a progress token and nothing else.
        let response = try probe.response(to: #"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"progressToken":0}}}"#)
        XCTAssertEqual(response["result"].objectValue?.keys.sorted(), ["tools"], "a handshake client gets the list and nothing else")
        let tools = try XCTUnwrap(response["result"]["tools"].arrayValue)
        XCTAssertEqual(tools.map { $0["name"].stringValue },
                       ["get_hud", "get_hud_screenshot", "draft_hud", "discard_hud_draft", "finish_hud_edit"])
        XCTAssertEqual(try probe.response(to: #"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)["result"], response["result"],
                       "the same list in the same order every time, so a client may cache it")

        for tool in tools {
            let name = try XCTUnwrap(tool["name"].stringValue)
            XCTAssertEqual(tool.objectValue?.keys.sorted(), ["annotations", "description", "inputSchema", "name"], "no output schema is declared")
            XCTAssertNotNil(name.range(of: "^[A-Za-z0-9_-]{1,64}$", options: .regularExpression), name)
            XCTAssertLessThanOrEqual(tool["description"].stringValue?.count ?? .max, 2048, "\(name) is described within what a client shows whole")
            XCTAssertEqual(tool["inputSchema"]["type"], .string("object"), name)
            XCTAssertEqual(tool["inputSchema"]["additionalProperties"], .bool(false), "\(name) takes no property it does not name")
            try assertSchema(tool["inputSchema"], at: name)
        }
        XCTAssertEqual([MCPTools.getHUD, MCPTools.getHUDScreenshot, MCPTools.draftHUD, MCPTools.discardHUDDraft,
                        MCPTools.finishHUDEdit], tools.map { $0["name"].stringValue })
        for index in [0, 1, 3] {
            XCTAssertEqual(tools[index]["inputSchema"]["properties"].objectValue?.keys.sorted(),
                           index == 1 ? ["agent", "screen", "show"] : ["screen"],
                           "\(tools[index]["name"]) takes the screen it acts on and, for a picture, what to draw and for which agent")
            XCTAssertEqual(tools[index]["inputSchema"]["properties"]["screen"], tools[2]["inputSchema"]["properties"]["screen"],
                           "every tool names a screen the same way")
        }
        XCTAssertEqual(tools[1]["inputSchema"]["properties"]["show"]["enum"], .array([.string("panel"), .string("hud")]))
        XCTAssertEqual(tools[1]["inputSchema"]["properties"]["agent"]["type"], .string("string"))
        XCTAssertEqual(tools[1]["inputSchema"]["properties"]["agent"]["description"].stringValue,
                       "An agent by name from that screen's shows in get_hud, or by its block's key, to draw its own panel "
                       + "instead of the whole one: the card a click on its mark opens beside it, agentPanelWidth wide, on a "
                       + "dynamic or simple HUD with separateAgents on. Needs a screen key; not with show 'hud'.")
        XCTAssertEqual(tools[4]["inputSchema"]["properties"].objectValue?.keys.sorted(), ["screen"],
                       "finish_hud_edit takes the screen it lets go of and nothing else")
        XCTAssertEqual(tools[4]["inputSchema"]["properties"]["screen"]["description"].stringValue,
                       "A screen key from get_hud to let go of that screen's panel; 'all', the default, for every screen's.")
        XCTAssertFalse(tools.contains { $0["name"].stringValue?.contains("save") == true }, "saving is the user's click in the panel")
    }

    func testTheLayoutToolTakesTheLayoutAndNothingElse() throws {
        let schema = try XCTUnwrap(MCPTools.definitions.first { $0["name"] == .string("draft_hud") })["inputSchema"]
        let properties = try XCTUnwrap(schema["properties"].objectValue)
        XCTAssertEqual(properties.keys.sorted(),
                       ["app", "blocks", "countdown", "from", "hiddenSections", "hud", "order", "revision", "screen", "sections",
                        "sessions", "style", "tokens"])
        XCTAssertNil(schema.objectValue?["required"], "every property is optional")
        func choices(_ node: JSONValue) -> [String] { node["enum"].arrayValue?.compactMap(\.stringValue) ?? [] }

        let block = schema["properties"]["blocks"]["additionalProperties"]
        XCTAssertEqual(schema["properties"]["blocks"]["type"], .string("object"))
        XCTAssertEqual(block["additionalProperties"], .bool(false))
        XCTAssertEqual(block["properties"].objectValue?.keys.sorted(),
                       ["accounts", "columns", "countdown", "hidden", "hiddenRows", "measures", "period", "ringSize", "rows", "span",
                        "widget"])
        XCTAssertEqual(block["properties"]["rows"]["type"], .string("array"))
        XCTAssertEqual(block["properties"]["accounts"]["type"], .string("array"))
        XCTAssertEqual(schema["properties"]["countdown"]["type"], .string("boolean"))
        XCTAssertEqual(choices(block["properties"]["widget"]), ["bar", "ring", "number", "chart"])
        XCTAssertEqual(block["properties"]["measures"]["type"], .string("array"))
        XCTAssertEqual(choices(block["properties"]["measures"]["items"]), ["quota", "burn", "tokens", "cost"])
        XCTAssertEqual(block["properties"]["hidden"]["type"], .string("boolean"))
        XCTAssertEqual(block["properties"]["hiddenRows"]["items"], .object(["type": .string("string")]))
        XCTAssertEqual(block["properties"]["countdown"]["type"], .string("boolean"))
        XCTAssertEqual(block["properties"]["columns"]["type"], .string("integer"))
        XCTAssertEqual(choices(block["properties"]["ringSize"]), ["small", "medium", "large"])
        XCTAssertEqual(choices(block["properties"]["span"]), ["full", "half"])
        XCTAssertEqual(choices(block["properties"]["period"]), ["day", "week", "month"])

        let style = schema["properties"]["style"]
        XCTAssertEqual(style["type"], .string("object"))
        XCTAssertEqual(style["additionalProperties"], .bool(false))
        XCTAssertEqual(style["properties"].objectValue?.keys.sorted(), ["separators", "spacing", "width"])
        XCTAssertEqual(choices(style["properties"]["spacing"]), ["compact", "regular", "roomy"])
        XCTAssertEqual(style["properties"]["separators"]["type"], .string("boolean"))
        XCTAssertEqual(style["properties"]["width"]["type"], .string("integer"))
        XCTAssertEqual(style["properties"]["width"]["description"].stringValue,
                       "The whole panel's width, 480 to 720 points; 540 if left out.")

        let sessions = schema["properties"]["sessions"]
        XCTAssertEqual(sessions["additionalProperties"], .bool(false))
        XCTAssertEqual(sessions["properties"].objectValue?.keys.sorted(), ["agents", "limit", "period", "projects", "sort", "states"])
        XCTAssertEqual(sessions["properties"]["limit"]["type"], .string("integer"))
        XCTAssertEqual(sessions["properties"]["limit"]["description"].stringValue, "Rows at most, 1 to 10; 5 if left out.")
        XCTAssertEqual(sessions["properties"]["agents"]["items"], .object(["type": .string("string")]))
        XCTAssertEqual(sessions["properties"]["projects"]["items"], .object(["type": .string("string")]))
        XCTAssertEqual(choices(sessions["properties"]["states"]["items"]), ["waiting", "running", "ended"])
        XCTAssertEqual(choices(sessions["properties"]["period"]), ["day", "week"])
        XCTAssertEqual(choices(sessions["properties"]["sort"]), ["active", "date"])
        XCTAssertEqual(sessions["properties"]["period"]["description"].stringValue,
                       "How long after its last activity an ended session is still listed: day, the last 24 hours, the default, "
                       + "or week, the last 7 days. Sessions at work are listed whatever the period.")
        XCTAssertEqual(sessions["properties"]["sort"]["description"].stringValue,
                       "How the rows are ordered: active, the default, lists sessions at work first, those waiting for approval "
                       + "ahead of those running, then the others by their last activity; date lists every session by its last "
                       + "activity, newest first, whatever its state.")
        XCTAssertEqual(sessions["properties"]["projects"]["description"].stringValue,
                       "Only these projects' sessions, ids from get_hud's sessions.offeredProjects: a project's folder written "
                       + "with ~, a Git repository's worktrees counted as one project, or unassigned for sessions with no folder. "
                       + "A path, absolute or with ~, inside a project or a worktree of it names that project, so your own "
                       + "working directory names the project you work in; it is listed by its id. None for every project.")
        let tokens = schema["properties"]["tokens"]
        XCTAssertEqual(tokens["additionalProperties"], .bool(false))
        XCTAssertEqual(tokens["properties"].objectValue?.keys.sorted(), ["agents", "models", "period"])
        XCTAssertEqual(choices(tokens["properties"]["period"]), ["day", "week", "month"])
        XCTAssertEqual(tokens["properties"]["agents"]["items"], .object(["type": .string("string")]))
        XCTAssertEqual(tokens["properties"]["models"]["items"], .object(["type": .string("string")]))

        XCTAssertEqual(schema["properties"]["order"]["items"], .object(["type": .string("string")]))
        for name in ["sections", "hiddenSections"] {
            XCTAssertEqual(schema["properties"][name]["type"], .string("array"))
            XCTAssertEqual(choices(schema["properties"][name]["items"]), ["blocks", "tokens", "sessions"], name)
        }
        XCTAssertEqual(choices(schema["properties"]["from"]), ["default", "previous", "shared"])
        XCTAssertEqual(schema["properties"]["revision"]["type"], .string("integer"))
        XCTAssertEqual(schema["properties"]["screen"]["type"], .string("string"))
        XCTAssertEqual(schema["properties"]["screen"]["description"].stringValue,
                       "A screen key from get_hud for that screen's own layout; 'all', the default, for the shared layout the other screens use.")

        let hud = schema["properties"]["hud"]
        XCTAssertEqual(hud["type"], .string("object"))
        XCTAssertEqual(hud["additionalProperties"], .bool(false))
        XCTAssertEqual(hud["properties"].objectValue?.keys.sorted(),
                       ["agents", "appearance", "edge", "edgeDistance", "glassStyle", "glassTransparency", "glow", "logoGap",
                        "logoSize", "material", "mode", "position", "reset", "ringWidth", "separateAgents", "showLogos",
                        "showNumbers", "showRings", "simpleGap", "simpleSize", "solidTransparency", "transparency"])
        XCTAssertEqual(hud["properties"]["reset"]["type"], .string("boolean"))
        XCTAssertEqual(hud["properties"]["reset"]["description"].stringValue,
                       "True proposes the screen's default style, the placement its hardware is given, dark and solid, and the default glow, with no other style field.")
        XCTAssertEqual(hud["description"].stringValue,
                       "The resting HUD: the marks of a Dynamic dock, the items of a Simple panel, the glow, its colours and its material; a style field left out stays as saved.")
        XCTAssertEqual(choices(hud["properties"]["appearance"]), ["dark", "light", "system"])
        XCTAssertEqual(choices(hud["properties"]["material"]), ["solid", "glass"])
        XCTAssertEqual(hud["properties"]["appearance"]["description"].stringValue,
                       "Its colours, its panel's and its cards' too: dark, light, or system to follow macOS's light or dark setting. At a hardware notch a notch HUD stays dark in the notch's row, black or black glass; the panel and the cards it opens into take the look.")
        XCTAssertEqual(hud["properties"]["material"]["description"].stringValue,
                       "Solid, or the system's glass, which shows the desktop through it blurred, light or dark as the appearance and the desktop make it; solid while Reduce Transparency is on. At a hardware notch a notch HUD stays dark in the notch's row, black or black glass; the panel and the cards it opens into take the look.")
        XCTAssertEqual(hud["properties"]["transparency"]["type"], .string("number"))
        XCTAssertEqual(hud["properties"]["transparency"]["description"].stringValue,
                       "How much of the desktop shows through, glass's or solid's as the material is: glass lays less of the appearance's colour beneath it, solid draws less of its fill, all of it at 0; Solid, Clear and Regular draw three fifths of it, 0.6 at 1, and Material all of it; 0 to 1. It sets the transparency of the material the HUD would have; get_hud lists what each keeps as solidTransparency and glassTransparency.")
        XCTAssertEqual(choices(hud["properties"]["glassStyle"]), ["clear", "regular", "material"])
        XCTAssertEqual(hud["properties"]["glassStyle"]["description"].stringValue,
                       "What glass is made of: clear or regular Liquid Glass, which the HUD draws alike, as clear, and a window that matches its screen as regular, or material, an even frosted blur; Clear and Regular are a frosted blur before macOS 26. Kept while the material is solid, as Settings keeps it.")
        for (name, material) in [("solidTransparency", "solid's"), ("glassTransparency", "glass's")] {
            XCTAssertEqual(hud["properties"][name]["type"], .string("number"), name)
            XCTAssertEqual(hud["properties"][name]["description"].stringValue,
                           "Read only: \(material) transparency as get_hud lists it, which can be sent back as listed; transparency sets the material's own.")
        }
        XCTAssertEqual(schema["properties"]["from"]["description"].stringValue,
                       "Propose the built-in layout, the last saved one, or for a screen the shared layout, instead of the one described here. Only 'previous' brings a screen's HUD style with it.")
        XCTAssertEqual(hud["properties"]["agents"]["type"], .string("array"))
        XCTAssertEqual(hud["properties"]["agents"]["items"], .object(["type": .string("string")]))
        XCTAssertEqual(choices(hud["properties"]["mode"]), ["notch", "dynamic", "simple"])
        XCTAssertEqual(choices(hud["properties"]["edge"]), ["top", "right", "bottom", "left"])
        for name in ["position", "logoSize", "logoGap", "edgeDistance", "simpleSize", "simpleGap", "ringWidth", "transparency"] {
            XCTAssertEqual(hud["properties"][name]["type"], .string("number"), name)
        }
        for name in ["showLogos", "showRings", "showNumbers", "separateAgents"] {
            XCTAssertEqual(hud["properties"][name]["type"], .string("boolean"), name)
        }
        XCTAssertEqual(hud["properties"]["logoSize"]["description"].stringValue, "Dynamic: logo size in points, 20 to 60.")
        XCTAssertEqual(hud["properties"]["simpleGap"]["description"].stringValue,
                       "Simple: gap between agents, as a share of a ring, 0.1 to 0.6.")
        XCTAssertEqual(hud["properties"]["logoGap"]["description"].stringValue,
                       "Dynamic: gap between logos, as a share of a logo, 0.1 to 0.6.")
        let glow = hud["properties"]["glow"]
        XCTAssertEqual(glow["additionalProperties"], .bool(false))
        XCTAssertEqual(glow["properties"].objectValue?.keys.sorted(),
                       ["amplitude", "blur", "brightness", "effect", "gridCore", "gridDensity", "gridFade", "gridPitch",
                        "idleSpeed", "outwardOnly", "range", "speed", "style"])
        XCTAssertEqual(choices(glow["properties"]["style"]), ["blur", "dots", "ascii", "blocks", "braille", "binary"])
        XCTAssertEqual(choices(glow["properties"]["effect"]), ["breathe", "flow", "scan", "ripple", "shimmer", "boot"])
        XCTAssertEqual(glow["properties"]["speed"]["description"].stringValue,
                       "Seconds a cycle takes while an agent works, 1 to 12.")
        XCTAssertEqual(glow["properties"]["brightness"]["description"].stringValue, "Its strength, 0.2 to 1.")
        XCTAssertEqual(glow["properties"]["idleSpeed"]["description"].stringValue,
                       "Seconds a cycle takes while every agent is idle, 1 to 24 in steps of 0.5.")
        XCTAssertEqual(glow["properties"]["gridPitch"]["description"].stringValue,
                       "Grid styles, all but blur: spacing between cells in points, 4 to 12 in whole steps.")
        XCTAssertEqual(glow["properties"]["gridDensity"]["description"].stringValue,
                       "Grid styles: how much of its cell a mark fills, past 1 into its neighbours, 0.5 to 1.5 in steps of 0.05.")
        XCTAssertEqual(glow["properties"]["gridCore"]["description"].stringValue,
                       "Grid styles: rows held at full strength, 0 to 8 in whole steps.")
        XCTAssertEqual(glow["properties"]["gridFade"]["description"].stringValue,
                       "Grid styles: rows it fades over after them, 0 to 8 in whole steps.")
        for name in ["idleSpeed", "gridPitch", "gridDensity", "gridCore", "gridFade"] {
            XCTAssertEqual(glow["properties"][name]["type"], .string("number"), name)
        }
    }

    /// The names the schema offers are the names the layout reads, so an agent is never offered one that is refused.
    func testTheSchemaOffersTheNamesTheLayoutReads() throws {
        let schema = try XCTUnwrap(MCPTools.definitions.first { $0["name"] == .string("draft_hud") })["inputSchema"]
        func choices(_ node: JSONValue) -> [String] { node["enum"].arrayValue?.compactMap(\.stringValue) ?? [] }
        let block = schema["properties"]["blocks"]["additionalProperties"]["properties"]
        XCTAssertEqual(choices(block["widget"]), PanelWidget.allCases.map(\.rawValue))
        XCTAssertEqual(choices(block["measures"]["items"]), PanelMeasure.allCases.map(\.rawValue))
        XCTAssertEqual(choices(block["ringSize"]), PanelRingSize.allCases.map(\.rawValue))
        XCTAssertEqual(choices(block["span"]), PanelSpan.allCases.map(\.rawValue))
        XCTAssertEqual(choices(schema["properties"]["style"]["properties"]["spacing"]), PanelSpacing.allCases.map(\.rawValue))
        XCTAssertEqual(choices(schema["properties"]["sections"]["items"]), PanelSection.allCases.map(\.rawValue))
        XCTAssertEqual(choices(schema["properties"]["sessions"]["properties"]["states"]["items"]),
                       PanelSessionState.allCases.map(\.rawValue))
        XCTAssertEqual(choices(schema["properties"]["tokens"]["properties"]["period"]), PanelTokenPeriod.allCases.map(\.rawValue))
        XCTAssertEqual(choices(schema["properties"]["sessions"]["properties"]["period"]), PanelSessionPeriod.allCases.map(\.rawValue))
        XCTAssertEqual(choices(schema["properties"]["sessions"]["properties"]["sort"]), PanelSessionSort.allCases.map(\.rawValue))
        XCTAssertEqual(choices(block["period"]), PanelTokenPeriod.allCases.map(\.rawValue))
        let hud = schema["properties"]["hud"]["properties"]
        XCTAssertEqual(choices(hud["mode"]), HUDMode.allCases.map(\.agentName))
        XCTAssertEqual(choices(hud["edge"]), HUDEdge.allCases.map(\.rawValue))
        XCTAssertEqual(choices(hud["appearance"]), HUDAppearance.allCases.map(\.rawValue))
        XCTAssertEqual(choices(hud["material"]), HUDMaterial.allCases.map(\.rawValue))
        XCTAssertEqual(choices(hud["glow"]["properties"]["style"]), GlowStyle.allCases.map(\.rawValue))
        XCTAssertEqual(choices(hud["glow"]["properties"]["effect"]), GlowEffect.allCases.map(\.rawValue))
        XCTAssertEqual(choices(hud["glassStyle"]), HUDGlassStyle.allCases.map(\.rawValue))
        XCTAssertEqual(Set(hud.objectValue?.keys.map { $0 } ?? []),
                       Set(["agents"] + HUDStyle.names + [HUDStyle.resetName] + HUDStyle.KeptTransparency.allCases.map(\.rawValue)),
                       "the hud's fields are the ones the strict reading knows")
        XCTAssertEqual(Set(hud["glow"]["properties"].objectValue?.keys.map { $0 } ?? []),
                       Set(HUDStyle.GlowField.allCases.map(\.rawValue)), "and the glow's")

        // The app's settings take what get_hud lists under app, Hover behavior's under hover, the read-only language
        // with them, and nothing else.
        let app = schema["properties"]["app"]
        XCTAssertEqual(app["additionalProperties"], .bool(false))
        XCTAssertEqual(app["properties"].objectValue?.keys.sorted(),
                       ["appearance", "botLogos", "hover", "language", "matchScreen", "showMenuBarIcon", "writtenIn"])
        XCTAssertEqual(Set(app["properties"].objectValue?.keys.map { $0 } ?? []),
                       Set(AppDraft.Field.allCases.filter { !$0.isHover }.map(\.rawValue) + ["hover"] + AppDraft.readOnly),
                       "the app's fields are the ones the strict reading knows")
        XCTAssertEqual(choices(app["properties"]["appearance"]), AppearanceMode.allCases.map(\.rawValue))
        let hover = app["properties"]["hover"]
        XCTAssertEqual(hover["additionalProperties"], .bool(false))
        XCTAssertEqual(Set(hover["properties"].objectValue?.keys.map { $0 } ?? []),
                       Set(AppDraft.Field.allCases.filter(\.isHover).map(\.rawValue)), "and the hover's")
        XCTAssertEqual(hover["properties"]["delayMs"]["type"], .string("integer"))
        XCTAssertEqual(hover["properties"]["delayMs"]["description"].stringValue,
                       "How long hovering waits before it opens the panel, in milliseconds, 0 to 1500 in steps of 50.")
        XCTAssertTrue(app["description"].stringValue?.contains("only when the user explicitly asks for one of these "
                                                                + "settings, never as a side effect of arranging the panel") == true,
                      "an agent is told when it may change them")
    }

    func testEachToolSaysWhatItDoesAndWhatItMayChange() throws {
        let tools = Dictionary(uniqueKeysWithValues: MCPTools.definitions.map { ($0["name"].stringValue ?? "", $0) })
        XCTAssertEqual(tools["get_hud"]?["description"].stringValue,
                       "Read what the Agent HUD hover panel shows now: its blocks (one per agent or API provider), each block's accounts, with the id a layout names one by, its email or name and its plan, and its rows in the order shown with their latest readings, and each API balance with runsOutAt, when it runs out at the pace the balance itself fell over the last day, and over the block's period cost, the estimated cost of the requests recorded, burnPerDay, that cost per day, and burnRunsOutAt, when the balance runs out at that pace; the layout in effect, whether reset countdowns show, the panel's width and height and whether it scrolls, and the widgets and measures each block accepts; how many sessions it lists, over what period and in what order, and the agents and projects it can narrow them to, each project with its id and name; its token chart's period, the agents and models it can draw and its total; the attached screens, each with its resting HUD's style, the look it draws now, the agents it can show, agentPanelWidth, the width of one agent's own panel on a dock that separates its agents, and whether an edit holds its panel open; and app, the whole app's settings as drawn now, with appUnsaved, whether an unsaved draft of them waits: appearance; matchScreen, with which a screen's style draft also restyles the app's windows and the menu-bar menu on that screen; botLogos, whether marks turn into bots on a Dynamic dock, a Simple panel and reminders; showMenuBarIcon; language, and writtenIn, the language names are written in, both read only; and hover: pressOptionToOpen, openAtTopEdge, and delayMs and collapseDelayMs in milliseconds. Call it before changing the layout; block, row, account and screen keys and project ids come from here. Names are written as the panel shows them, in the application's language.")
        XCTAssertEqual(tools["get_hud_screenshot"]?["description"].stringValue,
                       "Show a picture of the Agent HUD hover panel as the layout in effect draws it, a draft included, account names masked, session titles left out, or with agent one agent's own panel, or with show 'hud' a screen's resting HUD; glass is one stand-in. Use it to check a change; get_hud reads it as data.")
        XCTAssertLessThanOrEqual(tools["get_hud_screenshot"]?["description"].stringValue?.count ?? .max, 300)
        XCTAssertEqual(tools["draft_hud"]?["description"].stringValue,
                       "Propose a layout for the Agent HUD hover panel: its blocks, their rows' order and the accounts each shows, its sections and reset countdowns, which are also the switches of Settings' Expanded panel, its session rows, token chart and style, its width included, and the screen's resting HUD: which agents it shows and, for one screen, its mode, edge, sizes, appearance, material, glass style and glow. Send the whole layout you want; anything left out uses the default, and a HUD style field left out stays as saved. A call with only hud style fields leaves the layout as it is. What the layout from get_hud holds for a block, a row, an account, an agent, a project or a model not listed now can be sent back as it is. It shows at once as an unsaved draft: the user keeps it with Save in the panel, and it is dropped when Agent HUD restarts. Pass from 'default' to propose the built-in layout instead, 'previous' for the last saved one, for a screen with the resting HUD style that went with it, or 'shared' to have a screen use the shared layout again; hud.reset proposes a screen's default HUD style. A layout edit holds the panel open on screen for two minutes from the call, and a call with only hud style fields closes it so the HUD shows; call finish_hud_edit when you are done. app changes the whole app's own settings, its appearance, matchScreen, botLogos, showMenuBarIcon and hover, for every screen, as a draft the same Save keeps: name it only when the user explicitly asks for one of those settings, never as a side effect of arranging the panel. A call that leaves app out leaves them as they are, and a call with only app changes them alone and opens nothing.")
        XCTAssertEqual(tools["discard_hud_draft"]?["description"].stringValue,
                       "Drop the unsaved draft so the Agent HUD panel returns to the saved layout and, for a screen, its resting HUD to its saved style; a draft of the app's own settings stays. Like a layout edit it holds the panel open, unless it drops a HUD style alone.")
        XCTAssertEqual(tools["finish_hud_edit"]?["description"].stringValue,
                       "Say your edit of the Agent HUD panel is done, so the panel an edit held open closes unless the user's pointer is on it. Drafts stay as they are, waiting for the user's Save.")

        XCTAssertEqual(tools["get_hud"]?["annotations"], .object(["readOnlyHint": .bool(true), "openWorldHint": .bool(false)]))
        XCTAssertEqual(tools["get_hud_screenshot"]?["annotations"], .object(["readOnlyHint": .bool(true), "openWorldHint": .bool(false)]))
        let draft: JSONValue = .object(["readOnlyHint": .bool(false), "destructiveHint": .bool(false),
                                        "idempotentHint": .bool(true), "openWorldHint": .bool(false)])
        XCTAssertEqual(tools["draft_hud"]?["annotations"], draft)
        XCTAssertEqual(tools["discard_hud_draft"]?["annotations"], draft)
        XCTAssertEqual(tools["finish_hud_edit"]?["annotations"], draft, "letting go of the panel destroys nothing and may be repeated")
    }

    func testAToolTheServerDoesNotHaveIsTheProtocolsError() throws {
        let response = try call(#"{"name":"save_panel_layout","arguments":{}}"#, id: "3")
        XCTAssertEqual(response["id"], .integer(3))
        XCTAssertEqual(response["error"], .object(["code": .integer(-32602), "message": .string("Unknown tool: save_panel_layout")]))
        XCTAssertEqual(try call(#"{"arguments":{}}"#)["error"]["code"], .integer(-32602), "a call names its tool")
        XCTAssertEqual(try call(#"{"name":7}"#)["error"]["code"], .integer(-32602))
        XCTAssertEqual(try call(#"{"name":"get_hud","arguments":[1]}"#)["error"]["code"], .integer(-32602), "arguments are an object")
        XCTAssertEqual(try call(#"{"name":"get_hud","arguments":"{}"}"#)["error"]["code"], .integer(-32602))
        XCTAssertTrue(probe.calls.isEmpty, "none of these reaches the application")
    }

    func testACallWithNoApplicationRunningSaysSo() throws {
        probe.answer = .notRunning
        // Claude Code 2.1.296 on a handshake connection.
        let line = #"{"method":"tools/call","params":{"name":"get_hud","arguments":{},"_meta":{"claudecode/toolUseId":"toolu_1","progressToken":2}},"jsonrpc":"2.0","id":2}"#
        let response = try probe.response(to: line)
        XCTAssertEqual(response["id"], .integer(2))
        XCTAssertEqual(try text(of: response), "Agent HUD is not running. Open Agent HUD and try again.")
        XCTAssertEqual(response["result"]["isError"], .bool(true))
        XCTAssertEqual(response["result"].objectValue?.keys.sorted(), ["content", "isError"],
                       "a client's own _meta does not make the request one of the newest revision")
        XCTAssertEqual(probe.calls, [MCPToolRequest(tool: "get_hud")])

        // Codex 0.162.1 puts the turn it runs in beside the call.
        let codex = #"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":{"callId":"exec-1","x-codex-turn-metadata":{"session_id":"thread-1","sandbox":"seatbelt","codex_version":"0.162.1"},"threadId":"thread-1","progressToken":1},"name":"get_hud","arguments":{}}}"#
        XCTAssertEqual(try probe.response(to: codex)["result"], response["result"])
    }

    func testWhatTheApplicationReturnsComesBackAsOneBlockOfJSON() throws {
        let result: [String: JSONValue] = ["revision": .integer(12), "saved": .bool(false), "share": .number(0.25),
                                           "blocks": .array([.object(["key": .string("claude"), "title": .string("Claude\nMax \"20x\"")])])]
        probe.answer = .reply(.result(result))
        let response = try call(#"{"name":"get_hud"}"#)
        XCTAssertEqual(response["result"]["isError"], .bool(false))
        XCTAssertNil(response["result"].objectValue?["structuredContent"])
        let text = try text(of: response)
        XCTAssertFalse(text.contains("\n") || text.contains(": "), "compact, on one line")
        XCTAssertEqual(try JSONValue.parse(Data(text.utf8)), .object(result))
        XCTAssertEqual(probe.calls, [MCPToolRequest(tool: "get_hud")], "arguments left out are none")
    }

    /// A picture comes after the text that says what it shows, as the image block every revision the server answers
    /// defines: the PNG in base64 with its media type.
    func testAPictureTheApplicationDrewComesBackAsAnImageBlockAfterTheText() throws {
        let png = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00, 0x3E])
        let result: [String: JSONValue] = ["width": .integer(540), "height": .integer(741), "scrolls": .bool(false)]
        probe.answer = .reply(.result(result, images: [MCPImage(mimeType: "image/png", data: png)]))
        let image: JSONValue = .object(["type": .string("image"), "data": .string(png.base64EncodedString()),
                                        "mimeType": .string("image/png")])
        for meta in ["", "," + MCPServerProbe.meta] {
            let response = try call(#"{"name":"get_hud_screenshot","arguments":{}\#(meta)}"#)
            let content = try XCTUnwrap(response["result"]["content"].arrayValue)
            XCTAssertEqual(content.count, 2, meta)
            XCTAssertEqual(content[0]["type"], .string("text"))
            XCTAssertEqual(try JSONValue.parse(Data(try XCTUnwrap(content[0]["text"].stringValue).utf8)), .object(result))
            XCTAssertEqual(content[1], image, "the block has its type, its data and its media type, and nothing else")
            XCTAssertEqual(Data(base64Encoded: content[1]["data"].stringValue ?? ""), png)
            XCTAssertEqual(response["result"]["isError"], .bool(false))
            XCTAssertEqual(response["result"]["resultType"], meta.isEmpty ? .null : .string("complete"),
                           "the newest revision says the result is complete; a handshake client gets the content alone")
        }
        XCTAssertEqual(probe.calls, [MCPToolRequest(tool: "get_hud_screenshot"), MCPToolRequest(tool: "get_hud_screenshot")])

        probe.answer = .reply(.failure("This copy of Agent HUD cannot draw the panel."))
        let refused = try call(#"{"name":"get_hud_screenshot"}"#)
        XCTAssertEqual(try text(of: refused), "This copy of Agent HUD cannot draw the panel.", "a refusal is its sentence and no picture")
        XCTAssertEqual(refused["result"]["isError"], .bool(true))
    }

    func testArgumentsReachTheApplicationAsTheyCame() throws {
        probe.answer = .reply(.result(["revision": .integer(13)]))
        let arguments = #"{"blocks":{"claude":{"widget":"ring","hiddenRows":["weekly"]}},"order":["codex","claude"],"revision":12,"unknown":null}"#
        _ = try call(#"{"name":"draft_hud","arguments":\#(arguments)}"#)
        _ = try call(#"{"name":"discard_hud_draft","arguments":{}}"#)
        let layout = MCPToolRequest(tool: "draft_hud",
                                    arguments: try XCTUnwrap(try JSONValue.parse(Data(arguments.utf8)).objectValue))
        XCTAssertEqual(probe.calls, [layout, MCPToolRequest(tool: "discard_hud_draft")], "checking them is the application's job")
    }

    func testARefusalOrSilenceFromTheApplicationIsAnErrorResult() throws {
        probe.answer = .reply(.failure("The layout changed since revision 12. Call get_hud and send the layout again."))
        let refused = try call(#"{"name":"draft_hud","arguments":{"revision":12}}"#)
        XCTAssertEqual(try text(of: refused), "The layout changed since revision 12. Call get_hud and send the layout again.")
        XCTAssertEqual(refused["result"]["isError"], .bool(true))
        XCTAssertNil(refused.objectValue?["error"], "the model reads a refusal and tries again; it is not a protocol error")

        probe.answer = .unanswered
        let silent = try call(#"{"name":"discard_hud_draft"}"#)
        XCTAssertEqual(try text(of: silent), "Agent HUD did not answer. Try again.")
        XCTAssertEqual(silent["result"]["isError"], .bool(true))
    }
}
