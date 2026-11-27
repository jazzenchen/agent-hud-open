//! `--app-channel`: each request line goes to the running app and its answer
//! comes back, one thread a request, so a slow one holds up no other session
//! of a client that keeps one relay for all of them (Antigravity, DeepSeek
//! Harness).
//!
//! A call that sends (`send_message`, `start_session`) comes with the
//! relay's own name for it (`call`). The app may still be carrying it out
//! when the wait for its answer ends, so the relay then asks what became of
//! that call, and tells the agent what the app says: the message's own
//! receipt, or that nothing was sent (Open #118).
//!
//! Each request opens its own connection to the app, so an app that restarts
//! is reached again by the next one. While it cannot be asked, `host_tools`
//! is answered with the tools the app last listed and kept: a client that
//! starts a relay then (an update stops every relay, and Codex starts a new
//! one at once) reads the tools only that once.
use agent_hud_runtime::local_tools::{self, NO_ANSWER};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

/// How long after a sending call began the relay still asks what became of
/// it. The app's own senders give up within 120 s; with the last question's
/// own 40 s this stays under the MCP server's wait for such a call.
const SEND_WAIT: Duration = Duration::from_secs(100);
const NOT_SENT: &str =
    "Agent HUD did not get to this call in time, and nothing was sent. Send it again.";
const UNKNOWN: &str = "Agent HUD did not answer in time, and the message may still have gone out. Tell the user before sending it again.";

pub fn run(directory: &Path) -> io::Result<()> {
    let output = Arc::new(Mutex::new(io::stdout()));
    let directory = Arc::new(directory.to_owned());
    let mut answering = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let (output, directory) = (output.clone(), directory.clone());
        answering.retain(|thread: &std::thread::JoinHandle<()>| !thread.is_finished());
        answering.push(std::thread::spawn(move || {
            let response = match serde_json::from_str::<Value>(&line) {
                Ok(request) => or_saved(
                    &request,
                    answer(
                        &request,
                        &|request| local_tools::request(&directory, request),
                        SEND_WAIT,
                    ),
                    || local_tools::saved_host_tools(&directory),
                ),
                Err(_) => {
                    json!({"id":null,"error":{"code":"parse_error","message":"Expected one JSON request per line"}})
                }
            };
            let mut line = serde_json::to_vec(&response).unwrap_or_default();
            line.push(b'\n');
            let mut output = output.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = output.write_all(&line).and_then(|()| output.flush());
        }));
    }
    // The input ended: the requests already read are still answered.
    for thread in answering {
        let _ = thread.join();
    }
    Ok(())
}

/// `response`, or for `host_tools` the app did not answer, the tools it last
/// listed when it kept them.
fn or_saved(request: &Value, response: Value, saved: impl FnOnce() -> Option<Value>) -> Value {
    if request["method"] != "host_tools" || response.get("error").is_none() {
        return response;
    }
    match saved() {
        Some(tools) => json!({"id":response["id"],"result":tools}),
        None => response,
    }
}

/// The relay's name for `request` when it is a call that sends.
fn sending_call(request: &Value) -> Option<&str> {
    let params = &request["params"];
    (request["method"] == "host_tool"
        && matches!(
            params["name"].as_str(),
            Some("send_message" | "start_session")
        ))
    .then(|| params["call"].as_str())
    .flatten()
    .filter(|call| !call.is_empty())
}

/// The app's answer to `request`, asked through `ask`. A sending call whose
/// answer did not come is asked about by its name for `wait` after it began.
fn answer(request: &Value, ask: &dyn Fn(&Value) -> Result<Value, String>, wait: Duration) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let failed =
        |message: &str| json!({"id":id,"error":{"code":"tools_unavailable","message":message}});
    let began = Instant::now();
    let error = match ask(request) {
        Ok(response) => return response,
        Err(error) => error,
    };
    let Some(call) = sending_call(request) else {
        return failed(&error);
    };
    // Whether the request reached the app at all is known only when the
    // answer was what went missing.
    let unsure = || failed(if error == NO_ANSWER { UNKNOWN } else { &error });
    let question =
        json!({"id":id,"method":"host_tool","params":{"name":"call_result","call":call}});
    loop {
        match ask(&question) {
            Ok(response) => {
                return match response["result"]["found"].as_bool() {
                    Some(true) => json!({"id":id,"result":response["result"]["result"]}),
                    Some(false) => failed(NOT_SENT),
                    None => unsure(),
                }
            }
            // The call still runs, and the question waited behind it.
            Err(again) if again == NO_ANSWER && began.elapsed() < wait => {}
            Err(_) => return unsure(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn send() -> Value {
        json!({"id":7,"method":"host_tool","params":{"name":"send_message","call":"c-1","arguments":{"to":"s","text":"Hi"}}})
    }
    /// Answers each request with the next of `answers`, and keeps the requests.
    fn asked(
        request: &Value,
        answers: Vec<Result<Value, String>>,
        wait: Duration,
    ) -> (Value, Vec<Value>) {
        let answers = RefCell::new(answers.into_iter());
        let requests = RefCell::new(Vec::new());
        let response = answer(
            request,
            &|request| {
                requests.borrow_mut().push(request.clone());
                answers.borrow_mut().next().expect("asked once too often")
            },
            wait,
        );
        (response, requests.into_inner())
    }
    const LONG: Duration = Duration::from_secs(60);

    #[test]
    fn an_answer_that_comes_is_passed_on_as_it_is() {
        let (response, requests) = asked(
            &send(),
            vec![Ok(json!({"id":7,"result":{"id":"m-1"}}))],
            LONG,
        );
        assert_eq!(response, json!({"id":7,"result":{"id":"m-1"}}));
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn a_send_whose_answer_was_lost_answers_with_what_the_app_says_it_sent() {
        let receipt = json!({"delivered":true});
        let (response, requests) = asked(
            &send(),
            vec![
                Err(NO_ANSWER.into()),
                // The call still runs: the first question waits out too.
                Err(NO_ANSWER.into()),
                Ok(json!({"id":7,"result":{"found":true,"result":receipt}})),
            ],
            LONG,
        );
        assert_eq!(response, json!({"id":7,"result":receipt}));
        assert_eq!(
            requests[1],
            json!({"id":7,"method":"host_tool","params":{"name":"call_result","call":"c-1"}})
        );
        assert_eq!(requests.len(), 3);
    }

    #[test]
    fn a_send_the_app_never_began_says_nothing_was_sent() {
        let (response, _) = asked(
            &send(),
            vec![
                Err(NO_ANSWER.into()),
                Ok(json!({"id":7,"result":{"found":false}})),
            ],
            LONG,
        );
        assert_eq!(response["error"]["message"], NOT_SENT);
    }

    #[test]
    fn a_send_still_unanswered_when_the_wait_ends_says_it_may_have_gone_out() {
        let (response, requests) = asked(
            &send(),
            vec![Err(NO_ANSWER.into()), Err(NO_ANSWER.into())],
            Duration::ZERO,
        );
        assert_eq!(response["error"]["message"], UNKNOWN);
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn a_send_that_never_reached_the_app_keeps_its_own_error() {
        let off = "Agent HUD is not listening";
        let (response, _) = asked(&send(), vec![Err(off.into()), Err(off.into())], LONG);
        assert_eq!(response["error"]["message"], off);
        // The app went away after the request reached it.
        let (response, _) = asked(&send(), vec![Err(NO_ANSWER.into()), Err(off.into())], LONG);
        assert_eq!(response["error"]["message"], UNKNOWN);
        // An app that does not know the question, or refuses it.
        let (response, _) = asked(
            &send(),
            vec![
                Err(NO_ANSWER.into()),
                Ok(json!({"id":7,"error":{"message":"Unknown local application tool"}})),
            ],
            LONG,
        );
        assert_eq!(response["error"]["message"], UNKNOWN);
    }

    #[test]
    fn host_tools_the_app_cannot_answer_are_the_ones_it_last_kept() {
        let listing = json!({"id":3,"method":"host_tools"});
        let kept = json!({"tools":[{"name":"send_message"}]});
        let away = json!({"id":3,"error":{"code":"tools_unavailable","message":"off"}});
        assert_eq!(
            or_saved(&listing, away.clone(), || Some(kept.clone())),
            json!({"id":3,"result":kept})
        );
        // Nothing kept: the error stands.
        assert_eq!(or_saved(&listing, away.clone(), || None), away);
        // The app's own answer, even none, is never replaced.
        let none = json!({"id":3,"result":{"tools":[]}});
        assert_eq!(
            or_saved(&listing, none.clone(), || Some(kept.clone())),
            none
        );
        // Only the listing falls back: a call the app missed fails.
        let call = json!({"id":4,"method":"host_tool","params":{"name":"list_sessions"}});
        assert_eq!(or_saved(&call, away.clone(), || Some(kept.clone())), away);
    }

    #[test]
    fn a_call_that_sends_nothing_or_has_no_name_is_never_asked_about() {
        for request in [
            json!({"id":1,"method":"host_tool","params":{"name":"list_sessions","call":"c-1"}}),
            json!({"id":1,"method":"host_tool","params":{"name":"send_message"}}),
            json!({"id":1,"method":"snapshot"}),
        ] {
            let (response, requests) = asked(&request, vec![Err(NO_ANSWER.into())], LONG);
            assert_eq!(response["error"]["message"], NO_ANSWER);
            assert_eq!(requests.len(), 1);
        }
    }
}
