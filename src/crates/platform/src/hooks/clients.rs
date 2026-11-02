//! Explicit installation coordinator. Status is refreshed only at delegation or
//! a user setting action, never by the five-second snapshot projection.
use super::{install, observers, InstallContext};
use agent_hud_core::l10n;
use serde_json::{json, Value};
#[derive(Default)]
pub(super) struct Clients {
    context: Option<InstallContext>,
    rows: Vec<Value>,
    notice: Option<String>,
}
/// The hooks a client is installed with, from its descriptor.
fn flags(source: &str) -> Vec<&'static str> {
    agent_hud_agents::hooks::hooks_by_id(source)
        .map(|hooks| hooks.flags())
        .unwrap_or_default()
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
impl Clients {
    pub(super) fn context(&self) -> Option<&InstallContext> {
        self.context.as_ref()
    }
    pub fn delegate(&mut self, context: InstallContext) -> Result<(), String> {
        install::canonical(&context)?;
        self.context = Some(context);
        self.refresh(Vec::new());
        Ok(())
    }
    pub fn install(&mut self, enabled: bool) -> Result<(), String> {
        let context=self.context.as_ref().ok_or_else(|| l10n::text("这个版本没有指定固定的回调程序，客户端配置未作改动", "No fixed hook-owner executable has been delegated; no client configuration was changed"))?;
        let sources = crate::discovery::discover_at(&context.home, &context.environment)
            .into_iter()
            .filter(|p| p.installed || p.configured || p.usage)
            .map(|p| p.id)
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for source in sources {
            if observed(&source) {
                if let Err(error) = observers::observer_plan(context, &source, enabled)
                    .and_then(|plan| observers::apply_observer_plan(&plan))
                {
                    errors.push((source.clone(), error));
                }
            }
            // Removing ours stays possible everywhere; adding only where the
            // hooks can launch.
            let hooked = !enabled || launches(&source, crate::os::OS);
            for flag in flags(&source).into_iter().filter(|_| hooked) {
                if let Err(error) = install::install_plan(context, &source, flag, enabled)
                    .and_then(|plan| install::apply_plan(&plan))
                {
                    errors.push((source.clone(), error));
                }
            }
        }
        // A client's optional adapter must not stop the other clients or
        // prevent the app from launching. Keep its failure in Settings.
        self.refresh(errors);
        Ok(())
    }
    fn refresh(&mut self, errors: Vec<(String, String)>) {
        self.notice = (!errors.is_empty()).then(|| {
            errors
                .iter()
                .map(|(source, error)| format!("{source}: {error}"))
                .collect::<Vec<_>>()
                .join("; ")
        });
        self.rows.clear();
        let Some(context) = self.context.as_ref() else {
            return;
        };
        for source in crate::discovery::discover_at(&context.home, &context.environment)
            .into_iter()
            .filter(|p| p.installed || p.configured || p.usage)
            .map(|p| p.id)
        {
            let mut known = false;
            // Planning again cannot recover an error that occurred only
            // while applying a plan (for example, a failed write).
            let mut error = errors
                .iter()
                .find(|(client, _)| client == &source)
                .map(|(_, error)| error.clone());
            let mut installed = error.is_none();
            if observed(&source) {
                known = true;
                match observers::observer_plan(context, &source, true) {
                    Ok(plan) => installed &= !plan.changed,
                    Err(e) => {
                        installed = false;
                        error = Some(e);
                    }
                }
            }
            let hooked = launches(&source, crate::os::OS);
            for flag in flags(&source).into_iter().filter(|_| hooked) {
                known = true;
                match install::install_plan(context, &source, flag, true) {
                    Ok(plan) => installed &= !plan.changed,
                    Err(e) => {
                        installed = false;
                        error = Some(e);
                    }
                }
            }
            if known {
                self.rows.push(
                    json!({"source":source,"installed":installed,"active":false,"notice":error}),
                );
            }
        }
    }
    pub fn status(&self, enabled: bool, desired: bool) -> Value {
        let installed = self.rows.iter().any(|row| row["installed"] == true);
        let notice=self.notice.clone().or_else(||if self.context.is_none(){Some(l10n::text("这个版本没有指定固定的回调程序。接收回调不会安装客户端回调，客户端配置未作改动。", "No fixed hook-owner executable has been delegated. Listening does not install client hooks; client configuration is unchanged."))}else{None});
        json!({"supported":super::supported(),"enabled":enabled,"desired":desired,"installed":installed,"installable":self.context.is_some(),"notice":notice,"clients":self.rows})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_hud_agents::Os;
    use std::{collections::HashMap, fs, path::Path};

    fn context(home: &Path) -> InstallContext {
        // This fixed-owner fixture is checked for identity, never executed.
        let executable = home.join("fixed hook owner");
        fs::write(&executable, b"fixture only; never execute").unwrap();
        InstallContext {
            home: home.to_owned(),
            environment: HashMap::new(),
            canonical_executable: Some(executable.clone()),
            executable,
        }
    }

    fn row<'a>(status: &'a Value, source: &str) -> &'a Value {
        status["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["source"] == source)
            .unwrap()
    }

    #[test]
    fn client_install_failures_leave_notices_and_allow_other_clients_to_install() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".config")).unwrap(); // plain directory
                                                           // Discover OpenCode independently of its unusable configuration path.
        fs::create_dir_all(home.join(".local/share/opencode")).unwrap(); // plain directory
        let opencode = home.join(".config/opencode");
        fs::write(&opencode, b"a regular file, not a directory").unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap(); // plain directory
        let claude = home.join(".claude/settings.json");
        fs::write(&claude, b"invalid JSON").unwrap();
        fs::create_dir_all(home.join(".pi/agent/sessions")).unwrap(); // plain directory

        let mut clients = Clients::default();
        clients.delegate(context(&home)).unwrap();
        clients.install(true).unwrap();

        assert_eq!(
            fs::read(&opencode).unwrap(),
            b"a regular file, not a directory"
        );
        assert_eq!(fs::read(&claude).unwrap(), b"invalid JSON");
        assert_eq!(
            fs::read(home.join(".pi/agent/extensions/agent-hud.ts")).unwrap(),
            agent_hud_agents::pi::AGENT
                .observer
                .unwrap()
                .body
                .as_bytes()
        );
        let status = clients.status(true, true);
        assert_eq!(row(&status, "pi")["installed"], true);
        for source in ["opencode", "claude"] {
            let failed = row(&status, source);
            assert_eq!(failed["installed"], false);
            let notice = failed["notice"].as_str().unwrap();
            assert!(!notice.is_empty());
            assert!(status["notice"].as_str().unwrap().contains(notice));
        }
    }

    #[test]
    fn an_apply_failure_stays_visible_even_when_the_next_plan_is_current() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".pi/agent/sessions")).unwrap(); // plain directory
        let mut clients = Clients::default();
        clients.delegate(context(&home)).unwrap();
        clients.install(true).unwrap();
        assert_eq!(row(&clients.status(true, true), "pi")["installed"], true);

        let error = "Observer file could not be committed";
        clients.refresh(vec![("pi".into(), error.into())]);
        let status = clients.status(true, true);
        assert_eq!(row(&status, "pi")["installed"], false);
        assert_eq!(row(&status, "pi")["notice"], error);
        assert!(status["notice"].as_str().unwrap().contains(error));
    }

    #[test]
    fn installation_without_a_delegated_owner_remains_an_error() {
        let mut clients = Clients::default();
        assert!(clients.install(true).is_err());
        assert_eq!(clients.status(false, true)["installable"], false);
    }

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
}
