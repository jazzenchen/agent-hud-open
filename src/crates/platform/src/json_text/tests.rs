use super::merge::align;
use super::*;
use serde_json::json;

fn edit(original: &str, updated: Value) -> String {
    let bytes = edited(original.as_bytes(), &updated).expect("edited in place");
    String::from_utf8(bytes).unwrap()
}

const CLAUDE: &str = r#"{
  "permissions": {
    "allow": ["Bash(ls:*)"],
    "defaultMode": "plan"
  },
  "model": "opus",
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "hooks": [{ "type": "command", "command": "say done" }]
      }
    ]
  },
  "env": { "B": "2", "A": "a\/b" }
}
"#;

#[test]
fn an_unchanged_value_keeps_its_file() {
    let current: Value = serde_json::from_str(CLAUDE).unwrap();
    assert_eq!(edit(CLAUDE, current), CLAUDE);
}

#[test]
fn a_new_key_is_appended_and_the_rest_stays() {
    let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
    updated["statusLine"] = json!({"type": "command", "command": "hud"});
    assert_eq!(
        edit(CLAUDE, updated),
        CLAUDE.replace(
            "\"A\": \"a\\/b\" }\n}",
            "\"A\": \"a\\/b\" },\n  \"statusLine\": {\n    \"command\": \"hud\",\n    \"type\": \"command\"\n  }\n}"
        )
    );
}

#[test]
fn a_changed_string_is_the_only_change() {
    let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
    updated["hooks"]["Stop"][0]["hooks"][0]["command"] = json!("say finished");
    assert_eq!(
        edit(CLAUDE, updated),
        CLAUDE.replace("say done", "say finished")
    );
}

#[test]
fn removed_keys_take_their_comma_with_them() {
    let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
    updated.as_object_mut().unwrap().remove("permissions");
    updated.as_object_mut().unwrap().remove("env");
    let expected = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "hooks": [{ "type": "command", "command": "say done" }]
      }
    ]
  }
}
"#;
    assert_eq!(edit(CLAUDE, updated), expected);
    let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
    updated.as_object_mut().unwrap().remove("model");
    assert_eq!(
        edit(CLAUDE, updated),
        CLAUDE.replace("  \"model\": \"opus\",\n", "")
    );
}

#[test]
fn array_items_are_added_removed_and_changed_in_place() {
    let original = "{\n  \"Stop\": [\n    { \"a\": 1 },\n    { \"b\": 2 }\n  ]\n}\n";
    let edited = edit(original, json!({"Stop": [{"b": 2}, {"c": 3, "a": 0}]}));
    assert_eq!(
        edited,
        "{\n  \"Stop\": [\n    { \"b\": 2 },\n    {\n      \"a\": 0,\n      \"c\": 3\n    }\n  ]\n}\n"
    );
    let edited = edit(original, json!({"Stop": [{"a": 1}, {"b": 3}]}));
    assert_eq!(edited, original.replace("\"b\": 2", "\"b\": 3"));
    let edited = edit(original, json!({"Stop": [{"z": 0}, {"a": 1}, {"b": 2}]}));
    assert_eq!(
        edited,
        "{\n  \"Stop\": [\n    {\n      \"z\": 0\n    },\n    { \"a\": 1 },\n    { \"b\": 2 }\n  ]\n}\n"
    );
}

#[test]
fn empty_containers_open_up_in_the_files_style() {
    assert_eq!(
        edit("{}\n", json!({"hooks": {"Stop": []}})),
        "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n"
    );
    assert_eq!(
        edit("{\n\t\"hooks\": {}\n}", json!({"hooks": {"Stop": [1]}})),
        "{\n\t\"hooks\": {\n\t\t\"Stop\": [\n\t\t\t1\n\t\t]\n\t}\n}"
    );
    assert_eq!(edit("{\n  \"hooks\": {\"a\": 1}\n}", json!({})), "{}");
}

#[test]
fn compact_and_crlf_files_keep_their_form() {
    assert_eq!(
        edit(
            r#"{"b":1,"a":{"x":true}}"#,
            json!({"b": 1, "a": {"x": false}, "c": [1]})
        ),
        r#"{"b":1,"a":{"x":false},"c":[1]}"#
    );
    assert_eq!(
        edit("{\r\n  \"b\": 1\r\n}\r\n", json!({"b": 1, "a": {"x": 1}})),
        "{\r\n  \"b\": 1,\r\n  \"a\": {\r\n    \"x\": 1\r\n  }\r\n}\r\n"
    );
}

#[test]
fn a_file_the_editor_cannot_follow_is_written_whole() {
    let updated = json!({"a": 2});
    assert_eq!(edited(br#"{"a": 1, "a": 3}"#, &updated), None);
    let bytes = rewrite(Some(br#"{"a": 1, "a": 3}"#), &updated).unwrap();
    assert_eq!(bytes, b"{\n  \"a\": 2\n}\n");
    assert_eq!(rewrite(None, &updated).unwrap(), b"{\n  \"a\": 2\n}\n");
    assert_eq!(rewrite(Some(b""), &updated).unwrap(), b"{\n  \"a\": 2\n}\n");
}

#[test]
fn installing_then_removing_a_hook_restores_the_file() {
    let current: Value = serde_json::from_str(CLAUDE).unwrap();
    let command = "'/Applications/Agent HUD.app/Contents/MacOS/Agent HUD' --permission-hook claude";
    let installed = agent_hud_agents::patch_configuration(
        "claude",
        "--permission-hook",
        &current,
        Some(command),
    )
    .unwrap();
    let text = String::from_utf8(rewrite(Some(CLAUDE.as_bytes()), &installed).unwrap()).unwrap();
    assert!(text.starts_with("{\n  \"permissions\": {\n    \"allow\": [\"Bash(ls:*)\"],"));
    assert!(text.contains("\"Stop\": [\n      {\n        \"matcher\": \"\",\n        \"hooks\": [{ \"type\": \"command\", \"command\": \"say done\" }]"));
    assert!(text.ends_with("  \"env\": { \"B\": \"2\", \"A\": \"a\\/b\" }\n}\n"));
    let removed =
        agent_hud_agents::patch_configuration("claude", "--permission-hook", &installed, None)
            .unwrap();
    let restored = rewrite(Some(text.as_bytes()), &removed).unwrap();
    assert_eq!(String::from_utf8(restored).unwrap(), CLAUDE);
}

#[test]
fn alignment_pairs_equal_items_first() {
    let v = |items: &[i64]| items.iter().map(|&i| json!(i)).collect::<Vec<_>>();
    assert_eq!(
        align(&v(&[1, 2, 3]), &v(&[1, 3])),
        [(Some(0), Some(0)), (Some(1), None), (Some(2), Some(1))]
    );
    assert_eq!(
        align(&v(&[1, 2]), &v(&[1, 9, 2, 8])),
        [
            (Some(0), Some(0)),
            (None, Some(1)),
            (Some(1), Some(2)),
            (None, Some(3))
        ]
    );
    assert_eq!(align(&v(&[5]), &v(&[6])), [(Some(0), Some(0))]);
}
