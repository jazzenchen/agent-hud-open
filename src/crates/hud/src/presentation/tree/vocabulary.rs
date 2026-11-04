//! What a panel tree may hold: the controls, the data they read, the formats,
//! the actions and the edit steps, and the catalog get_hud lists them in.
use super::*;

/// The node types of the panel's own parts. They are in every panel.
pub(super) const OWN: &[&str] = &["panel", "blocks", "tokens", "sessions", "quota", "balance"];
pub(super) const CONTROLS: &[&str] = &[
    "row", "column", "grid", "repeat", "text", "logo", "bar", "rings", "segments", "chart",
    "divider", "picker",
];
pub(super) const PARENTS: &[&str] = &["row", "column", "grid", "repeat"];
pub(super) const GAPS: &[&str] = &["none", "small", "medium", "large"];
pub(super) const ALIGNS: &[&str] = &["start", "center", "end", "spread"];
pub(super) const TEXT_ALIGNS: &[&str] = &["start", "center", "end"];
pub(super) const TEXT_STYLES: &[&str] = &["title", "body", "caption", "figure"];
pub(super) const COLORS: &[&str] = &[
    "primary",
    "secondary",
    "tertiary",
    "level",
    "agent",
    "ok",
    "warning",
    "critical",
];
pub(super) const SIZES: &[&str] = &["small", "medium", "large"];
pub(super) const SPANS: &[&str] = &["full", "half"];
pub(super) const LEVELS: &[&str] = &["normal", "warning", "critical", "stale"];
pub(super) const FORMATS: &[&str] = &[
    "percent",
    "number",
    "money",
    "tokens",
    "countdown",
    "relative",
    "clock",
    "date",
    "weekday",
];
pub(super) const PERIODS: &[&str] = &["day", "week", "month"];
pub(super) const STATISTICS_TABS: &[&str] = &["tokens", "sessions"];
pub(super) const SETTINGS_PAGES: &[&str] = &[
    "general",
    "display",
    "agents",
    "agent-plugins",
    "prices",
    "relays",
    "local-network",
    "updates",
];
pub(super) const OPENS: &[&str] = &["statistics", "session", "settings"];
pub(super) const VERBS: &[&str] = &["open", "set", "next", "copy"];
pub(super) const CONDITIONS: &[&str] = &["is", "in", "not", "above", "below", "present"];
pub(super) const EDITS: &[&str] = &["update", "insert", "move", "remove", "replace"];
/// At most this many controls of an agent's own in one panel, this deep.
pub(super) const NODE_LIMIT: usize = 300;
pub(super) const DEPTH_LIMIT: usize = 8;
pub(super) const REPEAT_LIMIT: i64 = 20;
pub(super) const RING_ITEMS: i64 = 4;
pub(super) const TEXT_LIMIT: usize = 200;

/// What a control may read: each item of a collection, by the item's name inside a
/// `repeat` over it, with its fields.
pub(super) struct Collection {
    pub(super) name: &'static str,
    pub(super) item: &'static str,
    pub(super) fields: &'static [(&'static str, &'static str)],
}

pub(super) const COLLECTIONS: &[Collection] = &[
    Collection {
        name: "windows",
        item: "window",
        fields: &[
            ("id", "the window's id"),
            ("agent", "the agent's id, such as claude or codex"),
            ("account", "the account's id"),
            ("accountName", "the account's name"),
            (
                "kind",
                "the same on every computer, such as claude-session or claude-weekly",
            ),
            ("name", "its full name, such as Current session"),
            ("short", "its short name, such as 5h or Weekly"),
            ("used", "the share used, 0 to 1; shown as a percentage"),
            ("level", "normal, warning, critical or stale"),
            (
                "resetsAt",
                "when it resets; shown as the panel's rows show it: the time left within a day, else the weekday and time",
            ),
            ("burn", "the share used per hour"),
            (
                "runsOutAt",
                "when it would run out at this pace, if before the reset",
            ),
        ],
    },
    Collection {
        name: "balances",
        item: "balance",
        fields: &[
            ("id", "the balance's id"),
            ("agent", "the agent's id, such as deepseek"),
            ("account", "the account's id"),
            ("name", "its name"),
            ("amount", "what is left; shown as money"),
            ("currency", "such as USD or CNY"),
            ("level", "normal, warning or critical"),
            ("cost", "the estimated cost today; shown as money"),
            ("runsOutAt", "when it would run out at this pace"),
        ],
    },
    Collection {
        name: "accounts",
        item: "account",
        fields: &[
            ("id", "the account's id"),
            ("agent", "the agent's id"),
            ("name", "the account's name"),
            ("plan", "its plan, such as Pro or Ultra"),
        ],
    },
    Collection {
        name: "agents",
        item: "agent",
        fields: &[
            ("id", "such as claude or codex; a logo's agent"),
            ("name", "such as Claude Code"),
            ("sessions", "how many sessions it has in the list"),
            ("running", "how many of them are running now"),
        ],
    },
    Collection {
        name: "sessions",
        item: "session",
        fields: &[
            (
                "id",
                "the session's id; open it with {\"open\": \"session\"}",
            ),
            ("agent", "the agent's id"),
            ("title", "its title"),
            ("project", "its project folder"),
            ("state", "running, waiting or idle"),
            ("updatedAt", "its last message; shown as a relative time"),
            ("tokens", "its tokens; shown short, such as 1.2M"),
            ("cost", "its estimated cost; shown as money"),
            ("model", "its model's name"),
        ],
    },
];
pub(super) const NOW_FIELDS: &[(&str, &str)] = &[(
    "time",
    "the current time; shown as a clock, or with |date or |weekday",
)];

pub(super) fn collection(name: &str) -> Option<&'static Collection> {
    COLLECTIONS.iter().find(|c| c.name == name)
}

pub(super) fn item(name: &str) -> Option<&'static Collection> {
    COLLECTIONS.iter().find(|c| c.item == name)
}

pub(super) fn field_names(fields: &'static [(&'static str, &'static str)]) -> Vec<&'static str> {
    fields.iter().map(|(name, _)| *name).collect()
}

// MARK: The catalog

/// Everything a tree may hold, as get_hud lists it: the spec an agent builds from.
pub(in crate::presentation) fn catalog() -> Value {
    let described = |fields: &[(&str, &str)]| -> Value {
        Value::Object(
            fields
                .iter()
                .map(|(name, what)| ((*name).to_owned(), json!(what)))
                .collect(),
        )
    };
    let mut data = Map::new();
    for c in COLLECTIONS {
        data.insert(
            c.name.into(),
            json!({"item": c.item, "fields": described(c.fields)}),
        );
    }
    data.insert("now".into(), json!({"fields": described(NOW_FIELDS)}));
    json!({
        "howItWorks": [
            "get_hud's panel is the hover panel as a tree. Each node has an id, a type and its fields; parents have children.",
            "The panel's own parts are the sections blocks, tokens and sessions, and one quota or balance block per agent inside blocks. They cannot be deleted; hide one with hidden: true.",
            "Add your own controls from controls below: as a section of the panel (inside panel), or as a block among the agents' blocks (inside blocks).",
            "Change the panel with edit, a list of steps that each name a node by its id. Or send the whole tree back as panel.",
            "Text shows data with placeholders such as {window.used}. Inside a repeat over windows, each item is window; now and state can be read anywhere.",
            "Nothing is saved until the person presses Save in the panel."
        ],
        "own": {
            "panel": {"fields": {"spacing": "compact, regular or roomy", "separators": "true or false: lines between parts", "width": "480 to 720 points", "countdown": "true or false: the reset countdowns"}, "children": "the sections blocks, tokens and sessions, and your own controls"},
            "blocks": {"children": "one quota or balance block per agent, and your own controls; span: half puts two on a line"},
            "quota": {"fields": {"widget": "bar, rings, segments, number or chart", "measures": "what the picker offers, the first shown: quota, burn, tokens, consumed, cost", "rows": "the order of its row ids", "hiddenRows": "row ids to hide", "accounts": "show only these account ids", "hiddenAccounts": "account ids to hide", "countdown": "true or false", "columns": "2 to 6, for number", "ringSize": "small, medium or large, for rings and segments", "span": "full or half (not for bar)", "period": "day, week or month, for chart", "hidden": "true or false"}},
            "balance": {"fields": {"widget": "number or chart", "measures": "quota, burn, cost", "period": "day, week or month", "span": "full or half", "hidden": "true or false"}},
            "tokens": {"fields": {"period": "day, week or month", "agents": "agent names from get_hud's tokens.offeredAgents", "models": "model ids from tokens.offeredModels", "hiddenModels": "model ids to hide", "modelOrder": "model ids in order", "hidden": "true or false"}},
            "sessions": {"fields": {"limit": "1 to 10", "period": "day or week", "agents": "agent names from sessions.offeredAgents", "projects": "project ids from sessions.offeredProjects", "hidden": "true or false"}}
        },
        "controls": {
            "row": {"fields": {"gap": "none, small, medium or large", "align": "where the children sit along it: start (the default), center, end, or spread to both ends"}, "children": "side by side, centred top to bottom; a bar takes the room left"},
            "column": {"fields": {"gap": "none, small, medium or large", "align": "start, center or end; left out, the children take its whole width"}, "children": "one under another"},
            "grid": {"fields": {"columns": "2 to 6", "gap": "none, small, medium or large"}, "children": "in cells, left to right"},
            "repeat": {"fields": {"over": "windows, balances, accounts, agents or sessions", "where": "only items whose fields have these values, such as {\"agent\": \"claude\"}; a list allows several", "sort": "a field to sort by; -used sorts largest first", "limit": "1 to 20"}, "children": "drawn once for each item, in the repeat's parent"},
            "text": {"fields": {"text": "words and placeholders, up to 200 characters", "style": "title, body, caption or figure (a large number)", "color": "primary, secondary, tertiary, level (the item's level), agent (its agent's colour), ok, warning or critical", "align": "start, center or end"}},
            "logo": {"fields": {"agent": "an agent's id, or {window.agent}", "size": "small, medium or large"}},
            "bar": {"fields": {"value": "0 to 1, or a placeholder such as {window.used}", "level": "normal, warning, critical, or {window.level}; sets the colour"}},
            "rings": {"fields": {"items": "which windows: {\"over\": \"windows\", \"where\": {...}, \"sort\": ..., \"limit\": up to 4}; one ring each, the first outermost", "size": "small, medium or large", "legend": "true or false: the windows' names and percentages beside it"}},
            "segments": {"fields": {"items": "as for rings; one segment each, the first on top", "size": "small, medium or large", "legend": "true or false"}},
            "chart": {"fields": {"period": "day, week or month", "agent": "an agent's id, or all agents when left out"}, "draws": "the tokens over the period, by model"},
            "divider": {"fields": {}},
            "picker": {"fields": {"state": "the name of a state its block declares", "options": "2 to 6 of {\"value\": ..., \"text\": ...}; the value shown is the state's"}}
        },
        "everyControl": {
            "id": "letters, digits, - and _; required on a control placed in panel or blocks, given for the others when left out",
            "when": "show it only when a condition holds: {\"value\": \"{window.level}\", \"in\": [\"warning\", \"critical\"]}; one of is, in, not, above, below or present",
            "onClick": "what a click does; see actions",
            "hidden": "true or false",
            "span": "full or half; for a control placed in blocks",
            "state": "for a control placed in panel or blocks: {\"name\": {\"options\": [...], \"default\": ...}}, read as {state.name} by everything inside it"
        },
        "data": data,
        "placeholders": "Write {item.field} in text, or {item.field|format} to pick a format: percent, number, money, tokens, countdown, relative, clock, date or weekday. A field has a format of its own when none is picked.",
        "actions": {
            "open": "{\"open\": \"statistics\", \"tab\": \"tokens\" or \"sessions\", \"agent\": optional, \"session\": optional}, {\"open\": \"session\", \"session\": \"{session.id}\"}, {\"open\": \"settings\", \"page\": general, display, agents, agent-plugins, prices, relays, local-network or updates}",
            "set": "{\"set\": \"tab\", \"to\": \"week\"}: sets a state",
            "next": "{\"next\": \"tab\"}: the state's next option",
            "copy": "{\"copy\": \"{session.title}\"}: copies the text"
        },
        "edits": {
            "update": "{\"update\": \"claude\", \"widget\": \"rings\"}: sets fields of a node; null clears one",
            "insert": "{\"insert\": {node}, \"after\": \"claude\"}; or \"before\": id, or \"into\": id with an optional \"at\": index",
            "move": "{\"move\": \"codex\", \"before\": \"claude\"}; or \"after\" or \"into\"",
            "remove": "{\"remove\": \"clock\"}; the panel's own parts are hidden instead",
            "replace": "{\"replace\": \"clock\", \"with\": {node}}: one of your own controls"
        },
        "examples": [
            {"ask": "Show Claude's windows as rings", "edit": [{"update": "claude", "widget": "rings"}]},
            {"ask": "Put a clock above the agents", "edit": [{"insert": {"id": "clock", "type": "text", "text": "{now.time|clock} · {now.time|weekday}", "style": "title"}, "into": "blocks", "at": 0}]},
            {"ask": "List the windows over 80%", "edit": [{"insert": {"id": "tight", "type": "column", "gap": "small", "children": [{"type": "repeat", "over": "windows", "where": {"level": ["warning", "critical"]}, "sort": "-used", "limit": 5, "children": [{"type": "row", "align": "spread", "children": [{"type": "text", "text": "{window.agent} {window.short}"}, {"type": "text", "text": "{window.used}", "color": "level"}]}]}]}, "before": "tokens"}]}
        ],
        "limits": {"controls": NODE_LIMIT, "depth": DEPTH_LIMIT, "repeat": REPEAT_LIMIT, "text": TEXT_LIMIT}
    })
}
