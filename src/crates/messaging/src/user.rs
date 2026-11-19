//! The person's own words from the HUD: what the clipboard window sends into
//! a Claude Code, Codex, Antigravity, DeepSeek Harness, Pi or OpenCode
//! session, or into a new one of any of them installed here. They go as the
//! person typed them (`PromptForm::Own`), with no label, no reply line and no
//! journal: the person reads the answer in the session itself. They do not
//! need agent messages switched on: the window offers each of these agents
//! the person has turned on in Agent HUD and, where it has one, whose own
//! "Send messages" switch is on (`user_client_off`); a Pi or OpenCode session
//! only while it is open in its client, where Agent HUD's observer hands the
//! words over. Words that go throw the paper plane from the person to the
//! session (`person_flight`).
use super::*;
use agent_hud_core::naming::surfaces::{Category, Surface, AGENT_HUD};

/// The clients the clipboard window sends to.
pub const USER_CLIENTS: [&str; 6] = [
    "claude",
    "codex",
    "antigravity",
    "deepseek",
    "pi",
    "opencode",
];

/// Where the person's words go: a session there is, or a new one of
/// `client` in `folder` (empty: a task folder of its own).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UserTarget {
    #[serde(rename_all = "camelCase")]
    Session { client: String, session_id: String },
    #[serde(rename_all = "camelCase")]
    New { client: String, folder: String },
}

impl Messaging {
    /// The sessions the person can send to, newest first, each with whether
    /// it takes words now and why not; and the folders each client starts a
    /// new session in. Only clients the window offers now are there.
    pub fn user_sessions(&mut self, host: &Host<'_>) -> Value {
        self.preferences = Preferences::read(&host.snapshot["settings"]);
        self.clients.update_targets(sessions_targets(host.snapshot));
        let language = language(host.snapshot);
        let loaded = self.loaded();
        let offered = |client: &str| {
            USER_CLIENTS.contains(&client)
                && self
                    .user_client_off(client, host.snapshot, language)
                    .is_none()
        };
        let mut rows = sessions(host.snapshot)
            .filter(|session| offered(session["source"].as_str().unwrap_or("")))
            .filter_map(|session| {
                let id = session["id"].as_str().unwrap_or_default();
                let client = session["source"].as_str().unwrap_or_default();
                let (reachable, reason) =
                    self.reachable(client, id, &loaded, (session, host.snapshot), language);
                // A Pi or OpenCode session no running client holds would only
                // keep the words for an hour: not offered.
                if Provider::parse(client).is_some_and(Provider::uses_observer) && reachable != true
                {
                    return None;
                }
                let folder =
                    started_in(session).or_else(|| session["cwd"].as_str().map(Into::into));
                Some(json!({
                    "id": id,
                    "client": client,
                    "title": session["title"],
                    "folder": folder.as_deref().or(session["project"].as_str()),
                    // The folder as the start offers write it, to start a
                    // new session beside this one.
                    "startFolder": folder
                        .as_deref()
                        .and_then(|folder| self.starts.offered_folder(folder)),
                    "state": state(session),
                    "reachable": reachable,
                    "reason": reason,
                    "updatedAt": session["updatedAt"],
                }))
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| b["updatedAt"].as_str().cmp(&a["updatedAt"].as_str()));
        let starts = self
            .starts
            .offers_for(&all_sessions(host.snapshot))
            .into_iter()
            .filter(|offer| offered(offer.provider.as_str()))
            .map(|offer| {
                json!({
                    "client": offer.provider.as_str(),
                    "folders": offer.folders,
                    "withoutFolder": offer.without_folder,
                })
            })
            .collect::<Vec<_>>();
        json!({"sessions": rows, "starts": starts})
    }

    /// Why the window does not offer `client` now, if it does not: the person
    /// turned the agent off in Agent HUD, or its own "Send messages" switch
    /// (Claude Code's and DeepSeek Harness's).
    fn user_client_off(
        &self,
        client: &str,
        snapshot: &Value,
        language: Language,
    ) -> Option<String> {
        if !agent_on(snapshot, client) {
            return Some(words::client_off(language, client));
        }
        match client {
            "claude" if !self.preferences.claude => Some(CLAUDE_MESSAGES_OFF.into()),
            "deepseek" if !self.preferences.deepseek => Some(DEEPSEEK_MESSAGES_OFF.into()),
            _ => None,
        }
    }

    /// Sends `text` to `target` as the person. Answers `{state, reason,
    /// sessionId}`, `state` being queued, taken, started, uncertain or
    /// refused.
    pub fn user_send(
        &mut self,
        target: &UserTarget,
        text: &str,
        host: &Host<'_>,
        now: i64,
    ) -> Result<Value> {
        self.preferences = Preferences::read(&host.snapshot["settings"]);
        let language = language(host.snapshot);
        if let Some(reason) = check_text(text, language) {
            return Ok(refused(&reason));
        }
        let surface = Surface::new(Category::Desktop, AGENT_HUD);
        match target {
            UserTarget::Session { client, session_id } => {
                self.user_into(client, session_id, text, &surface, host, now)
            }
            UserTarget::New { client, folder } => {
                self.user_start(client, folder, text, &surface, host, now)
            }
        }
    }

    fn user_into(
        &mut self,
        client: &str,
        session_id: &str,
        text: &str,
        surface: &Surface,
        host: &Host<'_>,
        now: i64,
    ) -> Result<Value> {
        let language = language(host.snapshot);
        if !USER_CLIENTS.contains(&client) {
            return Ok(refused(&words::other_client(language)));
        }
        if let Some(reason) = self.user_client_off(client, host.snapshot, language) {
            return Ok(refused(&reason));
        }
        self.clients.update_targets(sessions_targets(host.snapshot));
        let Some(session) = find_in(host.snapshot, client, session_id) else {
            return Ok(refused(&words::unknown(language)));
        };
        let id = session["id"].as_str().unwrap_or(session_id).to_owned();
        if let (Value::Bool(false), reason) = self.reachable(
            client,
            &id,
            &self.loaded(),
            (session, host.snapshot),
            language,
        ) {
            return Ok(refused(&reason));
        }
        let receipt = self.clients.send(SendRequest {
            provider: provider(client),
            session_id: id.clone(),
            request_id: message_id(),
            text: text.to_owned(),
            form: PromptForm::Own,
            // Into the session's own queue: never into a turn in progress.
            operation: Operation::Enqueue,
            expected_turn_id: None,
            model: None,
        });
        Ok(match receipt {
            Ok(receipt) => {
                self.clients.delivered(&receipt, surface, now);
                let state = delivered(receipt.state);
                if !matches!(state, "refused" | "expired") {
                    (self.fly)(person_flight(&party(host.snapshot, client, &id), false));
                }
                json!({
                    "state": state,
                    "reason": receipt.reason.as_deref().map(plain),
                    "sessionId": id,
                })
            }
            Err(error) => refused(&plain(&error.to_string())),
        })
    }

    fn user_start(
        &mut self,
        client: &str,
        folder: &str,
        text: &str,
        surface: &Surface,
        host: &Host<'_>,
        now: i64,
    ) -> Result<Value> {
        let language = language(host.snapshot);
        if !USER_CLIENTS.contains(&client) {
            return Ok(refused(&words::other_client(language)));
        }
        if let Some(reason) = self.user_client_off(client, host.snapshot, language) {
            return Ok(refused(&reason));
        }
        let provider = provider(client);
        let offers = self.starts.offers_for(&all_sessions(host.snapshot));
        let Some(offer) = offers.iter().find(|offer| offer.provider == provider) else {
            return Ok(refused(&if client == "deepseek" {
                words::deepseek_cannot_start(language)
            } else {
                words::cannot_start(language, client)
            }));
        };
        let offered = if folder.is_empty() {
            offer.without_folder
        } else {
            offer.folders.iter().any(|known| known == folder)
        };
        if !offered {
            return Ok(refused(&words::folder(language, folder, &offer.folders)));
        }
        let receipt = self.starts.start_with(
            StartRequest {
                provider,
                request_id: uuid::Uuid::new_v4().to_string(),
                text: text.to_owned(),
                folder: folder.to_owned(),
                title: Some(title_of(text)),
                model: None,
            },
            &offers,
        );
        Ok(match receipt {
            Ok(receipt) => {
                self.clients.started(&receipt, surface, now);
                if receipt.handoff_pending {
                    (self.follow)(receipt.clone());
                }
                let state = match receipt.state.as_str() {
                    "started" => "started",
                    "uncertain" => "uncertain",
                    _ => "refused",
                };
                if state != "refused" {
                    let newcomer = Party {
                        client: client.to_owned(),
                        session_id: receipt.session_id.clone().unwrap_or_default(),
                        title: None,
                    };
                    (self.fly)(person_flight(&newcomer, true));
                }
                json!({
                    "state": state,
                    "reason": receipt.reason.as_deref().map(plain),
                    "sessionId": receipt.session_id,
                })
            }
            Err(error) => refused(&plain(&error.to_string())),
        })
    }
}

/// A new session's title: the words' first line that says something, at
/// most 80 characters.
fn title_of(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    line.chars().take(80).collect()
}

/// The folder a Claude Code session was started in, which the window files
/// it under: a `cd` inside the session moves its cwd, but not its log, which
/// Claude keeps in a folder named after the start (every mark but letters
/// and digits as `-`). The cwd or the folder above it that has that name.
fn started_in(session: &Value) -> Option<String> {
    if session["source"] != "claude" {
        return None;
    }
    let cwd = session["cwd"].as_str()?;
    let named = std::path::Path::new(session["logPath"].as_str()?)
        .parent()?
        .file_name()?
        .to_str()?;
    let mark = |path: &str| -> String {
        path.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect()
    };
    std::path::Path::new(cwd)
        .ancestors()
        .filter_map(|path| path.to_str())
        .find(|path| mark(path) == named)
        .map(Into::into)
}

#[cfg(test)]
mod user_tests {
    use super::*;

    #[test]
    fn a_target_reads_as_the_window_writes_it() {
        let session: UserTarget = serde_json::from_value(json!({
            "kind": "session", "client": "codex", "sessionId": "abc"
        }))
        .unwrap();
        assert_eq!(
            session,
            UserTarget::Session {
                client: "codex".into(),
                session_id: "abc".into()
            }
        );
        let new: UserTarget =
            serde_json::from_value(json!({"kind": "new", "client": "claude", "folder": ""}))
                .unwrap();
        assert_eq!(
            new,
            UserTarget::New {
                client: "claude".into(),
                folder: String::new()
            }
        );
    }

    #[test]
    fn a_claude_session_is_filed_where_it_started_not_where_it_went() {
        let session = |cwd: &str| {
            json!({
                "source": "claude",
                "cwd": cwd,
                "logPath": "/home/u/.claude/projects/-work-agent-hud-all/f95d.jsonl",
            })
        };
        assert_eq!(
            started_in(&session("/work/agent-hud-all/agent-hud-open/src")).as_deref(),
            Some("/work/agent-hud-all")
        );
        assert_eq!(
            started_in(&session("/work/agent-hud-all")).as_deref(),
            Some("/work/agent-hud-all")
        );
        assert_eq!(started_in(&session("/elsewhere")), None);
        let mut codex = session("/work/agent-hud-all/x");
        codex["source"] = json!("codex");
        assert_eq!(started_in(&codex), None);
    }

    #[test]
    fn a_title_is_the_first_line_that_says_something() {
        assert_eq!(
            title_of("\n  \n  Fix the toggle row  \nmore"),
            "Fix the toggle row"
        );
        assert_eq!(title_of(&"x".repeat(200)).chars().count(), 80);
    }
}
