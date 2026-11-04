//! Transient parser for Cloud decision-card events.
//!
//! Question, option, consequence, and message text are parsed only long enough
//! to validate the event. The retained representation contains IDs and counts.
use serde::{de::IgnoredAny, Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap;

const MAX_ID_CHARS: usize = 256;
const MAX_QUESTION_CHARS: usize = 1200;
const MAX_LABEL_CHARS: usize = 40;
const MAX_CONSEQUENCE_CHARS: usize = 800;

pub(super) struct Message {
    pub(super) id: String,
    pub(super) server_notice: bool,
    event: Event,
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawMessage {
            id: String,
            #[serde(default)]
            server_notice: bool,
            #[serde(default)]
            body: Body,
        }

        let raw = RawMessage::deserialize(deserializer)?;
        Ok(Self {
            id: raw.id,
            server_notice: raw.server_notice,
            event: raw.body.0,
        })
    }
}

#[derive(Default)]
struct Body(Event);

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let body = Value::deserialize(deserializer)?;
        Ok(Self(parse_body(body)))
    }
}

#[derive(Default)]
enum Event {
    #[default]
    Other,
    Question {
        option_count: usize,
    },
    Answer(Answer),
    Malformed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Choice(usize),
    Withdrawn,
}

struct Answer {
    decision_id: String,
    option_index: Option<usize>,
    close: Option<String>,
    closed_by: Option<String>,
}

#[derive(Deserialize)]
struct AskDecision {
    decision: Option<Decision>,
}

#[derive(Deserialize)]
struct Decision {
    question: Option<String>,
    options: Option<Vec<OptionText>>,
}

#[derive(Deserialize)]
struct OptionText {
    label: Option<String>,
    consequence: Option<String>,
    #[serde(flatten)]
    ignored: HashMap<String, IgnoredAny>,
}

#[derive(Deserialize)]
struct Decide {
    decision_id: Option<String>,
    option_index: Option<usize>,
    close: Option<String>,
    closed_by: Option<String>,
    #[serde(flatten)]
    ignored: HashMap<String, IgnoredAny>,
}

fn parse_body(body: Value) -> Event {
    let body = match body {
        Value::String(encoded) => match serde_json::from_str::<Value>(&encoded) {
            Ok(body) => body,
            Err(_) => return Event::Other,
        },
        body => body,
    };
    let Some(object) = body.as_object() else {
        return Event::Other;
    };
    if object.get("kind").and_then(Value::as_str) != Some("activity") {
        return Event::Other;
    }
    match object.get("tool").and_then(Value::as_str) {
        Some("ask_decision") => parse_question(body),
        Some("decide") => parse_answer(body),
        _ => Event::Other,
    }
}

fn parse_question(body: Value) -> Event {
    let Ok(raw) = serde_json::from_value::<AskDecision>(body) else {
        return Event::Malformed;
    };
    let Some(decision) = raw.decision else {
        return Event::Malformed;
    };
    let Some(question) = decision.question else {
        return Event::Malformed;
    };
    let Some(options) = decision.options else {
        return Event::Malformed;
    };
    let count = options.len();
    let valid = !question.trim().is_empty()
        && question.chars().count() <= MAX_QUESTION_CHARS
        && (2..=4).contains(&count)
        && options.iter().all(|option| {
            option.label.as_ref().is_some_and(|label| {
                !label.trim().is_empty() && label.chars().count() <= MAX_LABEL_CHARS
            }) && option
                .consequence
                .as_ref()
                .is_none_or(|consequence| consequence.chars().count() <= MAX_CONSEQUENCE_CHARS)
        });
    // Explicitly consume ignored extension fields; never retain their contents.
    drop(
        options
            .into_iter()
            .map(|option| option.ignored)
            .collect::<Vec<_>>(),
    );
    if valid {
        Event::Question {
            option_count: count,
        }
    } else {
        Event::Malformed
    }
}

fn parse_answer(body: Value) -> Event {
    let Ok(raw) = serde_json::from_value::<Decide>(body) else {
        return Event::Malformed;
    };
    let Some(decision_id) = raw.decision_id.filter(|id| valid_id(id)) else {
        return Event::Malformed;
    };
    let _ignored = raw.ignored;
    Event::Answer(Answer {
        decision_id,
        option_index: raw.option_index,
        close: raw.close,
        closed_by: raw.closed_by,
    })
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty()
        && id.chars().count() <= MAX_ID_CHARS
        && !id.chars().any(char::is_whitespace)
}

fn resolution(answer: &Answer, option_count: usize) -> Result<Resolution, ()> {
    if answer
        .option_index
        .is_some_and(|index| index >= option_count)
    {
        return Err(());
    }
    match (answer.closed_by.as_deref(), answer.close.as_deref()) {
        (Some("session"), Some("withdrawn")) => Ok(Resolution::Withdrawn),
        (None, None) => answer.option_index.map(Resolution::Choice).ok_or(()),
        (Some("session"), Some("answered")) => {
            answer.option_index.map(Resolution::Choice).ok_or(())
        }
        _ => Err(()),
    }
}

/// Whether any well-formed decision question remains unanswered.
///
/// Answers are joined by decision identity after all messages have been read,
/// so their order in paginated input does not affect the result.
pub(super) fn pending(messages: &[Message]) -> Result<bool, ()> {
    let mut questions = HashMap::<String, usize>::new();
    let mut answers = Vec::<Answer>::new();
    for message in messages {
        // A user can paste JSON that resembles an activity event. Only server
        // notices participate in the thread's decision status.
        if !message.server_notice {
            continue;
        }
        match &message.event {
            Event::Other => {}
            Event::Malformed => return Err(()),
            Event::Question { option_count } => {
                if !valid_id(&message.id) {
                    return Err(());
                }
                if questions
                    .insert(message.id.clone(), *option_count)
                    .is_some()
                {
                    return Err(());
                }
            }
            Event::Answer(answer) => {
                answers.push(Answer {
                    decision_id: answer.decision_id.clone(),
                    option_index: answer.option_index,
                    close: answer.close.clone(),
                    closed_by: answer.closed_by.clone(),
                });
            }
        }
    }

    let mut resolved = HashMap::<String, Resolution>::new();
    for answer in answers {
        let Some(option_count) = questions.get(&answer.decision_id).copied() else {
            return Err(());
        };
        let result = resolution(&answer, option_count)?;
        if resolved
            .insert(answer.decision_id, result)
            .is_some_and(|previous| previous != result)
        {
            return Err(());
        }
    }
    Ok(questions
        .keys()
        .any(|question_id| !resolved.contains_key(question_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn question(id: &str) -> Value {
        json!({
            "id": id,
            "server_notice": true,
            "body": {
                "kind": "activity",
                "tool": "ask_decision",
                "decision": {
                    "question": "Synthetic test question?",
                    "options": [
                        {"label":"Synthetic A","consequence":"Synthetic consequence A"},
                        {"label":"Synthetic B","consequence":"Synthetic consequence B"}
                    ]
                },
                "text":"Synthetic decision prompt"
            }
        })
    }

    fn answer(id: &str, option_index: Option<usize>) -> Value {
        let mut body = json!({"kind":"activity","tool":"decide","decision_id":id});
        if let Some(option_index) = option_index {
            body["option_index"] = json!(option_index);
        }
        json!({"id":"synthetic-answer","server_notice":true,"body":body})
    }

    fn messages(values: &[Value]) -> Vec<Message> {
        values
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn an_unanswered_question_is_pending_and_a_tap_answer_resolves_by_id() {
        let open = messages(&[question("decision-a")]);
        assert_eq!(pending(&open), Ok(true));

        let answered = messages(&[answer("decision-a", Some(1)), question("decision-a")]);
        assert_eq!(pending(&answered), Ok(false));

        let mut encoded = question("decision-a");
        encoded["body"] = json!(encoded["body"].to_string());
        assert_eq!(pending(&messages(&[encoded])), Ok(true));
    }

    #[test]
    fn session_answer_and_withdrawal_are_terminal() {
        let mut closed = answer("decision-a", Some(0));
        closed["body"]["closed_by"] = json!("session");
        closed["body"]["close"] = json!("answered");
        assert_eq!(
            pending(&messages(&[question("decision-a"), closed])),
            Ok(false)
        );

        let mut withdrawn = answer("decision-a", None);
        withdrawn["body"]["closed_by"] = json!("session");
        withdrawn["body"]["close"] = json!("withdrawn");
        assert_eq!(
            pending(&messages(&[question("decision-a"), withdrawn])),
            Ok(false)
        );
    }

    #[test]
    fn questions_and_answers_join_by_identity_independent_of_input_order() {
        let input = [
            answer("decision-b", Some(0)),
            question("decision-a"),
            answer("decision-a", Some(1)),
            question("decision-b"),
        ];
        assert_eq!(pending(&messages(&input)), Ok(false));

        let one_open = [
            answer("decision-b", Some(0)),
            question("decision-a"),
            question("decision-b"),
        ];
        assert_eq!(pending(&messages(&one_open)), Ok(true));
    }

    #[test]
    fn malformed_or_unknown_recognized_events_fail_closed() {
        let mut malformed = question("decision-a");
        malformed["body"]["decision"]["options"] = json!([{"label":"Only one"}]);
        assert_eq!(pending(&messages(&[malformed])), Err(()));
        assert_eq!(pending(&messages(&[answer("unknown", Some(0))])), Err(()));
        assert_eq!(
            pending(&messages(&[
                question("decision-a"),
                answer("decision-a", Some(2))
            ])),
            Err(())
        );

        let mut long_label = question("decision-label");
        long_label["body"]["decision"]["options"][0]["label"] = json!("L".repeat(41));
        assert_eq!(pending(&messages(&[long_label])), Err(()));
        let mut long_consequence = question("decision-consequence");
        long_consequence["body"]["decision"]["options"][0]["consequence"] = json!("C".repeat(801));
        assert_eq!(pending(&messages(&[long_consequence])), Err(()));

        let mut unknown_close = answer("decision-a", Some(0));
        unknown_close["body"]["closed_by"] = json!("unknown");
        unknown_close["body"]["close"] = json!("answered");
        assert_eq!(
            pending(&messages(&[question("decision-a"), unknown_close])),
            Err(())
        );

        let not_a_notice = json!({"id":"decision-a","body":question("decision-a")["body"]});
        assert_eq!(pending(&messages(&[not_a_notice])), Ok(false));
    }

    #[test]
    fn unrecognized_messages_do_not_create_decisions() {
        let messages = messages(&[
            json!({"id":"ordinary","body":"ordinary message"}),
            json!({"id":"other","server_notice":true,"body":{"kind":"activity","tool":"other"}}),
        ]);
        assert_eq!(pending(&messages), Ok(false));
    }

    #[test]
    fn user_messages_that_contain_decision_shaped_json_are_ignored() {
        let mut pasted = question("synthetic-pasted-id");
        pasted["server_notice"] = json!(false);
        let malformed = json!({
            "id":"synthetic-malformed-user-message",
            "server_notice":false,
            "body":{"kind":"activity","tool":"ask_decision","decision":{"options":[]}}
        });
        assert_eq!(pending(&messages(&[pasted, malformed])), Ok(false));
    }
}
