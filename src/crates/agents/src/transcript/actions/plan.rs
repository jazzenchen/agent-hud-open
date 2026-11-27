//! The agent's plan: the changes a call makes to it, each client's way of
//! writing them, and the plan as it stands after them (see `super`).
use super::{clip, text_of, PLAN_STEPS, STEP_BYTES};
use serde_json::{json, Value};

/// The plan as it stands after the changes applied so far.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Plan {
    /// Each step's number (as its client counts them, from 1), text and status.
    steps: Vec<(String, String, &'static str)>,
    next: u64,
}

impl Plan {
    /// Applies a `"plan"` action's change. False when it changed nothing: a
    /// step moved on in a plan this never saw whole, say.
    pub fn apply(&mut self, change: &Value) -> bool {
        let before = self.clone();
        if let Some(steps) = change["set"].as_array() {
            self.steps = steps
                .iter()
                .enumerate()
                .filter_map(|(index, step)| {
                    Some((
                        (index + 1).to_string(),
                        step["text"].as_str()?.to_owned(),
                        status(step["status"].as_str().unwrap_or("pending"))?,
                    ))
                })
                .collect();
            self.next = self.steps.len() as u64;
            return true;
        }
        if change["advance"] == true {
            if let Some(step) = self.steps.iter_mut().find(|step| step.2 == "active") {
                step.2 = "done";
            }
            if let Some(step) = self.steps.iter_mut().find(|step| step.2 == "pending") {
                step.2 = "active";
            }
        }
        for place in change["done"].as_array().into_iter().flatten() {
            if let Some(step) = self.place(place) {
                step.2 = "done";
            }
        }
        if let Some(step) = self.place(&change["start"]) {
            step.2 = "active";
        }
        if let Some(text) = change["add"].as_str() {
            self.next += 1;
            if self.steps.len() < PLAN_STEPS {
                self.steps
                    .push((self.next.to_string(), text.to_owned(), "pending"));
            }
        }
        let update = &change["update"];
        if let Some(id) = update["id"].as_str() {
            if update["status"] == "removed" {
                self.steps.retain(|step| step.0 != id);
            } else if let Some(step) = self.steps.iter_mut().find(|step| step.0 == id) {
                if let Some(new) = update["status"].as_str().and_then(status) {
                    step.2 = new;
                }
                if let Some(text) = update["text"].as_str() {
                    step.1 = text.to_owned();
                }
            }
        }
        *self != before
    }

    fn place(&mut self, place: &Value) -> Option<&mut (String, String, &'static str)> {
        let place = place.as_u64()? as usize;
        self.steps.get_mut(place.checked_sub(1)?)
    }

    /// The number the step just added was given here, counted from the
    /// plan's start: a client's own number can differ when the start of the
    /// session is not read.
    pub fn added(&self) -> String {
        self.next.to_string()
    }

    /// Gives the step numbered `given` here the number its client gave it,
    /// and counts the steps added after it from there.
    pub fn number(&mut self, given: &str, number: &str) {
        let Ok(count) = number.parse::<u64>() else {
            return;
        };
        if given == number || self.steps.iter().any(|step| step.0 == number) {
            return;
        }
        if let Some(step) = self.steps.iter_mut().find(|step| step.0 == given) {
            step.0 = number.to_owned();
            self.next = self.next.max(count);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The steps in order, each `{"text","status"}`.
    pub fn steps(&self) -> Value {
        self.steps
            .iter()
            .map(|(_, text, status)| json!({"text":text,"status":status}))
            .collect()
    }
}

/// A client's word for a step's status as this module's; None for a word no
/// client is known to write.
fn status(word: &str) -> Option<&'static str> {
    match word {
        "pending" | "todo" | "not_started" => Some("pending"),
        "in_progress" | "active" | "in-progress" | "doing" => Some("active"),
        "completed" | "done" | "complete" => Some("done"),
        _ => None,
    }
}

/// A whole plan from a list of steps, whichever of the known spellings each
/// step uses for its text.
pub(super) fn plan_set(steps: &Value) -> Vec<Value> {
    let Some(steps) = steps.as_array() else {
        return Vec::new();
    };
    let steps = steps
        .iter()
        .take(PLAN_STEPS)
        .filter_map(|step| {
            let text = text_of(step, &["content", "step", "subject", "text", "title"])?;
            let status = status(step["status"].as_str().unwrap_or("pending")).unwrap_or("pending");
            Some(json!({"text":clip(text, STEP_BYTES),"status":status}))
        })
        .collect::<Vec<_>>();
    vec![json!({"plan":{"set":steps}})]
}

/// Codex's `update_plan` hands over the whole plan (`plan`); the Claude
/// desktop app's hands it over (`items`) or moves along the one it has.
pub(super) fn update_plan(arguments: &Value) -> Vec<Value> {
    for key in ["plan", "items"] {
        if arguments[key].is_array() {
            return plan_set(&arguments[key]);
        }
    }
    let mut change = json!({});
    if arguments["advance"] == true {
        change["advance"] = json!(true);
    }
    if let Some(done) = arguments["done"].as_array().filter(|done| !done.is_empty()) {
        change["done"] = json!(done);
    }
    if arguments["start"].is_u64() {
        change["start"] = arguments["start"].clone();
    }
    if change.as_object().is_some_and(|change| change.is_empty()) {
        return Vec::new();
    }
    vec![json!({"plan":change})]
}

pub(super) fn task_update(arguments: &Value) -> Vec<Value> {
    let Some(id) = arguments["taskId"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| arguments["taskId"].as_u64().map(|id| id.to_string()))
    else {
        return Vec::new();
    };
    let mut update = json!({"id":id});
    match arguments["status"].as_str() {
        Some("deleted") => update["status"] = json!("removed"),
        Some(word) => {
            if let Some(status) = status(word) {
                update["status"] = json!(status);
            }
        }
        None => {}
    }
    if let Some(text) = arguments["subject"].as_str() {
        update["text"] = json!(clip(text, STEP_BYTES));
    }
    if update.as_object().is_some_and(|update| update.len() == 1) {
        return Vec::new();
    }
    vec![json!({"plan":{"update":update}})]
}
