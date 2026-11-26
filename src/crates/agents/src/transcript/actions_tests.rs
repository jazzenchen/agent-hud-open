use super::*;
use serde_json::json;

fn plan_after(calls: &[(&str, Value)]) -> Value {
    let mut plan = Plan::default();
    for (name, arguments) in calls {
        for action in actions(name, arguments) {
            plan.apply(&action["plan"]);
        }
    }
    plan.steps()
}

#[test]
fn the_claude_desktop_plan_is_set_whole_and_then_moved_along() {
    let set = json!({"items":[
        {"content":"Read the code","status":"in_progress"},
        {"content":"Write the fix","status":"pending"},
        {"content":"Run the tests","status":"pending"},
    ],"scope":"first_task"});
    let name = "mcp__ccd_turn__update_plan";
    assert_eq!(
        plan_after(&[(name, set.clone())]),
        json!([
            {"text":"Read the code","status":"active"},
            {"text":"Write the fix","status":"pending"},
            {"text":"Run the tests","status":"pending"},
        ])
    );
    assert_eq!(
        plan_after(&[(name, set.clone()), (name, json!({"advance":true}))]),
        json!([
            {"text":"Read the code","status":"done"},
            {"text":"Write the fix","status":"active"},
            {"text":"Run the tests","status":"pending"},
        ])
    );
    assert_eq!(
        plan_after(&[(name, set), (name, json!({"done":[1,2],"start":3}))]),
        json!([
            {"text":"Read the code","status":"done"},
            {"text":"Write the fix","status":"done"},
            {"text":"Run the tests","status":"active"},
        ])
    );
}

#[test]
fn a_step_moved_on_in_a_plan_never_seen_whole_changes_nothing() {
    let mut plan = Plan::default();
    let advance = actions("mcp__ccd_turn__update_plan", &json!({"advance":true}));
    assert_eq!(advance.len(), 1);
    assert!(!plan.apply(&advance[0]["plan"]));
    assert!(plan.is_empty());
}

#[test]
fn claude_code_tasks_are_added_and_updated_by_number() {
    let steps = plan_after(&[
        (
            "TaskCreate",
            json!({"subject":"Scaffold the app","description":"…","activeForm":"Scaffolding"}),
        ),
        ("TaskCreate", json!({"subject":"Port the model"})),
        ("TaskCreate", json!({"subject":"Drop me"})),
        ("TaskUpdate", json!({"taskId":"1","status":"completed"})),
        (
            "TaskUpdate",
            json!({"taskId":"2","status":"in_progress","subject":"Port the domain model"}),
        ),
        ("TaskUpdate", json!({"taskId":"3","status":"deleted"})),
        ("TaskCreate", json!({"subject":"Verify"})),
    ]);
    assert_eq!(
        steps,
        json!([
            {"text":"Scaffold the app","status":"done"},
            {"text":"Port the domain model","status":"active"},
            {"text":"Verify","status":"pending"},
        ])
    );
}

#[test]
fn todo_lists_and_the_codex_plan_are_whole_plans() {
    assert_eq!(
        plan_after(&[(
            "TodoWrite",
            json!({"todos":[{"content":"A","status":"completed","activeForm":"Doing A"},{"content":"B","status":"in_progress"}]})
        )]),
        json!([{"text":"A","status":"done"},{"text":"B","status":"active"}])
    );
    // Codex writes a function call's arguments as JSON in a string.
    let arguments = json!("{\"explanation\":\"x\",\"plan\":[{\"step\":\"Look\",\"status\":\"completed\"},{\"step\":\"Fix\",\"status\":\"in_progress\"}]}");
    assert_eq!(
        plan_after(&[("update_plan", arguments)]),
        json!([{"text":"Look","status":"done"},{"text":"Fix","status":"active"}])
    );
}

#[test]
fn an_edit_keeps_what_changed_and_a_few_lines_beside_it() {
    let old = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine";
    let new = "one\ntwo\nthree\nfour\nFIVE\nfive and a half\nsix\nseven\neight\nnine";
    let found = actions(
        "Edit",
        &json!({"file_path":"/p/a.rs","old_string":old,"new_string":new,"replace_all":false}),
    );
    assert_eq!(
        found,
        vec![
            json!({"edit":{"path":"/p/a.rs","change":"edit","added":2,"removed":1,"cut":0,
            "lines":[" two"," three"," four","-five","+FIVE","+five and a half"," six"," seven"," eight"]}})
        ]
    );
}

#[test]
fn several_edits_of_one_file_are_one_edit_in_parts() {
    let found = actions(
        "MultiEdit",
        &json!({"file_path":"a.txt","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":""}]}),
    );
    assert_eq!(found[0]["edit"]["lines"], json!(["-a", "+b", "@@", "-c"]));
}

#[test]
fn a_written_file_is_all_new_lines_and_cut_to_size() {
    let content = (0..500)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let found = actions("Write", &json!({"file_path":"/p/new.md","content":content}));
    let edit = &found[0]["edit"];
    assert_eq!(edit["change"], "create");
    assert_eq!(edit["added"], 500);
    assert_eq!(edit["lines"].as_array().unwrap().len(), EDIT_LINES);
    assert_eq!(edit["cut"], 500 - EDIT_LINES);
    assert_eq!(edit["lines"][0], "+line 0");
}

#[test]
fn a_codex_patch_is_one_edit_a_file() {
    let patch = "*** Begin Patch\n*** Add File: notes.md\n+# Notes\n+first\n*** Update File: src/a.rs\n*** Move to: src/b.rs\n@@ fn main\n-    old();\n+    new();\n context\n@@\n-gone\n*** Delete File: old.txt\n*** End Patch\n";
    let found = actions("apply_patch", &json!(patch));
    assert_eq!(found.len(), 3);
    assert_eq!(
        found[0],
        json!({"edit":{"path":"notes.md","change":"create","added":2,"removed":0,"cut":0,"lines":["+# Notes","+first"]}})
    );
    assert_eq!(found[1]["edit"]["path"], "src/b.rs");
    assert_eq!(
        found[1]["edit"]["lines"],
        json!(["-    old();", "+    new();", " context", "@@", "-gone"])
    );
    assert_eq!(found[2]["edit"]["change"], "delete");
}

#[test]
fn shell_calls_are_commands_however_the_command_is_written() {
    assert_eq!(
        actions(
            "Bash",
            &json!({"command":"cargo test -p agents","description":"Run tests"})
        ),
        vec![json!({"command":"cargo test -p agents","title":"Run tests"})]
    );
    assert_eq!(
        actions(
            "shell",
            &json!("{\"command\":[\"bash\",\"-lc\",\"ls -la\"],\"workdir\":\"/p\"}")
        ),
        vec![json!({"command":"ls -la"})]
    );
    assert_eq!(
        actions("exec_command", &json!("{\"cmd\":\"git status\"}")),
        vec![json!({"command":"git status"})]
    );
    assert!(keeps_output(&actions("Bash", &json!({"command":"ls"}))));
    assert!(!keeps_output(&actions(
        "Write",
        &json!({"file_path":"a","content":"b"})
    )));
}

#[test]
fn a_codex_desktop_script_holds_its_commands_and_patches() {
    let source = "const results = await Promise.allSettled([\n tools.exec_command({cmd:\"cat /p/SKILL.md\",\"max_output_tokens\":8000}),\n tools.exec_command({\"cmd\":\"rg -n \\\"Mac|采购\\\" notes.md\",workdir:'/p'})\n]);\ntext(await tools.apply_patch(`*** Begin Patch\n*** Add File: /p/story.md\n+# 云舟过街\n*** End Patch`));\ntext(await tools.view_image({path:\"a.png\"}));";
    let found = actions("exec", &json!(source));
    assert_eq!(
        found,
        vec![
            json!({"command":"cat /p/SKILL.md"}),
            json!({"command":"rg -n \"Mac|采购\" notes.md"}),
            json!({"edit":{"path":"/p/story.md","change":"create","added":1,"removed":0,"cut":0,"lines":["+# 云舟过街"]}}),
        ]
    );
}

#[test]
fn files_handed_to_the_user_are_kept_by_their_paths() {
    assert_eq!(
        actions(
            "SendUserFile",
            &json!({"files":["/p/out/cover.png","out/cut.mp4"],"caption":"Before and after","status":"normal"})
        ),
        vec![
            json!({"files":{"paths":["/p/out/cover.png","out/cut.mp4"],"caption":"Before and after"}})
        ]
    );
    assert_eq!(
        actions("SendUserFile", &json!({"files":[]})),
        Vec::<Value>::new()
    );
}

#[test]
fn antigravity_writes_each_argument_as_json_in_a_string() {
    assert_eq!(
        actions(
            "edit_file",
            &json!({"Path":"\"/p/src/a.ts\"","Target":"\"const a = 1;\\nconst b = 2;\"","Replacement":"\"const a = 1;\\nconst b = 3;\""})
        ),
        vec![
            json!({"edit":{"path":"/p/src/a.ts","change":"edit","added":1,"removed":1,"cut":0,
            "lines":[" const a = 1;","-const b = 2;","+const b = 3;"]}})
        ]
    );
    let written = actions(
        "write_file",
        &json!({"Path":"\"/p/notes.md\"","Content":"\"# Notes\\nfirst\"","Overwrite":"true"}),
    );
    assert_eq!(written[0]["edit"]["lines"], json!(["+# Notes", "+first"]));
    assert_eq!(
        actions(
            "run_command",
            &json!({"CommandLine":"\"npx tsc --noEmit\"","WaitMsBeforeAsync":"10000","toolSummary":"\"TypeScript typecheck\""})
        ),
        vec![json!({"command":"npx tsc --noEmit","title":"TypeScript typecheck"})]
    );
    assert_eq!(
        plan_after(&[(
            "update_steps",
            json!({"steps":"[{\"status\":\"done\",\"title\":\"Create the module\"},{\"status\":\"in_progress\",\"title\":\"Wire it\"},{\"status\":\"pending\",\"title\":\"Verify\"}]"})
        )]),
        json!([
            {"text":"Create the module","status":"done"},
            {"text":"Wire it","status":"active"},
            {"text":"Verify","status":"pending"},
        ])
    );
}

#[test]
fn another_clients_tools_of_the_same_names_are_read_as_written() {
    // An MCP server's write_file, from Claude Code: the file's text is JSON.
    let written = actions(
        "mcp__filesystem__write_file",
        &json!({"path":"/p/package.json","content":"{\"name\":\"shop\"}"}),
    );
    assert_eq!(written[0]["edit"]["path"], "/p/package.json");
    assert_eq!(written[0]["edit"]["lines"], json!(["+{\"name\":\"shop\"}"]));
}

#[test]
fn an_exit_code_is_read_from_a_commands_output_only() {
    let printed = "{\"exit_code\": 1, \"note\": \"from a file\"}\n";
    assert_eq!(result(Some(printed), false, false)["failed"], false);
    assert_eq!(
        result(Some("x \"exit_code\": 2"), false, true)["failed"],
        true
    );
    // A log's own wrapper still says so for any call.
    let wrapped = r#"{"output":"patch failed","metadata":{"exit_code":1}}"#;
    assert_eq!(result(Some(wrapped), false, false)["failed"], true);
}

#[test]
fn tools_of_no_kind_have_no_action() {
    for (name, arguments) in [
        ("Read", json!({"file_path":"/p/a.rs"})),
        (
            "mcp__claude-in-chrome__navigate",
            json!({"url":"https://example.com"}),
        ),
        ("js", json!("{\"code\":\"1+1\"}")),
        ("update_plan", json!({})),
    ] {
        assert_eq!(actions(name, &arguments), Vec::<Value>::new(), "{name}");
    }
}

#[test]
fn a_commands_output_is_its_end_and_says_whether_it_failed() {
    let long = format!("{}\nlast line", "x".repeat(OUTPUT_BYTES * 2));
    let kept = result(Some(&long), false, true);
    assert_eq!(kept["failed"], false);
    assert!(kept["output"].as_str().unwrap().ends_with("last line"));
    assert_eq!(kept["output"].as_str().unwrap().len(), OUTPUT_BYTES);
    assert_eq!(kept["outputCut"], long.len() - OUTPUT_BYTES);

    assert_eq!(
        result(Some("boom"), true, true),
        json!({"failed":true,"output":"boom"})
    );
    // Codex wraps the output and its exit code.
    assert_eq!(
        result(Some("{\"output\":\"no such file\\n\",\"metadata\":{\"exit_code\":2,\"duration_seconds\":0.1}}"), false, true),
        json!({"failed":true,"output":"no such file"})
    );
    assert_eq!(
        result(
            Some("Script completed\nOutput:\n{\"exit_code\":0,\"output\":\"ok\"}"),
            false,
            true
        )["failed"],
        false
    );
}

#[test]
fn an_edit_that_worked_keeps_nothing_of_its_answer() {
    assert_eq!(
        result(
            Some("The file a.rs has been updated successfully."),
            false,
            false
        ),
        json!({"failed":false})
    );
    assert_eq!(
        result(Some("String to replace not found in file."), true, false),
        json!({"failed":true,"output":"String to replace not found in file."})
    );
}

/// A plain hash of the rules' own text (FNV-1a), to notice that it changed.
fn rules_text_hash() -> u64 {
    [
        include_str!("actions.rs"),
        include_str!("actions/plan.rs"),
        include_str!("actions/edits.rs"),
        include_str!("actions/commands.rs"),
    ]
    .iter()
    .flat_map(|text| text.bytes())
    .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The rules changed since their version was last recorded here. If what
/// any call reads as changed (a tool newly read, another field, another
/// bound), raise `VERSION`: stores then read their older items again. Then
/// record the new pair below. A change of wording alone keeps the version
/// and records only the new hash.
#[test]
fn the_rules_version_is_raised_with_the_rules() {
    const RECORDED: (u32, u64) = (4, 9_254_088_966_552_192_592);
    assert_eq!(
        (VERSION, rules_text_hash()),
        RECORDED,
        "the action rules changed: see this test's note"
    );
}
