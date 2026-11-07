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
        let zh = language(host.snapshot);
        let loaded = self.loaded();
        let mut rows = sessions(host.snapshot)
            .map(|session| {
                let id = session["id"].as_str().unwrap_or_default();
                let client = session["source"].as_str().unwrap_or_default();
                let (reachable, reason) = self.reachable(client, id, &loaded, zh);
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

    /// Whether `id` takes a message now, and why not or how.
    pub(crate) fn reachable(
        &self,
        client: &str,
        id: &str,
        loaded: &std::collections::BTreeSet<String>,
        zh: bool,
    ) -> (Value, String) {
        match client {
            "claude" if !self.preferences.claude => (json!(false), CLAUDE_MESSAGES_OFF.into()),
            "claude" if loaded.contains(id) => (json!(true), words::claude_ready(zh)),
            "claude" => (json!(false), words::claude_not_loaded(zh)),
            "deepseek" if !self.preferences.deepseek => {
                (json!(false), DEEPSEEK_MESSAGES_OFF.into())
            }
            "deepseek" if loaded.contains(id) => (json!(true), words::deepseek_ready(zh)),
            "deepseek" => (json!(false), words::deepseek_not_loaded(zh)),
            "antigravity" => (Value::Null, words::antigravity_waits(zh)),
            // With neither a command line nor the shared daemon, nothing
            // can queue it, even into a thread an app has open.
            _ if !self.places.codex_command
                && !self
                    .places
                    .codex_data
                    .join("app-server-control/app-server-control.sock")
                    .exists() =>
            {
                (json!(false), words::codex_no_command(zh))
            }
            _ if delivery::codex_thread_open(&self.places.codex_data, id) => {
                (json!(true), words::codex_open(zh))
            }
            _ if delivery::codex_desktop_installed(&self.places.applications) => {
                (json!(true), words::codex_wakes(zh))
            }
            _ => (Value::Null, words::codex_waits(zh)),
        }
    }
}
