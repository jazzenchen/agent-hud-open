//! The MCP tools' drafts as agent-hud-native-open's HUDTools tests tell them: a proposal is
//! kept as what it changes, a screen's proposal of the shared layout uses it, a
//! proposal the panel cannot draw as asked is refused whole and changes nothing, and
//! `from` is a whole proposal of a version.
use super::rules::strings;
use super::{panel_keys, rules, Presentation};
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

#[test]
fn status_bar_selection_and_compact_mode_persist_without_changing_animation_preferences() {
    let (folder, owner) = owner();
    let before = owner.view()["store"]["global"].clone();
    owner
        .update(json!({"screen":"side", "patch":{
            "compactDock":true, "dockScale":0.8, "dockHeight":1.25, "dockRoundness":0.4,
            "panelLayout":{"hud":{"agents":["codex","deepseek"]}}
        }}))
        .unwrap();
    let reopened = Presentation::new(folder.path().to_path_buf()).unwrap();
    let stored = reopened.view();
    assert_eq!(stored["store"]["screens"]["side"]["compactDock"], true);
    assert_eq!(stored["store"]["screens"]["side"]["dockScale"], 0.8);
    assert_eq!(stored["store"]["screens"]["side"]["dockHeight"], 1.25);
    assert_eq!(stored["store"]["screens"]["side"]["dockRoundness"], 0.4);
    assert_eq!(
        stored["layouts"]["side"]["hud"]["agents"],
        json!(["codex", "deepseek"])
    );
    assert_eq!(stored["store"]["global"], before);
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
               "blocks": {"claude": {"widget": "bar", "span": "full"},
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
        json!({"screen": "all", "order": ["codex", "claude"], "blocks": {"claude": {"widget": "rings", "rows": ["acct1/claude-weekly", "acct1/claude-session"], "ringSize": "medium"}}}),
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
        read["screens"][1]["hud"]["mode"], "simple",
        "a screen without a notch draws a Simple dock"
    );
    // The listed style sent back whole is no draft, Simple fields on a notch HUD included.
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
    let result = draft(&owner, json!({"screen": "main", "hud": {"mode": "simple"}}));
    assert_eq!(
        owner.view()["store"]["screens"]["main"],
        json!({"mode": "simple", "offset": 0})
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
    // A layout no longer chooses measures: every block offers all of them.
    assert!(
        error.contains("blocks.claude has no property \"measures\""),
        "{error}"
    );
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
        json!({"screen": "main", "hud": {"simpleSize": 30, "glow": {"gridPitch": 6}}}),
    );
    assert!(error.contains("hud.simpleSize: simpleSize sets a \"simple\" HUD, and the HUD of \"Built-in display\" would be \"notch\". Leave it out, or set mode to \"simple\"."), "{error}");
    assert!(error.contains("hud.glow.gridPitch: gridPitch sets a grid glow style, one of \"dots\", \"ascii\", \"blocks\", \"braille\", \"binary\", and the glow of \"Built-in display\" would be \"blur\"."), "{error}");
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"glow": {"style": "dots", "gridPitch": 7.3, "gridDensity": 0.53, "idleSpeed": 2.2}}}),
    );
    assert!(error.contains("hud.glow.gridPitch: gridPitch is a whole number from 4 to 12, as Settings sets it, which 7.3 is not."), "{error}");
    assert!(error.contains("hud.glow.gridDensity: gridDensity is a number from 0.5 to 1.5 in steps of 0.05, as Settings sets it, which 0.53 is not."), "{error}");
    assert!(error.contains("hud.glow.idleSpeed: idleSpeed is a number from 1 to 24 in steps of 0.5, as Settings sets it, which 2.2 is not."), "{error}");
    let error = refused(&owner, json!({"screen": "side", "hud": {"mode": "notch"}}));
    assert!(error.contains("hud.mode: \"Display 2\" has no notch, so its HUD cannot be \"notch\". It can only be \"simple\"."), "{error}");
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
        json!({"style": {"appearance": "light", "width": 1000}, "blocks": {"claude": {"colour": "red"}}, "wat": 1}),
    );
    assert_eq!(error, "Nothing was changed. Unknown: \"wat\". It takes: \"blocks\", \"order\", \"sections\", \"hiddenSections\", \"countdown\", \"style\", \"hud\", \"sessions\", \"tokens\", \"quotaHistory\", \"planValue\", \"panel\", \"edit\", \"app\", \"from\", \"revision\", \"screen\".");
    let error = refused(
        &owner,
        json!({"style": {"appearance": "light", "width": 1000}, "blocks": {"claude": {"colour": "red"}}}),
    );
    assert!(error.contains("style has no property \"appearance\"; the style takes spacing, separators and width. The HUD's appearance is hud.appearance, for one screen."), "{error}");
    assert!(
        error.contains("style.width is a whole number from 480 to 960."),
        "{error}"
    );
    assert!(
        error.contains("blocks.claude has no property \"colour\""),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"screen": "main", "hud": {"simpleSize": 90, "glow": {"hue": 3}}}),
    );
    assert!(
        error.contains("hud.simpleSize is a number from 20 to 60."),
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
        json!({"screen": "main", "hud": {"simpleSize": null}, "order": null}),
    );
}

#[test]
fn the_retired_dynamic_mode_is_refused_and_its_saved_screens_draw_simple() {
    let (_folder, owner) = owner();
    for mode in ["dynamic", "logos"] {
        let error = refused(&owner, json!({"screen": "side", "hud": {"mode": mode}}));
        assert!(error.contains("hud.mode: Dynamic mode was retired. It can be \"notch\" (only on a display with a notch) or \"simple\"."), "{error}");
    }
    let error = refused(
        &owner,
        json!({"screen": "side", "hud": {"logoSize": 30, "showLogos": false}}),
    );
    assert!(
        error.contains("hud.logoSize set the Dynamic mode, which was retired; leave it out."),
        "{error}"
    );
    assert!(
        error.contains("hud.showLogos set the Dynamic mode, which was retired; leave it out."),
        "{error}"
    );
    // A screen saved as Dynamic reads, and is drafted against, as Simple.
    owner
        .update(
            json!({"screen": "side", "patch": {"mode": "logos", "edge": "left", "offset": 0.3}}),
        )
        .unwrap();
    let read = call(&owner, "get_hud", json!({})).unwrap();
    assert_eq!(read["screens"][1]["hud"]["mode"], "simple");
    assert_eq!(read["screens"][1]["hud"]["edge"], "left");
    assert_eq!(read["screens"][1]["hud"]["position"], 0.3);
    assert!(read["screens"][1]["hud"].get("logoSize").is_none());
    draft(
        &owner,
        json!({"screen": "side", "hud": read["screens"][1]["hud"].clone()}),
    );
    assert_eq!(drafts(&owner)["styles"], json!([]));
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

// MARK: The panel as a tree

fn layout(owner: &Presentation) -> Value {
    call(owner, "get_hud", json!({"screen": "all"})).unwrap()["layout"].clone()
}

#[test]
fn get_hud_shows_the_panel_as_a_tree_and_sending_it_back_changes_nothing() {
    let (_folder, owner) = owner();
    let read = call(&owner, "get_hud", json!({})).unwrap();
    let panel = &read["panel"];
    assert_eq!(panel["type"], "panel");
    let sections: Vec<&str> = panel["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        sections,
        ["blocks", "tokens", "sessions", "quotaHistory", "planValue"]
    );
    assert_eq!(
        panel["children"][3],
        json!({"id": "quotaHistory", "type": "quotaHistory", "period": "day", "hidden": true}),
        "an added section starts hidden"
    );
    let blocks = &panel["children"][0]["children"];
    assert_eq!(
        blocks[0],
        json!({"id": "claude", "type": "quota", "widget": "bar"})
    );
    assert_eq!(blocks[2]["id"], "api:deepseek");
    assert_eq!(blocks[2]["type"], "balance");
    assert!(read["catalog"]["controls"]["rings"].is_object());
    let sent = draft(
        &owner,
        json!({"panel": panel, "revision": read["revision"]}),
    );
    assert_eq!(
        sent["note"],
        "This is the layout already saved, so there is no draft."
    );
    assert_eq!(drafts(&owner)["layouts"], json!([]));
}

#[test]
fn edits_change_the_nodes_they_name() {
    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"edit": [
            {"update": "claude", "widget": "rings", "ringSize": "large"},
            {"move": "codex", "before": "claude"},
            {"update": "sessions", "limit": 3},
            {"update": "panel", "spacing": "compact"}
        ]}),
    );
    let layout = layout(&owner);
    assert_eq!(
        layout["blocks"]["claude"],
        json!({"widget": "rings", "ringSize": "large"})
    );
    assert_eq!(layout["order"], json!(["codex"]));
    assert_eq!(layout["sessions"], json!({"limit": 3}));
    assert_eq!(layout["style"], json!({"spacing": "compact"}));
    // Removing one of the panel's own parts hides it, and says so.
    let result = draft(&owner, json!({"edit": [{"remove": "tokens"}]}));
    assert!(
        result["note"]
            .as_str()
            .unwrap()
            .contains("hidden, not deleted"),
        "{result}"
    );
    assert_eq!(layout_of(&owner)["hiddenSections"], json!(["tokens"]));
}

fn layout_of(owner: &Presentation) -> Value {
    layout(owner)
}

#[test]
fn an_agents_own_controls_are_checked_kept_and_named_for_later_edits() {
    let (_folder, owner) = owner();
    let result = draft(
        &owner,
        json!({"edit": [{"insert": {"id": "clock", "type": "column", "children": [
            {"type": "text", "text": "{now.time|clock}", "style": "title"},
            {"type": "repeat", "over": "windows", "where": {"agent": "claude"}, "sort": "-used", "limit": 2, "children": [
                {"type": "text", "text": "{window.short} {window.used}", "color": "level"}
            ]}
        ]}, "into": "blocks", "at": 0}]}),
    );
    assert!(
        result["note"]
            .as_str()
            .unwrap()
            .starts_with("Shown in the panel"),
        "{result}"
    );
    let layout = layout(&owner);
    assert_eq!(layout["order"][0], "clock");
    let clock = &layout["custom"]["clock"];
    assert_eq!(clock["children"][0]["id"], "clock-1");
    assert_eq!(clock["children"][1]["children"][0]["id"], "clock-3");
    // The tree shows it among the blocks, and its inner ids take edits.
    let read = call(&owner, "get_hud", json!({})).unwrap();
    assert_eq!(read["panel"]["children"][0]["children"][0]["id"], "clock");
    draft(
        &owner,
        json!({"edit": [{"update": "clock-1", "style": "figure"}]}),
    );
    assert_eq!(
        layout_of(&owner)["custom"]["clock"]["children"][0]["style"],
        "figure"
    );
    // A proposal in the layout's own fields keeps the control where it stood.
    draft(&owner, json!({"blocks": {"codex": {"hidden": true}}}));
    let kept = layout_of(&owner);
    assert_eq!(kept["order"][0], "clock");
    assert_eq!(kept["custom"]["clock"]["children"][0]["style"], "figure");
    assert_eq!(kept["blocks"]["codex"]["hidden"], true);
}

#[test]
fn controls_that_read_what_is_not_there_are_refused_with_what_is() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"edit": [{"insert": {"id": "x", "type": "column", "children": [
            {"type": "text", "text": "{window.used}"},
            {"type": "repeat", "over": "windows", "children": [{"type": "text", "text": "{window.percent}"}]},
            {"type": "bar", "value": 2},
            {"type": "picker", "state": "tab", "options": [{"value": "a", "text": "A"}, {"value": "b", "text": "B"}]},
            {"type": "button", "text": "hi"}
        ]}, "after": "tokens"}]}),
    );
    assert!(
        error.contains("window is read only inside a repeat over windows"),
        "{error}"
    );
    assert!(
        error.contains("a window has no field \"percent\""),
        "{error}"
    );
    assert!(error.contains("is a share from 0 to 1"), "{error}");
    assert!(error.contains("there is no state \"tab\""), "{error}");
    assert!(error.contains("\"button\", which is no control"), "{error}");
}

#[test]
fn a_tree_without_the_panels_own_parts_or_mixed_with_the_older_fields_is_refused() {
    let (_folder, owner) = owner();
    let read = call(&owner, "get_hud", json!({})).unwrap();
    let mut panel = read["panel"].clone();
    panel["children"]
        .as_array_mut()
        .unwrap()
        .retain(|child| child["id"] != "sessions");
    let error = refused(&owner, json!({"panel": panel}));
    assert!(error.contains("no \"sessions\" section"), "{error}");
    let error = refused(
        &owner,
        json!({"edit": [{"remove": "nothing"}], "order": ["codex"]}),
    );
    assert!(error.contains("spell the layout the older way"), "{error}");
    let error = refused(&owner, json!({"edit": [{"remove": "nothing"}]}));
    assert!(error.contains("no node has the id \"nothing\""), "{error}");
    let error = refused(
        &owner,
        json!({"edit": [{"insert": {"id": "q", "type": "text", "text": "hi"}, "into": "claude"}]}),
    );
    assert!(error.contains("nothing goes into \"claude\""), "{error}");
}

#[test]
fn a_click_needs_only_the_fields_its_action_takes() {
    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"edit": [{"insert": {"id": "open-token-stats", "type": "text", "text": "打开 Token 统计 →", "style": "body", "color": "agent", "align": "center", "onClick": {"open": "statistics", "tab": "tokens"}}, "after": "tokens"}]}),
    );
    assert_eq!(
        layout_of(&owner)["custom"]["open-token-stats"]["onClick"],
        json!({"open": "statistics", "tab": "tokens"})
    );
}

/// A row in the panel puts sections side by side: the layout keeps a slot for each in
/// the row and the sections' fields where they always are; the tree shows them in
/// the row, drawn there instead of at the panel's top.
#[test]
fn a_row_puts_the_panels_sections_side_by_side() {
    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"edit": [
            {"update": "panel", "width": 900},
            {"insert": {"id": "main", "type": "row", "children": []}, "into": "panel", "at": 0},
            {"move": "blocks", "into": "main"},
            {"insert": {"id": "side", "type": "column", "children": []}, "into": "main"},
            {"move": "sessions", "into": "side"},
            {"update": "sessions", "limit": 8, "width": 320},
            {"insert": {"id": "clock", "type": "text", "text": "{now.time|clock}", "width": 80}, "into": "side", "at": 0}
        ]}),
    );
    let layout = layout(&owner);
    assert_eq!(layout["style"]["width"], 900);
    assert_eq!(layout["sessions"], json!({"limit": 8}));
    assert_eq!(
        layout["custom"]["main"]["children"][0],
        json!({"id": "blocks", "type": "blocks"})
    );
    assert_eq!(
        layout["custom"]["main"]["children"][1]["children"][1],
        json!({"id": "sessions", "type": "sessions", "width": 320})
    );
    let tree = &call(&owner, "get_hud", json!({})).unwrap()["panel"];
    let top: Vec<&str> = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(top, ["main", "tokens", "quotaHistory", "planValue"]);
    let main = &tree["children"][0];
    assert_eq!(main["children"][0]["id"], "blocks");
    assert!(main["children"][0]["children"].as_array().unwrap().len() >= 2);
    assert_eq!(main["children"][1]["children"][1]["limit"], 8);
    assert_eq!(main["children"][1]["children"][1]["width"], 320);
    // Moving it back out puts it in the panel's list again, and sending the tree back
    // as it is changes nothing.
    draft(
        &owner,
        json!({"edit": [{"move": "sessions", "after": "tokens"}]}),
    );
    let tree = call(&owner, "get_hud", json!({})).unwrap()["panel"].clone();
    assert_eq!(tree["children"][2]["id"], "sessions");
    assert_eq!(tree["children"][2]["limit"], 8);
    let revision = owner.view()["revision"].clone();
    draft(&owner, json!({"revision": revision, "panel": tree}));
    assert_eq!(
        layout_of(&owner)["custom"]["main"]["children"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// A section sits in the panel or in a row, column or grid placed there: not in a
/// repeat, not among the blocks, and once.
#[test]
fn a_section_goes_only_where_it_can_be_drawn() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"edit": [
            {"insert": {"id": "list", "type": "repeat", "over": "agents", "children": []}, "into": "panel"},
            {"move": "tokens", "into": "list"}
        ]}),
    );
    assert!(
        error.contains("\"tokens\" stays in the panel, or in a row"),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"edit": [
            {"insert": {"id": "pair", "type": "row", "children": []}, "into": "blocks"},
            {"move": "sessions", "into": "pair"}
        ]}),
    );
    assert!(
        error.contains("not among the blocks or inside a repeat"),
        "{error}"
    );
    let error = refused(
        &owner,
        json!({"edit": [{"update": "tokens", "width": 300}]}),
    );
    assert!(
        error.contains("a section takes a width only inside a row"),
        "{error}"
    );
    let error = refused(&owner, json!({"edit": [{"update": "panel", "width": 961}]}));
    assert!(error.contains("480 to 960"), "{error}");
}

/// The older spelling keeps the sections a row holds where they are.
#[test]
fn a_proposal_in_the_older_spelling_keeps_sections_in_their_row() {
    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"edit": [
            {"insert": {"id": "main", "type": "row", "children": []}, "into": "panel", "at": 0},
            {"move": "blocks", "into": "main"},
            {"move": "tokens", "into": "main"}
        ]}),
    );
    draft(
        &owner,
        json!({"sections": ["sessions", "blocks", "tokens"], "countdown": false}),
    );
    let tree = &call(&owner, "get_hud", json!({})).unwrap()["panel"];
    let top: Vec<&str> = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(top, ["sessions", "main", "quotaHistory", "planValue"]);
    assert_eq!(tree["children"][1]["children"][1]["id"], "tokens");
}

/// Ring cells: a ring with its agent's logo inside, as big as its grid cell, and
/// cells that cover more of the grid; as the skill's example writes them.
#[test]
fn rings_make_compact_cells_that_span_a_grid() {
    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"edit": [
            {"update": "claude", "hidden": true},
            {"insert": {"id": "cells", "type": "grid", "columns": 4, "gap": "medium", "children": [
                {"type": "rings", "items": {"over": "windows", "where": {"agent": "claude"}}, "legend": false, "center": "percent", "size": "fill", "colSpan": 2, "rowSpan": 2},
                {"type": "repeat", "over": "agents", "where": {"id": ["codex", "cursor", "antigravity"]}, "children": [
                    {"type": "rings", "items": {"over": "windows", "where": {"agent": "{agent.id}"}}, "legend": false, "center": "percent", "onClick": {"open": "statistics", "tab": "tokens", "agent": "{agent.id}"}}
                ]}
            ]}, "into": "blocks", "at": 0}
        ]}),
    );
    let cells = &layout_of(&owner)["custom"]["cells"];
    assert_eq!(cells["children"][0]["colSpan"], 2);
    assert_eq!(cells["children"][0]["size"], "fill");
    assert_eq!(cells["children"][1]["children"][0]["center"], "percent");
    let error = refused(
        &owner,
        json!({"edit": [{"insert": {"id": "lone", "type": "rings", "items": {"over": "windows"}, "legend": false, "center": "face", "colSpan": 2}, "into": "panel"}]}),
    );
    assert!(
        error.contains("colSpan applies to a control inside a grid"),
        "{error}"
    );
    assert!(
        error.contains("lone.center is one of \"percent\", \"none\""),
        "{error}"
    );
}

/// A compact block: rings without their lines, sharing a line with its neighbour.
/// Bars have no legend to leave out and cannot share a line.
#[test]
fn compact_blocks_leave_out_their_lines_and_share_a_line() {
    let (_folder, owner) = owner();
    let error = refused(
        &owner,
        json!({"edit": [{"update": "codex", "widget": "bar", "legend": false, "span": "quarter"}]}),
    );
    assert!(error.contains("has no legend to leave out"), "{error}");
    assert!(error.contains("cannot share a line"), "{error}");
    draft(
        &owner,
        json!({"edit": [
            {"update": "claude", "widget": "rings", "legend": false, "span": "third"},
            {"update": "codex", "widget": "segments", "legend": false, "span": "sixth"}
        ]}),
    );
    let layout = layout_of(&owner);
    assert_eq!(
        layout["blocks"]["claude"],
        json!({"widget": "rings", "legend": false, "span": "third"})
    );
    let tree = call(&owner, "get_hud", json!({})).unwrap()["panel"].clone();
    let blocks = &tree["children"][0]["children"];
    assert_eq!(blocks[0]["legend"], false);
}

/// Settings keeps a block's columns when its widget changes to rings; an edit that
/// leaves the block alone sends that back and is not refused for it, in the tree and
/// in the older spelling. A value the layout does not hold yet still is.
#[test]
fn a_field_settings_kept_through_a_change_of_widget_is_no_issue() {
    let (_folder, owner) = owner();
    owner
        .set_layout(json!({"screen": "main", "layout": {"blocks": {"claude": {"widget": "ring", "columns": 3, "ringSize": "small"}}}}))
        .unwrap();
    draft(
        &owner,
        json!({"screen": "main", "edit": [{"insert": {"id": "clock", "type": "text", "text": "{now.time|clock}"}, "into": "blocks", "at": 0}]}),
    );
    draft(
        &owner,
        json!({"screen": "main", "blocks": {"claude": {"widget": "rings", "columns": 3}, "codex": {"hidden": true}}}),
    );
    let error = refused(
        &owner,
        json!({"screen": "main", "edit": [{"update": "claude", "columns": 4}]}),
    );
    assert!(error.contains("blocks.claude.columns"), "{error}");
}

/// Whatever an agent sends, the tools answer: a malformed node, edit or tree is
/// refused, never a panic in the app.
#[test]
fn no_proposal_makes_the_tools_panic() {
    let (_folder, owner) = owner();
    let nodes = [
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"open": "session"}}),
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"open": "settings"}}),
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"open": "url"}}),
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"set": "tab"}}),
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"next": 7}}),
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"copy": 1}}),
        json!({"id": "a", "type": "text", "text": "x", "onClick": {"open": 3, "set": "b"}}),
        json!({"id": "a", "type": "text", "text": "x", "when": {"above": 1}}),
        json!({"id": "a", "type": "text", "text": "x", "when": {"value": "{window.used}", "in": 4}}),
        json!({"id": "a", "type": "row", "children": [1, "x", null, [], {"type": "repeat"}, {"type": "text"}]}),
        json!({"id": "a", "type": "rings", "items": {"where": []}}),
        json!({"id": "a", "type": "rings", "items": {"over": "windows", "where": {"nope": 1}, "limit": 9}}),
        json!({"id": "a", "type": "picker", "state": 3, "options": [1, 2]}),
        json!({"id": "a", "type": "repeat", "over": "windows", "where": {"used": {"a": 1}}, "sort": 5, "children": [{"type": "text", "text": "{window.}"}]}),
        json!({"id": "a", "type": "text", "text": "}{", "state": "x", "span": 2}),
        json!({"id": "a", "type": "column", "state": {"t": {"options": "x"}, "": {}, "u": 3}}),
        json!({"id": "bad id", "type": "text", "text": "x"}),
        json!({"id": "claude", "type": "text", "text": "x"}),
        json!({"id": "a", "type": "quota"}),
        json!("text"),
        json!(null),
        json!([1]),
    ];
    let places = [
        json!({"into": "blocks"}),
        json!({"into": "panel", "at": -1}),
        json!({"into": "panel", "at": 99}),
        json!({"before": "claude"}),
        json!({"after": "panel"}),
        json!({"into": "sessions"}),
        json!({}),
    ];
    for node in &nodes {
        for place in &places {
            let mut step = place.as_object().unwrap().clone();
            step.insert("insert".into(), node.clone());
            let _ = call(&owner, "draft_hud", json!({"edit": [step]}));
        }
    }
    let deep = (0..12).fold(
        json!({"type": "text", "text": "x"}),
        |inner, _| json!({"type": "column", "children": [inner]}),
    );
    let _ = call(
        &owner,
        "draft_hud",
        json!({"edit": [{"insert": {"id": "deep", "type": "column", "children": [deep]}, "into": "panel"}]}),
    );
    for edit in [
        json!({"update": "panel", "children": []}),
        json!({"move": "panel", "into": "blocks"}),
        json!({"move": "blocks", "into": "claude"}),
        json!({"move": "claude", "before": "claude"}),
        json!({"move": "tokens", "into": "blocks"}),
        json!({"remove": "panel"}),
        json!({"replace": "claude", "with": {}}),
        json!({"replace": "nothing", "with": {"type": "text"}}),
        json!({"update": "claude", "type": "x", "id": "y"}),
        json!({"update": "claude", "widget": 4}),
        json!({"update": "sessions", "limit": "x", "children": 1}),
        json!({"insert": {"type": "text", "text": "x"}, "into": "blocks", "before": "claude"}),
        json!({}),
        json!(3),
        json!({"update": 1}),
        json!({"update": "claude", "remove": "codex"}),
    ] {
        let _ = call(&owner, "draft_hud", json!({"edit": [edit]}));
    }
    for panel in [
        json!({}),
        json!({"children": 5}),
        json!({"type": "row", "children": []}),
        json!({"type": "panel", "children": [{"id": "blocks", "children": 7}]}),
        json!({"children": [{"id": "blocks", "type": "blocks", "children": [{"id": "claude", "type": "balance"}, {"type": "quota"}, {"id": "codex"}, {"id": "codex"}]}, {"id": "tokens"}, {"id": "sessions"}, {"id": "tokens"}, {"type": "sessions", "children": [1]}]}),
        json!([]),
        json!("panel"),
    ] {
        let _ = call(&owner, "draft_hud", json!({"panel": panel}));
    }
    for odd in [
        json!({"edit": "x"}),
        json!({"edit": [], "hud": "simple"}),
        json!({"edit": [{"remove": "codex"}], "hud": {"agents": "x"}}),
        json!({"panel": {}, "edit": []}),
        json!({"edit": [], "from": "default"}),
    ] {
        let _ = call(&owner, "draft_hud", odd);
    }
    assert!(call(&owner, "get_hud", json!({})).is_ok());
}

#[test]
fn a_layout_saved_with_measures_loads_without_them() {
    // Every block offers all its measures; what an earlier layout chose is dropped.
    let layout = super::rules::normalize_layout(&json!({"blocks": {"claude": {
        "widget": "ring", "measures": ["quota", "burn"], "hiddenRows": ["acct1/claude-weekly"]
    }}}))
    .unwrap();
    assert_eq!(
        layout["blocks"]["claude"],
        json!({"widget": "rings", "hiddenRows": ["acct1/claude-weekly"]})
    );
}

/// The quota history and the plan value are sections a layout shows only once
/// it names them: a layout saved before them stays as it was drawn, an agent
/// shows one by updating it, and Settings' reordering does not show one.
#[test]
fn an_added_section_shows_only_once_it_is_asked_for() {
    let usual = rules::normalize_layout(&json!({"hiddenSections": ["sessions"]})).unwrap();
    assert_eq!(usual, json!({"hiddenSections": ["sessions"]}));
    let reordered =
        rules::normalize_layout(&json!({"sections": ["sessions", "blocks", "tokens"]})).unwrap();
    assert_eq!(
        reordered,
        json!({"sections": ["sessions", "blocks", "tokens", "quotaHistory", "planValue"],
               "hiddenSections": ["quotaHistory", "planValue"]})
    );
    assert_eq!(
        rules::normalize_layout(&reordered).unwrap(),
        reordered,
        "written once"
    );

    let (_folder, owner) = owner();
    draft(
        &owner,
        json!({"edit": [{"update": "quotaHistory", "hidden": false, "period": "week",
                         "hiddenModels": ["claude-model:claude-opus-4-5"]}]}),
    );
    let layout = layout_of(&owner);
    assert!(
        strings(&layout["sections"]).contains(&"quotaHistory".to_string()),
        "{layout}"
    );
    assert_eq!(layout["hiddenSections"], json!(["planValue"]));
    assert_eq!(
        layout["quotaHistory"],
        json!({"period": "week", "hiddenModels": ["claude-model:claude-opus-4-5"]})
    );
    let tree = &call(&owner, "get_hud", json!({})).unwrap()["panel"];
    assert!(tree["children"][3]["hidden"].is_null());
    assert_eq!(tree["children"][4]["hidden"], true);

    let error = refused(&owner, json!({"planValue": {"period": "week"}}));
    assert!(
        error.contains("planValue has no property \"period\""),
        "{error}"
    );

    let mut moved = json!({});
    panel_keys::panel_patch(
        &mut moved,
        json!({"panelSections": ["sessions", "blocks", "tokens", "quotaHistory", "planValue"]})
            .as_object()
            .unwrap(),
    );
    assert_eq!(
        rules::normalize_layout(&moved).unwrap()["hiddenSections"],
        json!(["quotaHistory", "planValue"]),
        "reordering keeps them hidden"
    );
}
