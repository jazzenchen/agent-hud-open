//! `list_sessions`: the sessions a message can go to, and which take one now.
use super::*;

impl Messaging {
    /// The Claude Code and DeepSeek Harness sessions whose plugin checked in
    /// lately.
    pub(crate) fn loaded(&self) -> std::collections::BTreeSet<String> {
        self.clients
            .targets(std::time::SystemTime::now())
            .into_iter()
            .filter(|row| {
                matches!(row.target.provider, Provider::Claude | Provider::DeepSeek)
                    && row.reachable == Some(true)
            })
            .map(|row| row.target.session_id)
            .collect()
    }
    pub(crate) fn list(&self, caller: Option<&Caller>, host: &Host<'_>) -> Value {
        let language = language(host.snapshot);
        let loaded = self.loaded();
        let mut rows = sessions(host.snapshot)
            .map(|session| {
                let id = session["id"].as_str().unwrap_or_default();
                let client = session["source"].as_str().unwrap_or_default();
                let (reachable, reason) =
                    self.reachable(client, id, &loaded, (session, host.snapshot), language);
                json!({
                    "id": id,
                    "client": client,
                    "title": session["title"],
                    "project": session["cwd"].as_str().or(session["project"].as_str()),
                    "state": state(session),
                    "reachable": reachable,
                    "reason": reason,
                    "self": caller.is_some_and(|caller| same_session(&caller.session_id, id)),
                    "updatedAt": session["updatedAt"],
                })
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| b["updatedAt"].as_str().cmp(&a["updatedAt"].as_str()));
        let starts = self
            .starts
            .offers_for(&all_sessions(host.snapshot))
            .into_iter()
            .map(|offer| {
                (
                    offer.provider.as_str().to_owned(),
                    json!({"folders":offer.folders,"withoutFolder":offer.without_folder,"app":offer.app}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        json!({"sessions":rows,"startFolders":starts,"passLimit":self.preferences.limit,"sendsAs":if self.preferences.as_user {"user"} else {"labelled"}})
    }

    /// Whether `id` takes a message now, and why not or how. `rows` is the
    /// session's snapshot row and the snapshot itself.
    pub(crate) fn reachable(
        &self,
        client: &str,
        id: &str,
        loaded: &std::collections::BTreeSet<String>,
        rows: (&Value, &Value),
        language: Language,
    ) -> (Value, String) {
        match client {
            "claude" if !self.preferences.claude => (json!(false), CLAUDE_MESSAGES_OFF.into()),
            "claude" if loaded.contains(id) => (json!(true), words::claude_ready(language)),
            "claude" => (json!(false), words::claude_not_loaded(language)),
            "deepseek" if !self.preferences.deepseek => {
                (json!(false), DEEPSEEK_MESSAGES_OFF.into())
            }
            "deepseek" if loaded.contains(id) => (json!(true), words::deepseek_ready(language)),
            "deepseek" => (json!(false), words::deepseek_not_loaded(language)),
            "antigravity" => (Value::Null, words::antigravity_waits(language)),
            // Pi and OpenCode: Agent HUD's observer in the client takes it.
            _ if Provider::parse(client).is_some_and(Provider::uses_observer) => {
                let name = words::client_name(client);
                if Provider::parse(client).is_some_and(|p| self.clients.hook_waiting(p, id)) {
                    (json!(true), words::observer_ready(language, name))
                } else {
                    (Value::Null, words::observer_closed(language, name))
                }
            }
            _ if Provider::parse(client).is_some_and(Provider::uses_hook_channel) => {
                let waiting = Provider::parse(client)
                    .is_some_and(|provider| self.clients.hook_waiting(provider, id));
                hook_reachable(client, rows, waiting, language)
            }
            // With neither a command line nor the shared daemon, nothing
            // can queue it, even into a thread an app has open.
            _ if !self.places.codex_command
                && !self
                    .places
                    .codex_data
                    .join("app-server-control/app-server-control.sock")
                    .exists() =>
            {
                (json!(false), words::codex_no_command(language))
            }
            _ if delivery::codex_thread_open(&self.places.codex_data, id) => {
                (json!(true), words::codex_open(language))
            }
            _ if delivery::codex_desktop_installed(&self.places.applications) => {
                (json!(true), words::codex_wakes(language))
            }
            _ => (Value::Null, words::codex_waits(language)),
        }
    }
}

/// A client whose own stop hook takes a message (`Provider::uses_hook_channel`):
/// it does so when one of the session's turns ends, and only once the
/// client is seen running Agent HUD's hooks (`hooks.at`), for this session or another of
/// the same client in the last week. Only a rewake hook waiting in the
/// session (`waiting`, Qoder CLI's) starts a turn in an idle one.
fn hook_reachable(
    client: &str,
    (session, snapshot): (&Value, &Value),
    waiting: bool,
    language: Language,
) -> (Value, String) {
    if waiting {
        return (json!(true), words::hook_waiting(language));
    }
    let week_ago = chrono::Utc::now() - chrono::Duration::days(7);
    let recent = |row: &Value| {
        row["hooks"]["at"]
            .as_str()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .is_some_and(|at| at >= week_ago)
    };
    let family = |source: &str| Provider::parse(source).map(Provider::as_str);
    let hooked = recent(session)
        || snapshot["sessions"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["source"].as_str().and_then(family) == family(client) && recent(row))
        });
    let name = words::client_name(client);
    if !hooked {
        return (json!(false), words::hook_unseen(language, name));
    }
    // The state the hooks saw: a client's live status may be off for display.
    if session["hooks"]["state"] == "running" || session["state"] == "running" {
        (json!(true), words::hook_running(language))
    } else if delivery::wakes_idle(client) {
        (Value::Null, words::hook_not_waiting(language, name))
    } else {
        (Value::Null, words::hook_idle(language, name))
    }
}

#[cfg(test)]
mod hook_tests {
    use super::*;

    #[test]
    fn a_hook_client_session_is_reachable_while_its_hooks_see_a_turn_run() {
        let now = chrono::Utc::now().to_rfc3339();
        // Live status off for display: the session reads idle and has no
        // live facts, but its hooks still say what they saw.
        let running = json!({"id":"kimi:session_a:main","source":"kimi","state":"idle","liveFacts":null,"hooks":{"at":now,"pids":[1],"state":"running"}});
        let idle = json!({"id":"kimi:session_b:main","source":"kimi","state":"idle","hooks":{"at":now,"pids":[2],"state":"idle"}});
        let unseen = json!({"id":"trae:c","source":"trae","state":"idle"});
        let snapshot = json!({"sessions":[running.clone(), idle.clone(), unseen.clone()]});
        assert_eq!(
            hook_reachable("kimi", (&running, &snapshot), false, Language::En).0,
            json!(true)
        );
        let (reachable, reason) = hook_reachable("kimi", (&idle, &snapshot), false, Language::En);
        assert_eq!(reachable, Value::Null);
        assert!(reason.contains("next turn"), "{reason}");
        let (reachable, reason) = hook_reachable("trae", (&unseen, &snapshot), false, Language::En);
        assert_eq!(reachable, json!(false));
        assert!(reason.contains("TRAE Settings → Hooks"), "{reason}");
        // Another session of the same client proves its hooks run.
        let fresh = json!({"id":"kimi:session_d:main","source":"kimi","state":"idle"});
        assert_eq!(
            hook_reachable("kimi", (&fresh, &snapshot), false, Language::En).0,
            Value::Null
        );
    }

    #[test]
    fn an_idle_qoder_session_is_reachable_while_a_rewake_hook_waits_in_it() {
        let now = chrono::Utc::now().to_rfc3339();
        let idle =
            json!({"id":"q1","source":"qoder","state":"idle","hooks":{"at":now,"state":"idle"}});
        let snapshot = json!({"sessions":[idle.clone()]});
        let (reachable, reason) = hook_reachable("qoder", (&idle, &snapshot), true, Language::En);
        assert_eq!(reachable, json!(true));
        assert!(reason.contains("at once"), "{reason}");
        // No waiter: it says why, and waits for the session's next turn.
        let (reachable, reason) = hook_reachable("qoder", (&idle, &snapshot), false, Language::En);
        assert_eq!(reachable, Value::Null);
        assert!(reason.contains("interactive"), "{reason}");
        assert!(reason.contains("next turn"), "{reason}");
    }
}
