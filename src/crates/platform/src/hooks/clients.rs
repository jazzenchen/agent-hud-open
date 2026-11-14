//! Explicit installation coordinator. Status is refreshed only at delegation or
//! a user setting action, never by the five-second snapshot projection.
use super::{install, observers, InstallContext};
use agent_hud_core::l10n::tr;
use serde_json::{json, Value};
#[derive(Default)]
pub(super) struct Clients {
    context: Option<InstallContext>,
    rows: Vec<Value>,
    notice: Option<String>,
    /// The clients whose opt-in hooks the user turned on.
    opted_in: Vec<String>,
    /// Whether the clients' hooks are in: the last install put them in.
    on: bool,
    /// Agent HUD's hook channel folder, written into the observer scripts
    /// whose mailbox hands their sessions messages (`observers::script`).
    channel: Option<std::path::PathBuf>,
}
/// Every hook the client has, in install order, each with whether it goes
/// in: one that waits for the user only once they turned it on.
fn flags(source: &str, opted_in: bool) -> Vec<(&'static str, bool)> {
    agent_hud_agents::hooks::hooks_by_id(source)
        .map(|hooks| hooks.all_flags())
        .unwrap_or_default()
        .into_iter()
        .map(|(flag, opt_in)| (flag, opted_in || !opt_in))
        .collect()
}
/// Whether the client's hooks have a way to launch on `os`. Where they have
/// none (Antigravity's on Windows) its settings are left alone, and that is
/// not a failure of the switch: the other clients' hooks still go in.
fn launches(source: &str, os: agent_hud_agents::Os) -> bool {
    agent_hud_agents::hooks::hooks_by_id(source)
        .is_some_and(|hooks| hooks.launch.on(os).value().is_some())
}
/// Whether the client reports its sessions through an observer plugin.
fn observed(source: &str) -> bool {
    agent_hud_agents::by_id(source).is_some_and(|agent| agent.observer.is_some())
}
/// The clients found on this computer, whose settings the hooks go into.
fn found(context: &InstallContext) -> Vec<String> {
    crate::discovery::discover_at(&context.home, &context.environment)
        .into_iter()
        .filter(|p| p.installed || p.configured || p.usage)
        .map(|p| p.id)
        .collect()
}
impl Clients {
    /// The installer for the clients, with the hook channel folder the
    /// observers' mailboxes read.
    pub(super) fn with_channel(channel: std::path::PathBuf) -> Self {
        Self {
            channel: Some(channel),
            ..Self::default()
        }
    }
    pub(super) fn context(&self) -> Option<&InstallContext> {
        self.context.as_ref()
    }
    pub fn delegate(&mut self, context: InstallContext) -> Result<(), String> {
        install::canonical(&context)?;
        self.context = Some(context);
        self.refresh(None);
        Ok(())
    }
    /// Puts in, or takes out, every found client's hooks and observer. A client that fails is left out and named in
    /// the notice; the others still go in, so one broken client folder never turns the switch off (Open #16). The
    /// error is only what stops every client.
    pub fn install(&mut self, enabled: bool) -> Result<(), String> {
        let context=self.context.as_ref().ok_or_else(|| tr!("No fixed hook-owner executable has been delegated; no client configuration was changed"))?;
        if enabled {
            install::canonical(context)?;
        }
        self.on = enabled;
        let mut errors = Vec::new();
        for source in found(context) {
            if observed(&source) {
                if let Err(error) =
                    observers::observer_plan(context, &source, enabled, self.channel.as_deref())
                        .and_then(|plan| observers::apply_observer_plan(&plan))
                {
                    errors.push(format!("{source}: {error}"));
                }
            }
            // Removing ours stays possible everywhere, the hooks that wait
            // for the user included; adding only where the hooks can launch,
            // and an opt-in hook only once the user turned it on.
            let hooked = !enabled || launches(&source, crate::os::OS);
            let opted_in = self.opted_in.contains(&source);
            // Every settings file, one per profile where the client keeps
            // them: a file that cannot be changed leaves the others to go in.
            for (flag, wanted) in flags(&source, opted_in).into_iter().filter(|_| hooked) {
                for plan in install::install_plans(context, &source, flag, enabled && wanted) {
                    if let Err(error) = plan.and_then(|plan| install::apply_plan(&plan)) {
                        errors.push(format!("{source}: {error}"));
                    }
                }
            }
        }
        self.refresh((!errors.is_empty()).then(|| errors.join("; ")));
        Ok(())
    }
    pub fn opted_in(&self) -> &[String] {
        &self.opted_in
    }
    /// Turns the opt-in hooks of the clients in `sources` on and every other
    /// client's off. While the hooks are in, a changed client's opt-in hooks
    /// go in or come out at once and its other hooks stay as they are; else
    /// the next install follows the list. When one cannot, the list and
    /// every file stay as they were and the error names the client.
    pub fn set_opted_in(&mut self, sources: Vec<String>) -> Result<(), String> {
        let previous = std::mem::replace(&mut self.opted_in, sources);
        let changed: Vec<String> = previous
            .iter()
            .chain(&self.opted_in)
            .filter(|source| previous.contains(source) != self.opted_in.contains(source))
            .cloned()
            .collect();
        if !self.on || changed.is_empty() {
            let notice = self.notice.take();
            self.refresh(notice);
            return Ok(());
        }
        let errors = self.put_opt_in(&changed);
        if !errors.is_empty() {
            // Puts back what went in for the other clients.
            self.opted_in = previous;
            self.put_opt_in(&changed);
        }
        let notice = self.notice.take();
        self.refresh(notice);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
    /// Puts each found client's opt-in hooks in, or takes them out, as the
    /// list says; the errors of those that could not.
    fn put_opt_in(&self, sources: &[String]) -> Vec<String> {
        let Some(context) = self.context.as_ref() else {
            return Vec::new();
        };
        let found = found(context);
        let mut errors = Vec::new();
        for source in sources {
            if !found.contains(source) || !launches(source, crate::os::OS) {
                continue;
            }
            let opted_in = self.opted_in.contains(source);
            let opt_in = agent_hud_agents::hooks::hooks_by_id(source)
                .map(|hooks| hooks.all_flags())
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, opt_in)| *opt_in);
            for (flag, _) in opt_in {
                for plan in install::install_plans(context, source, flag, opted_in) {
                    if let Err(error) = plan.and_then(|plan| install::apply_plan(&plan)) {
                        errors.push(format!("{source}: {error}"));
                    }
                }
            }
        }
        errors
    }
    fn refresh(&mut self, notice: Option<String>) {
        self.notice = notice;
        self.rows.clear();
        let Some(context) = self.context.as_ref() else {
            return;
        };
        for source in found(context) {
            let mut known = false;
            let mut installed = true;
            let mut error = None;
            if observed(&source) {
                known = true;
                match observers::observer_plan(context, &source, true, self.channel.as_deref()) {
                    Ok(plan) => installed &= !plan.changed,
                    Err(e) => {
                        installed = false;
                        error = Some(e);
                    }
                }
            }
            let hooked = launches(&source, crate::os::OS);
            let wanted = flags(&source, self.opted_in.contains(&source))
                .into_iter()
                .filter(|(_, wanted)| *wanted && hooked);
            for (flag, _) in wanted {
                known = true;
                for plan in install::install_plans(context, &source, flag, true) {
                    match plan {
                        Ok(plan) => installed &= !plan.changed,
                        Err(e) => {
                            installed = false;
                            error = Some(e);
                        }
                    }
                }
            }
            if known {
                // Installed hooks some clients run only after a step in the client.
                let notice = error.or_else(|| {
                    installed
                        .then(|| agent_hud_agents::hooks::setup_step(&source))
                        .flatten()
                });
                self.rows.push(
                    json!({"source":source,"installed":installed,"active":false,"notice":notice}),
                );
            }
        }
    }
    pub fn status(&self, enabled: bool, desired: bool) -> Value {
        let installed = self.rows.iter().any(|row| row["installed"] == true);
        let notice = self.notice.clone().or_else(|| {
            self.context.is_none().then(|| {
                let copy = crate::launched_executable()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                tr!(
                    "Only the installed Agent HUD can install client hooks; this copy is at {{copy}}. Client settings are unchanged.",
                    copy = copy
                )
            })
        });
        json!({"supported":true,"enabled":enabled,"desired":desired,"installed":installed,"installable":self.context.is_some(),"notice":notice,"clients":self.rows})
    }
}

#[cfg(test)]
mod tests {
    use agent_hud_agents::Os;

    #[test]
    fn hooks_go_in_only_where_the_client_can_launch_them() {
        // Antigravity's hooks run as a POSIX command, which Windows lacks.
        assert!(!super::launches("antigravity", Os::Windows));
        assert!(super::launches("antigravity", Os::MacOs));
        for client in ["claude", "codex"] {
            assert!(super::launches(client, Os::Windows), "{client}");
            assert!(super::launches(client, Os::MacOs), "{client}");
        }
        assert!(!super::launches("not-a-client", Os::MacOs));
    }

    #[test]
    fn copilots_approval_hook_goes_in_only_while_the_user_has_it_on() {
        use super::{Clients, InstallContext};
        use std::{collections::HashMap, fs};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        // Copilot is found by its own settings.
        fs::create_dir_all(home.join(".copilot")).unwrap(); // plain directory
        fs::write(home.join(".copilot/config.json"), "{}").unwrap();
        // A regular file passes the fixed-owner check; it is never launched.
        let executable = temp.path().join("fixed hook owner");
        fs::write(&executable, "fixture only").unwrap();
        let mut clients = Clients::default();
        clients
            .delegate(InstallContext {
                home: home.clone(),
                environment: HashMap::new(),
                canonical_executable: Some(executable.clone()),
                executable,
            })
            .unwrap();
        let file = home.join(".copilot/hooks/agent-hud.json");
        let events = || -> Vec<String> {
            let Ok(bytes) = fs::read(&file) else {
                return Vec::new();
            };
            let settings: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let mut events: Vec<String> = settings["hooks"]
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, handlers)| handlers.as_array().is_some_and(|h| !h.is_empty()))
                .map(|(event, _)| event.clone())
                .collect();
            events.sort();
            events
        };
        let copilot = || vec!["copilot".to_owned()];

        clients.install(true).unwrap();
        assert_eq!(events(), ["agentStop"]);
        // Turned on while hooks are in, it goes in at once; off, it comes out
        // and the completion hook stays.
        clients.set_opted_in(copilot()).unwrap();
        assert_eq!(events(), ["agentStop", "permissionRequest"]);
        let status = clients.status(true, true);
        let rows = status["clients"].as_array().unwrap();
        let row = rows.iter().find(|row| row["source"] == "copilot").unwrap();
        assert_eq!(row["installed"], true, "its row counts the approval hook");
        clients.set_opted_in(Vec::new()).unwrap();
        assert_eq!(events(), ["agentStop"]);

        // Turning client hooks off takes it out with the others; turned on
        // while they are off, it waits for them.
        clients.set_opted_in(copilot()).unwrap();
        clients.install(false).unwrap();
        assert!(events().is_empty());
        clients.set_opted_in(Vec::new()).unwrap();
        clients.set_opted_in(copilot()).unwrap();
        assert!(events().is_empty());
        clients.install(true).unwrap();
        assert_eq!(events(), ["agentStop", "permissionRequest"]);
        // Only a client whose hook waits for the user changes anything.
        let claude = || fs::read(home.join(".claude/settings.json")).ok();
        let before = claude();
        clients.set_opted_in(vec!["claude".to_owned()]).unwrap();
        assert_eq!(events(), ["agentStop"]);
        assert_eq!(claude(), before);
    }

    #[test]
    fn an_approval_hook_that_cannot_go_in_leaves_the_switch_as_it_was() {
        use super::{Clients, InstallContext};
        use std::{collections::HashMap, fs};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".copilot/hooks")).unwrap(); // plain directory
        fs::write(home.join(".copilot/config.json"), "{}").unwrap();
        let executable = temp.path().join("fixed hook owner");
        fs::write(&executable, "fixture only").unwrap();
        let mut clients = Clients::default();
        clients
            .delegate(InstallContext {
                home: home.clone(),
                environment: HashMap::new(),
                canonical_executable: Some(executable.clone()),
                executable,
            })
            .unwrap();
        clients.install(true).unwrap();
        let file = home.join(".copilot/hooks/agent-hud.json");
        // The user broke the file by hand after the hooks went in.
        fs::write(&file, "{ not json").unwrap();
        let error = clients
            .set_opted_in(vec!["copilot".to_owned()])
            .unwrap_err();
        assert!(error.starts_with("copilot: "), "{error}");
        assert!(clients.opted_in().is_empty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
    }
}
