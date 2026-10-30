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
        self.refresh(None);
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
                    errors.push(format!("{source}: {error}"));
                }
            }
            for flag in flags(&source) {
                if let Err(error) = install::install_plan(context, &source, flag, enabled)
                    .and_then(|plan| install::apply_plan(&plan))
                {
                    errors.push(format!("{source}: {error}"));
                }
            }
        }
        let notice = (!errors.is_empty()).then(|| errors.join("; "));
        self.refresh(notice.clone());
        if let Some(notice) = notice {
            Err(notice)
        } else {
            Ok(())
        }
    }
    fn refresh(&mut self, notice: Option<String>) {
        self.notice = notice;
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
            let mut installed = true;
            let mut error = None;
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
            for flag in flags(&source) {
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
