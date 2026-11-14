use agent_hud_core::l10n::tr;
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf};

mod files;

pub(super) use files::contents;
pub(crate) use files::{target, write_contents};

pub struct InstallContext {
    pub home: PathBuf,
    pub environment: HashMap<String, String>,
    pub executable: PathBuf,
    /// Host packaging policy, never inferred from the current target/debug,
    /// preview bundle or an arbitrary executable supplied by a hook payload.
    /// Until a deliberate owner migration supplies a fixed path, use None.
    pub canonical_executable: Option<PathBuf>,
}
pub struct InstallPlan {
    pub configuration: PathBuf,
    pub source: String,
    pub flag: String,
    pub enabled: bool,
    pub changed: bool,
    original: Option<Vec<u8>>,
    updated: Updated,
    target: PathBuf,
}
/// The settings as they will be written.
enum Updated {
    /// JSON, saved by `json_text::rewrite` so only changed values move.
    Json(Value),
    /// A text file the client's own editor changed (`agents::patch_text`).
    Text(String),
}
impl InstallPlan {
    /// Never return the full settings object: client configuration may contain credentials.
    pub fn summary(&self) -> Value {
        json!({"source":self.source,"flag":self.flag,"enabled":self.enabled,"changed":self.changed,"configuration":self.configuration})
    }
}
pub(super) fn canonical(context: &InstallContext) -> Result<(), String> {
    let fixed=context.canonical_executable.as_ref().ok_or_else(|| tr!("This build has no approved fixed hook-owner executable yet. Keep hooks disabled until an explicit migration supplies its canonical installation path."))?;
    let text = fixed.to_string_lossy();
    if !fixed.is_absolute()
        || text.starts_with("/Volumes/")
        || text.contains("/AppTranslocation/")
        || fixed.components().any(|p| p.as_os_str() == "target")
    {
        return Err(tr!("Hooks cannot point at target builds, disk images or translocated copies. Use the approved fixed installed executable."));
    }
    let resolved = fixed
        .canonicalize()
        .map_err(|_| tr!("The approved fixed hook executable is unavailable"))?;
    let physical = resolved.to_string_lossy();
    if physical.starts_with("/Volumes/")
        || physical.contains("/AppTranslocation/")
        || resolved.components().any(|p| p.as_os_str() == "target")
    {
        return Err(tr!("The fixed hook path resolves to a transient target build; client settings were not changed."));
    }
    if context.executable != *fixed || !fixed.is_file() {
        return Err(tr!("This executable is not the approved fixed hook owner; client settings were not changed."));
    }
    Ok(())
}
/// The client's hook settings files: where its descriptor puts them, the
/// first its own and then one per profile for a client whose profiles each
/// read their own, or its default place under home.
fn configurations(context: &InstallContext, source: &str, relative: &str) -> Vec<PathBuf> {
    let placed = agent_hud_agents::hooks::hooks_by_id(source)
        .and_then(|hooks| hooks.settings)
        .map(|settings| crate::os::resolve(&context.home, &context.environment, &settings))
        .unwrap_or_default();
    if placed.is_empty() {
        vec![context.home.join(relative)]
    } else {
        placed
    }
}
/// What a hooks-only file holds before Agent HUD's hook joins it: for a
/// client that reads the `hooks` of another settings file for the events this
/// one leaves out (`Layout::Events`), those events' groups copied in, so they
/// keep running. The other file is only read.
fn with_shadowed(
    context: &InstallContext,
    source: &str,
    flag: &str,
    current: &Value,
) -> Result<Value, String> {
    let shadowed =
        agent_hud_agents::hooks::hooks_by_id(source).and_then(|hooks| match hooks.layout {
            agent_hud_agents::hooks::Layout::Events { shadowed } => shadowed,
            _ => None,
        });
    let Some(path) = shadowed.and_then(|location| {
        crate::os::resolve(&context.home, &context.environment, &location)
            .into_iter()
            .next()
    }) else {
        return Ok(current.clone());
    };
    // A file the client cannot read holds no hooks it runs.
    let settings = contents(&target(&path)?)?
        .and_then(|bytes| {
            serde_json::from_slice::<Value>(&agent_hud_agents::discovery::jsonc(&bytes)).ok()
        })
        .unwrap_or(Value::Null);
    agent_hud_agents::hooks::configuration::seed_shadowed(source, flag, current, &settings)
}
/// A plan for each of the client's settings files (`configurations`), so a
/// client whose profiles each read their own has one per profile. A file
/// that cannot be changed safely says why in its place, naming itself when
/// there are several, and the others still go in. Only what stops every
/// file is one error alone.
pub fn install_plans(
    context: &InstallContext,
    source: &str,
    flag: &str,
    enabled: bool,
) -> Vec<Result<InstallPlan, String>> {
    let checked = if enabled { canonical(context) } else { Ok(()) };
    let description = match checked.and_then(|()| agent_hud_agents::configuration(source, flag)) {
        Ok(description) => description,
        Err(error) => return vec![Err(error)],
    };
    let files = configurations(context, &description.source, &description.relative_path);
    let several = files.len() > 1;
    files
        .into_iter()
        .map(|configuration| {
            let shown = several.then(|| configuration.display().to_string());
            plan(context, &description, configuration, enabled).map_err(|error| match shown {
                Some(file) => format!("{error} ({file})"),
                None => error,
            })
        })
        .collect()
}

/// The plan for one settings file.
fn plan(
    context: &InstallContext,
    description: &agent_hud_agents::HookConfiguration,
    configuration: PathBuf,
    enabled: bool,
) -> Result<InstallPlan, String> {
    let target = target(&configuration)?;
    let original = contents(&target)?;
    if agent_hud_agents::text_settings(&description.source) {
        let current = match original.as_deref() {
            None => String::new(),
            Some(bytes) => String::from_utf8(bytes.to_vec())
                .map_err(|_| tr!("Hook settings are not UTF-8 text"))?,
        };
        let updated = crate::os::hook_text(
            &description.source,
            &description.flag,
            &current,
            enabled.then_some(context.executable.as_path()),
        )?;
        let changed = updated != current && (enabled || original.is_some());
        return Ok(InstallPlan {
            configuration,
            source: description.source.clone(),
            flag: description.flag.clone(),
            enabled,
            changed,
            original,
            updated: Updated::Text(updated),
            target,
        });
    }
    let current = match original.as_deref() {
        None | Some([]) => json!({}),
        Some(bytes) => serde_json::from_slice::<Value>(bytes)
            .map_err(|_| tr!("Hook settings are not valid JSON"))?,
    };
    if !current.is_object() {
        return Err(tr!("Hook settings must be a JSON object"));
    }
    let base = if enabled {
        with_shadowed(context, &description.source, &description.flag, &current)?
    } else {
        current.clone()
    };
    let updated = crate::os::hook_configuration(
        &description.source,
        &description.flag,
        &base,
        enabled.then_some(context.executable.as_path()),
    )?;
    let changed = updated != current && (enabled || original.is_some());
    Ok(InstallPlan {
        configuration,
        source: description.source.clone(),
        flag: description.flag.clone(),
        enabled,
        changed,
        original,
        updated: Updated::Json(updated),
        target,
    })
}
/// Call only following an explicit install/uninstall action. Preparing a plan
/// never changes settings, and applying a stale plan refuses the write.
pub fn apply_plan(plan: &InstallPlan) -> Result<bool, String> {
    if !plan.changed {
        return Ok(false);
    }
    if target(&plan.configuration)? != plan.target || contents(&plan.target)? != plan.original {
        return Err(tr!(
            "Client settings changed after this hook plan was prepared; prepare a new plan."
        ));
    }
    let bytes = match &plan.updated {
        Updated::Json(updated) => {
            crate::json_text::rewrite(plan.original.as_deref(), updated).unwrap()
        }
        Updated::Text(updated) => updated.clone().into_bytes(),
    };
    write_contents(&plan.target, Some(&bytes))?;
    Ok(true)
}

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
#[path = "install_tests_unix.rs"]
mod unix_tests;
