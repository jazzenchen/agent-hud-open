//! Bounded extraction of Claude Code's managed Project wake origin.
//!
//! A Project wake is user content, so only its two IDs survive parsing. This
//! deliberately accepts the app's narrowly marked SDK wake envelope rather
//! than searching arbitrary prompt text for values that look like IDs.
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_HEADER_BYTES: usize = 4096;
const MAX_ORIGIN_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaudeProjectOrigin {
    Known {
        project_id: String,
        thread_id: String,
    },
    Unknown,
}

/// Returns None when the line is not a marked Project wake, including ordinary
/// user text containing a lookalike. A marked but malformed wake returns Err.
pub(crate) fn managed_origin(value: &Value) -> Option<Result<ClaudeProjectOrigin, ()>> {
    if !is_managed_project_prompt(value) {
        return None;
    }
    let content = value["message"]["content"].as_str()?;
    if !wake_candidate(content) {
        return None;
    }
    if content.len() > MAX_ORIGIN_CONTENT_BYTES {
        return Some(Err(()));
    }
    Some(
        parse_header(content)
            .ok_or(())
            .map(|(project_id, thread_id)| ClaudeProjectOrigin::Known {
                project_id,
                thread_id,
            }),
    )
}

/// Whether this line is the app's marked Project wake prompt, even if its
/// header is malformed. Consumers can avoid presenting the control envelope
/// as a user-authored title while preserving its ordinary turn accounting.
pub fn is_managed_project_prompt(value: &Value) -> bool {
    managed_user_turn(value)
        && value["message"]["content"]
            .as_str()
            .is_some_and(wake_candidate)
}

fn managed_user_turn(value: &Value) -> bool {
    let message = &value["message"];
    value["type"] == "user"
        && message
            .get("role")
            .is_none_or(|role| role.as_str() == Some("user"))
        && value["projectsUserTurn"] == true
        && value["textNotTyped"] == true
        && value["promptSource"] == "sdk"
        && value["entrypoint"] == "claude-desktop"
        && value["isSidechain"] == false
        && value["isMeta"] != true
        && value["isCompactSummary"] != true
        && !message["content"]
            .as_array()
            .is_some_and(|parts| parts.iter().any(|part| part["type"] == "tool_result"))
        && !value["type"]
            .as_str()
            .is_some_and(|kind| kind.starts_with("api-request"))
}

fn wake_candidate(content: &str) -> bool {
    content
        .strip_prefix("<wake")
        .and_then(|tail| tail.as_bytes().first())
        .is_some_and(|next| next.is_ascii_whitespace() || *next == b'>')
}

fn parse_header(content: &str) -> Option<(String, String)> {
    let full_content = content;
    let header_window = if content.len() <= MAX_HEADER_BYTES {
        content
    } else {
        let mut end = MAX_HEADER_BYTES;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        &content[..end]
    };
    let content = header_window;
    let mut cursor = 0;
    parse_open_tag(content, &mut cursor, "wake")?;
    skip_space(content, &mut cursor);
    let project = parse_open_tag(content, &mut cursor, "project")?;
    if project.get("type").map(String::as_str) != Some("project") {
        return None;
    }
    let project_id = project.get("id")?;
    if !valid_project_id(project_id) {
        return None;
    }
    skip_space(content, &mut cursor);
    let thread = parse_open_tag(content, &mut cursor, "thread")?;
    let thread_id = thread.get("ts")?;
    if !valid_thread_id(thread_id, project_id) {
        return None;
    }
    // The tail is never parsed for identity or retained. It is only scanned
    // as a bounded ambiguity sentinel so a second structural tag cannot be
    // mistaken for part of a unique origin envelope.
    if has_additional_structural_tag(full_content, cursor) {
        return None;
    }
    Some((project_id.clone(), thread_id.clone()))
}

fn has_additional_structural_tag(content: &str, from: usize) -> bool {
    ["<project", "<thread"].into_iter().any(|needle| {
        let mut search_from = from;
        while let Some(relative) = content
            .get(search_from..)
            .and_then(|tail| tail.find(needle))
        {
            let start = search_from + relative;
            let after = start + needle.len();
            if content
                .as_bytes()
                .get(after)
                .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(*byte, b'>' | b'/'))
            {
                return true;
            }
            search_from = after;
        }
        false
    })
}

fn parse_open_tag(
    input: &str,
    cursor: &mut usize,
    expected_name: &str,
) -> Option<std::collections::HashMap<String, String>> {
    let bytes = input.as_bytes();
    skip_space(input, cursor);
    if bytes.get(*cursor) != Some(&b'<') {
        return None;
    }
    *cursor += 1;
    let start = *cursor;
    while bytes
        .get(*cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
    {
        *cursor += 1;
    }
    if input.get(start..*cursor)? != expected_name {
        return None;
    }

    let mut attributes = std::collections::HashMap::new();
    loop {
        let before_space = *cursor;
        skip_space(input, cursor);
        match bytes.get(*cursor)? {
            b'>' => {
                *cursor += 1;
                return Some(attributes);
            }
            b'/' => return None,
            _ => {}
        }
        if *cursor == before_space {
            return None;
        }
        let name_start = *cursor;
        while bytes
            .get(*cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b':'))
        {
            *cursor += 1;
        }
        if *cursor == name_start {
            return None;
        }
        let name = input.get(name_start..*cursor)?.to_owned();
        skip_space(input, cursor);
        if bytes.get(*cursor) != Some(&b'=') {
            return None;
        }
        *cursor += 1;
        skip_space(input, cursor);
        let quote = *bytes.get(*cursor)?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        *cursor += 1;
        let value_start = *cursor;
        while bytes
            .get(*cursor)
            .is_some_and(|byte| *byte != quote && *byte != b'<' && *byte != b'&')
        {
            *cursor += 1;
        }
        if bytes.get(*cursor) != Some(&quote) {
            return None;
        }
        let value = input.get(value_start..*cursor)?.to_owned();
        *cursor += 1;
        if attributes.insert(name, value).is_some() {
            return None;
        }
    }
}

fn skip_space(input: &str, cursor: &mut usize) {
    while input
        .as_bytes()
        .get(*cursor)
        .is_some_and(u8::is_ascii_whitespace)
    {
        *cursor += 1;
    }
}

fn valid_project_id(value: &str) -> bool {
    bounded_identifier(value, "chan_")
}

fn valid_thread_id(value: &str, project_id: &str) -> bool {
    let Some(suffix) = value.strip_prefix("cmsg_") else {
        return false;
    };
    let project_suffix = project_id.strip_prefix("chan_").unwrap_or_default();
    suffix.starts_with(project_suffix)
        && suffix.len() > project_suffix.len()
        && (6..=120).contains(&suffix.len())
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn bounded_identifier(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        (6..=120).contains(&suffix.len())
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveLog;
    use serde_json::json;

    const PROJECT: &str = "chan_abcdef123456";
    const THREAD: &str = "cmsg_abcdef123456_789";

    fn wake(project: &str, thread: &str) -> String {
        format!(
            "<wake source=\"project\"><project id=\"{project}\" type=\"project\"><thread ts=\"{thread}\">"
        )
    }

    fn line(content: &str) -> Value {
        json!({
            "type":"user",
            "projectsUserTurn":true,
            "textNotTyped":true,
            "promptSource":"sdk",
            "entrypoint":"claude-desktop",
            "isSidechain":false,
            "message":{"role":"user","content":content}
        })
    }

    fn feed(log: &mut LiveLog, content: &str) {
        log.claude(&line(content));
    }

    #[test]
    fn only_a_marked_managed_wake_header_captures_ids() {
        let mut log = LiveLog::default();
        feed(
            &mut log,
            &format!("{}{}", wake(PROJECT, THREAD), "x".repeat(5000)),
        );
        assert_eq!(
            log.claude_project_origin,
            Some(ClaudeProjectOrigin::Known {
                project_id: PROJECT.into(),
                thread_id: THREAD.into(),
            })
        );

        for content in [
            format!("quoted: {}", wake(PROJECT, THREAD)),
            format!("<wakeup>{}</wakeup>", wake(PROJECT, THREAD)),
            "ordinary text mentioning chan_abcdef123456".into(),
        ] {
            assert_eq!(managed_origin(&line(&content)), None);
        }
        let mut absent = line(&wake(PROJECT, THREAD));
        absent.as_object_mut().unwrap().remove("projectsUserTurn");
        assert_eq!(managed_origin(&absent), None);
    }

    #[test]
    fn malformed_managed_headers_and_conflicts_become_sticky_unknown() {
        let mut malformed = LiveLog::default();
        feed(&mut malformed, &wake("chan_bad!", "cmsg_bad!"));
        assert_eq!(
            malformed.claude_project_origin,
            Some(ClaudeProjectOrigin::Unknown)
        );

        let mut duplicate = LiveLog::default();
        feed(
            &mut duplicate,
            &format!(
                "<wake><project id=\"{PROJECT}\" id=\"{PROJECT}\" type=\"project\"><thread ts=\"{THREAD}\">"
            ),
        );
        assert_eq!(
            duplicate.claude_project_origin,
            Some(ClaudeProjectOrigin::Unknown)
        );

        for malformed_header in [
            format!("<wake><project id=\"{PROJECT}\"type=\"project\"><thread ts=\"{THREAD}\">"),
            format!(
                "<wake><project id=\"{PROJECT}\" type=\"project\"><thread ts=\"cmsg_abcdef123456\">"
            ),
            format!(
                "{}<project id=\"{PROJECT}\" type=\"project\">",
                wake(PROJECT, THREAD)
            ),
            format!("{}<thread ts=\"{THREAD}\">", wake(PROJECT, THREAD)),
            format!(
                "{}{}<thread ts=\"{THREAD}\">",
                wake(PROJECT, THREAD),
                "x".repeat(5000)
            ),
        ] {
            let mut log = LiveLog::default();
            feed(&mut log, &malformed_header);
            assert_eq!(
                log.claude_project_origin,
                Some(ClaudeProjectOrigin::Unknown),
                "{malformed_header:?}"
            );
        }

        let mut conflict = LiveLog::default();
        feed(&mut conflict, &wake(PROJECT, THREAD));
        feed(&mut conflict, &wake(PROJECT, "cmsg_abcdef123456_other"));
        feed(&mut conflict, &wake(PROJECT, THREAD));
        assert_eq!(
            conflict.claude_project_origin,
            Some(ClaudeProjectOrigin::Unknown)
        );
    }

    #[test]
    fn repeated_matching_header_is_stable_and_checkpoint_keeps_only_ids() {
        let mut log = LiveLog::default();
        feed(&mut log, &wake(PROJECT, THREAD));
        feed(&mut log, &wake(PROJECT, THREAD));
        let encoded = serde_json::to_string(&log).unwrap();
        assert!(!encoded.contains("source=\"project\""));
        assert!(!encoded.contains("continue"));
        let decoded: LiveLog = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.claude_project_origin, log.claude_project_origin);
    }

    #[test]
    fn non_user_and_sidechain_lookalikes_are_ignored() {
        let mut assistant = line(&wake(PROJECT, THREAD));
        assistant["type"] = json!("assistant");
        assistant["message"]["role"] = json!("assistant");
        assert_eq!(managed_origin(&assistant), None);

        let mut sidechain = line(&wake(PROJECT, THREAD));
        sidechain["isSidechain"] = json!(true);
        assert_eq!(managed_origin(&sidechain), None);

        let mut wrong_type = line(&wake(PROJECT, THREAD));
        wrong_type["type"] = json!("assistant");
        assert_eq!(managed_origin(&wrong_type), None);

        let mut wrong_role = line(&wake(PROJECT, THREAD));
        wrong_role["message"]["role"] = json!("assistant");
        assert_eq!(managed_origin(&wrong_role), None);
    }

    #[test]
    fn an_overlarge_managed_body_fails_closed_for_origin_only() {
        let mut log = LiveLog::default();
        let body = format!("{}{}", wake(PROJECT, THREAD), "x".repeat(1_048_576));
        feed(&mut log, &body);
        assert_eq!(
            log.claude_project_origin,
            Some(ClaudeProjectOrigin::Unknown)
        );
    }
}
