//! The MCP tools' drafts as agent-hud-native-open's HUDTools tests tell them: a proposal is
//! kept as what it changes, a screen's proposal of the shared layout uses it, a
//! proposal the panel cannot draw as asked is refused whole and changes nothing, and
//! `from` is a whole proposal of a version.
use super::Presentation;
use serde_json::{json, Value};

fn snapshot() -> Value {
    json!({
        "generatedAt": "2026-10-21T10:00:00Z",
        "settings": {"appearance": "dark", "appearanceFollowsScreen": false, "showMenuBarIcon": true, "language": "en"},
        "providers": [{"id": "claude", "name": "Claude"}, {"id": "codex", "name": "Codex"}],
        "quotas": [
            {"id": "acct1:claude-session", "vendor": "claude", "account": "a@example.com", "accountId": "acct1", "legacyWindowId": "acct1/claude-session", "usedPercent": 10, "status": "normal"},
            {"id": "acct1:claude-weekly", "vendor": "claude", "account": "a@example.com", "accountId": "acct1", "legacyWindowId": "acct1/claude-weekly", "usedPercent": 20, "status": "normal"},
            {"id": "c1:codex-5h", "vendor": "codex", "account": "b@example.com", "accountId": "c1", "usedPercent": 5, "status": "normal"}
        ],
        "billing": [{"id": "ds1", "vendor": "deepseek", "label": "DeepSeek key", "amount": 10, "currency": "CNY"}],
        "sessions": [
            {"id": "s1", "source": "claude", "updatedAt": "2026-10-21T09:59:00Z", "cwd": "~/agent-hud-test/project-a", "project": "~/agent-hud-test/project-a"},
            {"id": "s2", "source": "claude", "updatedAt": "2026-10-21T09:58:00Z"}
        ]
    })
}

fn catalog() -> Value {
    json!({"available": {
        "providers": [{"id": "claude", "name": "Claude"}],
        "models": [{"id": "claude-model:opus", "provider": "claude", "name": "opus"}]
    }})
}

/// An owner with a notched main display and a second display without a notch.
fn owner() -> (tempfile::TempDir, Presentation) {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
    owner.screens(vec![
        json!({"id": "main", "hasNotch": true, "primary": true}),
        json!({"id": "side", "hasNotch": false, "primary": false}),
    ]);
    (folder, owner)
}

fn call(owner: &Presentation, name: &str, params: Value) -> Result<Value, String> {
    owner.tool(name, params, &snapshot(), Some(&catalog()))
}

fn draft(owner: &Presentation, params: Value) -> Value {
    call(owner, "draft_hud", params).unwrap_or_else(|e| panic!("refused: {e}"))
}

fn refused(owner: &Presentation, params: Value) -> String {
    let before = owner.view();
    let error = call(owner, "draft_hud", params).expect_err("should be refused");
    assert!(error.starts_with("Nothing was changed. "), "{error}");
    let after = owner.view();
    assert_eq!(
        before["drafts"], after["drafts"],
        "a refusal changed a draft"
    );
    assert_eq!(
        before["revision"], after["revision"],
        "a refusal changed the revision"
    );
    error
}

fn drafts(owner: &Presentation) -> Value {
    owner.view()["drafts"].clone()
}

// MARK: Goal 1: what a proposal changes

#[test]
fn a_proposal_that_spells_out_the_default_leaves_no_draft() {
    let (_folder, owner) = owner();
    let revision = owner.view()["revision"].clone();
    let result = draft(
        &owner,
        json!({"screen": "all", "order": ["claude", "codex", "api:deepseek"], "countdown": true,
               "sections": ["blocks"], "style": {"spacing": "regular", "separators": true, "width": 540},
               "blocks": {"claude": {"widget": "bar", "measures": ["quota", "burn", "tokens", "consumed", "cost"], "span": "full"},
                          "api:deepseek": {"widget": "number", "period": "day"}},
               "sessions": {"limit": 5, "period": "day"}, "tokens": {"period": "day"}}),
    );
    assert_eq!(drafts(&owner)["layouts"], json!([]));
    assert_eq!(
        result["note"],
        "This is the layout already saved, so there is no draft."
    );
    assert_eq!(owner.view()["revision"], revision, "a no-op is no change");
}

#[test]
fn sessions_take_no_state_filter_or_sort_and_a_saved_one_reads_without_them() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"screen": "all", "sessions": {"states": ["running"], "sort": "active"}}),
    );
    assert!(error.contains("sessions.states"), "{error}");
    assert!(error.contains("sessions.sort"), "{error}");
    owner
        .set_layout(json!({"screen": "all", "layout": {"sessions": {"limit": 8, "states": ["waiting", "running"], "sort": "date"}}}))
        .unwrap();
    let read = call(&owner, "get_hud", json!({"screen": "all"})).unwrap();
    assert_eq!(read["layout"]["sessions"], json!({"limit": 8}));
}

#[test]
fn model_visibility_and_order_survive_layout_storage_and_tool_round_trip() {
    let (_folder, owner) = owner();
    let tokens = json!({"hiddenModels": ["codex-model:old"], "modelOrder": ["codex-model:new", "codex-model:old"]});
    owner
        .set_layout(json!({"screen":"all", "layout":{"tokens": tokens}}))
        .unwrap();
    let read = call(&owner, "get_hud", json!({"screen":"all"})).unwrap();
    assert_eq!(read["layout"]["tokens"], tokens);
    draft(&owner, json!({"screen":"all", "tokens":tokens}));
    assert_eq!(drafts(&owner)["layouts"], json!([]));
}

#[test]
fn the_layout_get_hud_returned_sent_back_changes_nothing() {
    let (_folder, owner) = owner();
    owner
        .set_layout(json!({"screen": "all", "layout": {"blocks": {"claude": {"widget": "ring", "rows": ["acct1/claude-weekly"]}}, "order": ["codex"]}}))
        .unwrap();
    let read = call(&owner, "get_hud", json!({"screen": "all"})).unwrap();
    let mut proposal = read["layout"].as_object().unwrap().clone();
    proposal.insert("screen".into(), json!("all"));
    proposal.insert("revision".into(), read["revision"].clone());
    draft(&owner, Value::Object(proposal));
    assert_eq!(drafts(&owner)["layouts"], json!([]));
    // A real change is a draft; the saved layout sent back again clears it.
    draft(
        &owner,
        json!({"screen": "all", "blocks": {"claude": {"widget": "number"}}}),
    );
    assert_eq!(drafts(&owner)["layouts"], json!(["all"]));
    draft(
        &owner,
        json!({"screen": "all", "order": ["codex", "claude"], "blocks": {"claude": {"widget": "ring", "rows": ["acct1/claude-weekly", "acct1/claude-session"], "ringSize": "medium"}}}),
    );
    assert_eq!(drafts(&owner)["layouts"], json!([]));
}

#[test]
fn a_screen_proposal_of_the_shared_layout_uses_it() {
    let (_folder, owner) = owner();
    owner
        .set_layout(json!({"screen": "all", "layout": {"blocks": {"codex": {"hidden": true}}}}))
        .unwrap();
    // The screen uses the shared layout: proposing what it draws is no draft.
    let result = draft(
        &owner,
        json!({"screen": "side", "blocks": {"codex": {"hidden": true, "widget": "bar"}}}),
    );
    assert_eq!(drafts(&owner)["layouts"], json!([]));
    assert_eq!(
        result["note"],
        "This is what \"Display 2\" already has saved, so there is no draft for it."
    );
    // A screen with its own layout keeps it: the shared layout's content proposed is
    // an own layout, as in Open; while it is proposed to use the shared layout, the
    // shared content keeps that, and after Save the screen follows the shared layout.
    owner
        .set_layout(
            json!({"screen": "side", "layout": {"blocks": {"claude": {"widget": "number"}}}}),
        )
        .unwrap();
    draft(
        &owner,
        json!({"screen": "side", "blocks": {"codex": {"hidden": true}}}),
    );
    assert_eq!(
        owner.view()["layouts"]["side"],
        json!({"blocks": {"codex": {"hidden": true}}})
    );
    draft(&owner, json!({"screen": "side", "from": "shared"}));
    draft(
        &owner,
        json!({"screen": "side", "blocks": {"codex": {"hidden": true}}}),
    );
    assert_eq!(
        owner.view()["layouts"].get("side"),
        None,
        "the draft uses the shared layout"
    );
    assert_eq!(drafts(&owner)["layouts"], json!(["side"]));
    owner.save_panel_with("side", |_| Ok(())).unwrap();
    assert_eq!(owner.view()["layouts"].get("side"), None);
    assert_eq!(drafts(&owner)["layouts"], json!([]));
    // Its own saved layout proposed is its own, no draft.
    owner
        .set_layout(
            json!({"screen": "side", "layout": {"blocks": {"claude": {"widget": "number"}}}}),
        )
        .unwrap();
    draft(
        &owner,
        json!({"screen": "side", "blocks": {"claude": {"widget": "number", "columns": 4}}}),
    );
    assert_eq!(drafts(&owner)["layouts"], json!([]));
}

#[test]
fn a_style_is_kept_as_what_it_changes() {
    let (_folder, owner) = owner();
    let read = call(&owner, "get_hud", json!({"screen": "main"})).unwrap();
    let hud = read["screens"][0]["hud"].clone();
    assert_eq!(hud["mode"], "notch");
    assert_eq!(
        read["screens"][1]["hud"]["mode"], "dynamic",
        "a screen without a notch draws a dock"
    );
    // The listed style sent back whole is no draft, logo fields on a notch HUD included.
    draft(&owner, json!({"screen": "main", "hud": hud}));
    assert_eq!(drafts(&owner)["styles"], json!([]));
    // A change keeps only what it changes; reset on a default screen is no draft.
    draft(
        &owner,
        json!({"screen": "main", "hud": {"appearance": "light", "edge": "top"}}),
    );
    assert_eq!(
        owner.view()["store"]["screens"]["main"],
        json!({"appearance": "light"})
    );
    draft(&owner, json!({"screen": "side", "hud": {"reset": true}}));
    assert_eq!(drafts(&owner)["styles"], json!(["main"]));
    // A proposal is whole: a layout alone drops the style draft it does not bring.
    draft(
        &owner,
        json!({"screen": "main", "blocks": {"codex": {"hidden": true}}}),
    );
    assert_eq!(drafts(&owner)["styles"], json!([]));
}

#[test]
fn a_dock_leaving_the_notch_is_placed_clear_of_it() {
    let (_folder, owner) = owner();
    let result = draft(
        &owner,
        json!({"screen": "main", "hud": {"mode": "dynamic"}}),
    );
    assert_eq!(
        owner.view()["store"]["screens"]["main"],
        json!({"mode": "logos", "offset": 0})
    );
    assert!(result["note"]
        .as_str()
        .unwrap()
        .contains("clear of the notch"));
}

#[test]
fn the_app_is_kept_as_what_it_changes() {
    let (_folder, owner) = owner();
    let read = call(&owner, "get_hud", json!({})).unwrap();
    let result = draft(&owner, json!({"app": read["app"].clone()}));
    assert_eq!(drafts(&owner)["app"], false);
    assert!(result["note"]
        .as_str()
        .unwrap()
        .starts_with("The app's own settings are as they are saved"));
    draft(
        &owner,
        json!({"app": {"botLogos": true, "hover": {"delayMs": 400, "collapseDelayMs": 300}}}),
    );
    assert_eq!(
        owner.view()["appDraft"],
        json!({"hover": {"collapseDelayMs": 300}})
    );
    // The app alone leaves the layouts and styles.
    assert_eq!(drafts(&owner)["layouts"], json!([]));
}

// MARK: Goal 2: refusals

#[test]
fn balance_rows_are_accepted_for_visibility_and_order() {
    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"blocks": {"api:deepseek": {"hiddenRows": ["ds1"], "rows": ["ds1"]}}}),
    );
    let error = refused(
        &owner,
        json!({"blocks": {"api:deepseek": {"hiddenRows": ["unknown"]}}}),
    );
    assert!(error.contains("has no row \"unknown\""), "{error}");
}

#[test]
fn unknown_keys_are_refused_with_what_the_panel_has() {
    let (_folder, owner) = owner();
    let error = refused(&owner, json!({"blocks": {"gemini": {"hidden": true}}}));
    assert!(error.contains("blocks.gemini: No block has the key \"gemini\". The panel's blocks now: \"claude\", \"codex\", \"api:deepseek\"."), "{error}");
    let error = refused(&owner, json!({"order": ["claude", "gemini"]}));
    assert!(
        error.contains("order[1]: No block has the key \"gemini\"."),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"blocks": {"claude": {"hiddenRows": ["nope"], "accounts": ["acct9"]}}}),
    );
    assert!(error.contains("blocks.claude.hiddenRows[0]: \"claude\" has no row \"nope\". Its rows: \"acct1/claude-session\", \"acct1/claude-weekly\"."), "{error}");
    assert!(error.contains("blocks.claude.accounts[0]: \"claude\" has no account \"acct9\". Its accounts: \"acct1\" (a@example.com)."), "{error}");
    // A row named by its window key alone is one the panel matches.
    draft(
        &owner,
        json!({"blocks": {"claude": {"hiddenRows": ["claude-weekly"]}}}),
    );
}

#[test]
fn widgets_a_kind_rejects_and_half_bar_rows_are_refused() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"blocks": {"api:deepseek": {"widget": "bar"}, "claude": {"span": "half"}}}),
    );
    assert!(error.contains("blocks.api:deepseek.widget: \"api:deepseek\" is an API balance, which has no ceiling, so the \"bar\" widget has nothing to measure it against. It accepts: \"number\", \"chart\"."), "{error}");
    assert!(
        error.contains("blocks.claude.span: Bar rows need the full width of the panel"),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"blocks": {"claude": {"widget": "chart", "measures": ["quota"], "columns": 3}}}),
    );
    assert!(error.contains("blocks.claude.measures[0]: The \"chart\" widget of \"claude\" does not offer the measure \"quota\". It offers: \"tokens\"."), "{error}");
    assert!(
        error.contains("blocks.claude.columns: The \"chart\" widget of \"claude\" draws no cells."),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"blocks": {"api:deepseek": {"countdown": false}}}),
    );
    assert!(
        error.contains("which has no reset to count down to"),
        "{error}"
    );
}

#[test]
fn absent_agents_and_models_are_refused() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"sessions": {"agents": ["codex"]}, "tokens": {"agents": ["codex"], "models": ["gpt-model:5"]}}),
    );
    assert!(error.contains("sessions.agents[0]: \"codex\" has no session now. Agents with sessions now: \"claude\"."), "{error}");
    assert!(error.contains("tokens.agents[0]: \"codex\" has no model in the token chart now. Agents in the chart now: \"claude\"."), "{error}");
    assert!(error.contains("tokens.models[0]: \"gpt-model:5\" is no model in the token chart now. Models in the chart now: \"claude-model:opus\"."), "{error}");
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"agents": ["deepseek", "gemini"]}}),
    );
    assert!(error.contains("hud.agents[1]: \"gemini\" is not an agent the resting HUD of \"Built-in display\" can show now. It can show: \"claude\", \"codex\", \"deepseek\"."), "{error}");
    draft(
        &owner,
        json!({"sessions": {"agents": ["Claude"]}, "tokens": {"agents": ["claude"], "models": ["claude-model:opus"]}}),
    );
}

#[test]
fn style_fields_the_mode_or_glow_does_not_draw_are_refused() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"logoSize": 30, "glow": {"gridPitch": 6}}}),
    );
    assert!(error.contains("hud.logoSize: logoSize sets a \"dynamic\" HUD, and the HUD of \"Built-in display\" would be \"notch\". Leave it out, or set mode to \"dynamic\"."), "{error}");
    assert!(error.contains("hud.glow.gridPitch: gridPitch sets a grid glow style, one of \"dots\", \"ascii\", \"blocks\", \"braille\", \"binary\", and the glow of \"Built-in display\" would be \"blur\"."), "{error}");
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"glow": {"style": "dots", "gridPitch": 7.3, "gridDensity": 0.53, "idleSpeed": 2.2}}}),
    );
    assert!(error.contains("hud.glow.gridPitch: gridPitch is a whole number from 4 to 12, as Settings sets it, which 7.3 is not."), "{error}");
    assert!(error.contains("hud.glow.gridDensity: gridDensity is a number from 0.5 to 1.5 in steps of 0.05, as Settings sets it, which 0.53 is not."), "{error}");
    assert!(error.contains("hud.glow.idleSpeed: idleSpeed is a number from 1 to 24 in steps of 0.5, as Settings sets it, which 2.2 is not."), "{error}");
    let error = refused(&owner, json!({"screen": "side", "hud": {"mode": "notch"}}));
    assert!(error.contains("hud.mode: \"Display 2\" has no notch, so its HUD cannot be \"notch\". It can be \"dynamic\" or \"simple\"."), "{error}");
    let error = refused(
        &owner,
        json!({"screen": "side", "hud": {"mode": "simple", "glow": {"brightness": 0.5}}}),
    );
    assert!(error.contains("hud.glow.brightness: A \"simple\" HUD draws no glow, and the HUD of \"Display 2\" would be \"simple\"."), "{error}");
    let error = refused(
        &owner,
        json!({"screen": "all", "hud": {"appearance": "light"}}),
    );
    assert!(error.contains("hud.appearance sets the resting HUD of one screen, so the proposal needs that screen's key"), "{error}");
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"reset": true, "appearance": "light"}}),
    );
    assert!(
        error.contains(
            "hud.reset proposes the default style of the resting HUD of \"Built-in display\" whole"
        ),
        "{error}"
    );
    draft(
        &owner,
        json!({"screen": "main", "hud": {"glow": {"style": "dots", "gridPitch": 7, "gridDensity": 0.55, "idleSpeed": 2.5}}}),
    );
}

#[test]
fn misreadings_are_refused_with_open_sentences() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"style": {"appearance": "light", "width": 900}, "blocks": {"claude": {"colour": "red"}}, "wat": 1}),
    );
    assert_eq!(error, "Nothing was changed. Unknown: \"wat\". It takes: \"blocks\", \"order\", \"sections\", \"hiddenSections\", \"countdown\", \"style\", \"hud\", \"sessions\", \"tokens\", \"app\", \"from\", \"revision\", \"screen\".");
    let error = refused(
        &owner,
        json!({"style": {"appearance": "light", "width": 900}, "blocks": {"claude": {"colour": "red"}}}),
    );
    assert!(error.contains("style has no property \"appearance\"; the style takes spacing, separators and width. The HUD's appearance is hud.appearance, for one screen."), "{error}");
    assert!(
        error.contains("style.width is a whole number from 480 to 720."),
        "{error}"
    );
    assert!(
        error.contains("blocks.claude has no property \"colour\""),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"logoSize": 90, "glow": {"hue": 3}}}),
    );
    assert!(
        error.contains("hud.logoSize is a number from 20 to 60."),
        "{error}"
    );
    assert!(
        error.contains("hud.glow has no property \"hue\""),
        "{error}"
    );
    let error = refused(&owner, json!({"revision": "x"}));
    assert_eq!(
        error,
        "Nothing was changed. revision is the whole number get_hud returned."
    );
    // Null is not given.
    draft(
        &owner,
        json!({"screen": "main", "hud": {"logoSize": null}, "order": null}),
    );
}

#[test]
fn app_delays_and_appearance_are_checked_as_settings_sets_them() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"app": {"hover": {"delayMs": 1550, "collapseDelayMs": 225}}}),
    );
    assert!(error.contains("app.hover.delayMs: delayMs is a whole number of milliseconds from 0 to 1500 in steps of 50, as Settings sets it, which 1550 is not."), "{error}");
    assert!(error.contains("app.hover.collapseDelayMs: collapseDelayMs is a whole number of milliseconds from 0 to 1500 in steps of 50, as Settings sets it, which 225 is not."), "{error}");
    let error = refused(
        &owner,
        json!({"app": {"appearance": "light", "matchScreen": true}}),
    );
    assert!(error.contains("app.appearance: appearance sets the app's windows and the menu-bar menu, and with matchScreen on, as it would be"), "{error}");
    let error = refused(&owner, json!({"app": {"language": "fr", "delayMs": 100}}));
    assert!(error.contains("app.language is read only: it is the Language chosen in Settings, which only Settings changes, \"en\" as get_hud lists it."), "{error}");
    assert!(
        error.contains("Hover behavior's delayMs is app.hover.delayMs."),
        "{error}"
    );
}

#[test]
fn projects_are_named_by_id_or_by_a_path_inside_one() {
    let (_folder, owner) = owner();
    let read = call(&owner, "get_hud", json!({})).unwrap();
    assert_eq!(
        read["sessions"]["offeredProjects"],
        json!([{"id": "~/agent-hud-test/project-a", "name": "project-a"}, {"id": "unassigned", "name": "Unassigned"}])
    );
    let error = refused(
        &owner,
        json!({"sessions": {"projects": ["project-a", "~/elsewhere"]}}),
    );
    assert!(error.contains("sessions.projects[0]: \"project-a\" is a project's name, not its id. Name it by its id: \"~/agent-hud-test/project-a\"."), "{error}");
    assert!(error.contains("sessions.projects[1]: \"~/elsewhere\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. Projects with sessions now: \"~/agent-hud-test/project-a\", \"unassigned\"."), "{error}");
    // A path inside a project names it, and the layout lists it by its id.
    let result = draft(
        &owner,
        json!({"sessions": {"projects": ["~/agent-hud-test/project-a/src", "unassigned"]}}),
    );
    assert_eq!(
        owner.view()["sharedLayout"]["sessions"]["projects"],
        json!(["unassigned", "~/agent-hud-test/project-a"])
    );
    assert!(result["note"].as_str().unwrap().contains("\"~/agent-hud-test/project-a/src\" names the project \"~/agent-hud-test/project-a\", which sessions.projects lists by that id."));
}

// MARK: Goal 3: from

#[test]
fn from_is_a_whole_proposal_of_a_version() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"screen": "main", "from": "default", "hud": {"agents": ["claude"]}}),
    );
    assert_eq!(
        error,
        "Nothing was changed. Pass either from or a layout, not both."
    );
    let error = refused(&owner, json!({"from": "shared"}));
    assert!(error.contains("from \"shared\" is for a screen"), "{error}");
    let error = refused(&owner, json!({"from": "latest"}));
    assert_eq!(
        error,
        "Nothing was changed. from is \"default\", \"previous\" or \"shared\"."
    );
    let error = refused(&owner, json!({"screen": "main", "from": "previous"}));
    assert_eq!(
        error,
        "Nothing was changed. \"Built-in display\" has no previous saved layout."
    );
    // from default drops a style draft it does not bring; for a screen that uses the
    // shared layout it is a layout of its own.
    draft(
        &owner,
        json!({"screen": "main", "hud": {"appearance": "light"}}),
    );
    assert_eq!(drafts(&owner)["styles"], json!(["main"]));
    draft(&owner, json!({"screen": "main", "from": "default"}));
    assert_eq!(drafts(&owner)["styles"], json!([]));
    assert_eq!(owner.view()["layouts"]["main"], json!({}));
    // from shared for a screen that keeps its own layout uses the shared one.
    owner
        .set_layout(
            json!({"screen": "side", "layout": {"blocks": {"claude": {"widget": "number"}}}}),
        )
        .unwrap();
    draft(&owner, json!({"screen": "side", "from": "shared"}));
    assert_eq!(owner.view()["layouts"].get("side"), None);
    owner.save_panel_with("side", |_| Ok(())).unwrap();
    // from previous brings the version saved before back as a draft.
    draft(&owner, json!({"screen": "side", "from": "previous"}));
    assert_eq!(
        owner.view()["layouts"]["side"],
        json!({"blocks": {"claude": {"widget": "number"}}})
    );
}

#[test]
fn detected_clients_without_readings_have_configurable_empty_blocks() {
    let (_folder, owner) = owner();
    let snapshot = json!({
        "providers": [
            {"id":"codex", "enabled":true, "detected":true},
            {"id":"deepseek", "enabled":true, "detected":true},
            {"id":"kimi", "enabled":false, "detected":true},
            {"id":"cursor", "enabled":true, "detected":false}
        ],
        "quotas": [],
        "sessions": [],
        "billing": [{"id":"balance", "vendor":"deepseek", "amount":8.85, "currency":"CNY"}]
    });
    let read = owner
        .tool("get_hud", json!({"screen":"main"}), &snapshot, None)
        .unwrap();
    let blocks = read["blocks"].as_array().unwrap();
    assert_eq!(
        blocks
            .iter()
            .map(|block| block["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["codex", "api:deepseek"]
    );
    assert_eq!(blocks[0]["rows"], json!([]));
    assert_eq!(blocks[0]["accounts"], json!([]));
    let proposed = owner.tool(
        "draft_hud",
        json!({"screen":"main", "hud":{"agents":["codex"]}, "blocks":{"codex":{"hidden":true}}}),
        &snapshot,
        None,
    ).unwrap();
    assert_eq!(proposed["layout"]["blocks"]["codex"]["hidden"], true);
    assert_eq!(proposed["blocks"][0]["hidden"], true);
}
