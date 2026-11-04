//! An explicit, reversible status-line subscription to agy's own quota reports.
//! Settings retain their renderer and options; the silent callback receives the
//! same stdin before the existing renderer runs. No CLI or app is launched here.
use super::{install, InstallContext};
use agent_hud_core::l10n;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const METADATA: &str = "_agentHudAntigravityStatusLine";
const FLAG: &str = "--antigravity-statusline-hook";

fn supported() -> bool {
    agent_hud_agents::hooks::POSIX_ONLY
        .on(crate::os::OS)
        .value()
        .is_some()
}

fn unavailable() -> String {
    l10n::text(
        "此系统暂不支持 Antigravity CLI 状态栏回调，设置未作改动。",
        "Antigravity CLI status-line callbacks are unavailable on this system; settings were not changed.",
    )
}

fn conflict() -> String {
    l10n::text(
        "Antigravity CLI 的状态栏或 Agent HUD 安装记录已被其他程序修改，请先恢复原有配置。",
        "The Antigravity CLI status line or Agent HUD installation record was modified externally; restore its previous configuration first.",
    )
}

pub(super) fn status(context: Option<&InstallContext>) -> Value {
    let default_settings = context
        .map(|context| context.home.clone())
        .or_else(|| crate::os::home_dir().ok())
        .map(|home| home.join(".gemini/antigravity-cli/settings.json"));
    let notice = if !supported() {
        Some(unavailable())
    } else if let Some(context) = context {
        install::canonical(context).err()
    } else {
        Some(l10n::text(
            "尚未指定固定的回调程序，Antigravity CLI 设置未作改动。",
            "No fixed hook-owner executable has been delegated; Antigravity CLI settings were not changed.",
        ))
    };
    json!({"installable":notice.is_none(),"defaultSettings":default_settings,"notice":notice})
}

/// Resolves leaf and parent symlinks, including an existing parent of a new file.
/// Keeping the resolved path in both callback and preference gives one scope.
fn configuration(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(l10n::text(
            "请使用 CLI 设置文件的绝对路径。",
            "Use an absolute path to the CLI settings file.",
        ));
    }
    let resolved = install::target(path)?;
    let mut ancestor = resolved.as_path();
    let mut suffix = Vec::new();
    loop {
        match ancestor.canonicalize() {
            Ok(mut canonical) => {
                for name in suffix.iter().rev() {
                    canonical.push(name);
                }
                if canonical.is_dir() {
                    return Err("The CLI settings path is a directory".into());
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    ancestor
                        .file_name()
                        .ok_or("CLI settings path unavailable")?
                        .to_owned(),
                );
                ancestor = ancestor.parent().ok_or("CLI settings path unavailable")?;
            }
            Err(_) => return Err("CLI settings path could not be resolved".into()),
        }
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn callback(context: &InstallContext, data: &Path, settings: &Path) -> Result<String, String> {
    let path = |path: &Path| {
        path.to_str()
            .filter(|text| !text.contains('\0'))
            .map(quote)
            .ok_or_else(|| "The callback path is not valid UTF-8".to_owned())
    };
    if !data.is_absolute() {
        return Err("The callback data directory must be absolute".into());
    }
    Ok(format!(
        "{} {FLAG} {} {}",
        path(&context.executable)?,
        path(data)?,
        path(settings)?
    ))
}

fn original_renderer(value: &Value) -> Result<&str, String> {
    if !value.is_object() || (!value["type"].is_null() && value["type"] != "command") {
        return Err("The existing CLI statusLine must be a command object".into());
    }
    value["command"]
        .as_str()
        .filter(|command| !command.trim().is_empty() && !command.contains('\0'))
        .ok_or_else(|| "The existing CLI statusLine command is unavailable".into())
}

fn installed_line(original: Option<&Value>, callback: &str) -> Result<Value, String> {
    match original {
        Some(original) if original.is_object() && original["enabled"] == false => {
            // Connecting enables collection, not a previously disabled renderer.
            // The built-in line remains visible; the exact disabled object is
            // retained in our metadata and restored when disconnecting.
            let mut installed = original.clone();
            installed["enabled"] = json!(true);
            installed["type"] = json!("command");
            installed["command"] = json!(format!("{callback} >/dev/null 2>&1 || :"));
            installed["stack_with_default"] = json!(true);
            Ok(installed)
        }
        Some(original) => {
            let renderer = original_renderer(original)?;
            // A sentinel preserves trailing newlines through POSIX command
            // substitution. The payload stays in memory; no private stdin file.
            let command = format!(
                "agent_hud_input=$(cat; printf '.'); agent_hud_input=${{agent_hud_input%.}}; printf '%s' \"$agent_hud_input\" | {callback} >/dev/null 2>&1; printf '%s' \"$agent_hud_input\" | /bin/sh -c {}",
                quote(renderer),
            );
            let mut installed = original.clone();
            installed["command"] = json!(command);
            Ok(installed)
        }
        None => Ok(
            json!({"type":"command","command":format!("{callback} >/dev/null 2>&1 || :"),"stack_with_default":true}),
        ),
    }
}

fn metadata(original: Option<&Value>, installed: &Value, callback: &str, created: bool) -> Value {
    json!({"version":1,"callback":callback,"createdSettings":created,"hadStatusLine":original.is_some(),"originalStatusLine":original,"installedStatusLine":installed})
}

struct FileChange {
    configuration: PathBuf,
    target: PathBuf,
    original: Option<Vec<u8>>,
    updated: Option<Vec<u8>>,
}

impl FileChange {
    fn apply(&self, backwards: bool) -> Result<(), String> {
        let (expected, replacement) = if backwards {
            (&self.updated, &self.original)
        } else {
            (&self.original, &self.updated)
        };
        if configuration(&self.configuration)? != self.target
            || install::contents(&self.target)? != *expected
        {
            return Err(l10n::text(
                "准备状态栏改动后 CLI 设置又发生了变化，请重新操作。",
                "CLI settings changed after this status-line plan was prepared; prepare a new plan.",
            ));
        }
        install::write_contents(&self.target, replacement.as_deref())
    }
}

/// Applied file edits retained until the core preference save succeeds. A
/// failed save must call rollback; dropping a successful transaction commits it.
/// Neither this value nor its errors expose client settings or renderer text.
pub struct AntigravityCliChange {
    settings: Vec<PathBuf>,
    writes: Vec<FileChange>,
}
impl AntigravityCliChange {
    pub fn settings(&self) -> &[PathBuf] {
        &self.settings
    }
    pub fn rollback(&self) -> Result<(), String> {
        rollback(&self.writes)
    }
}

fn rollback(writes: &[FileChange]) -> Result<(), String> {
    let mut failures = 0;
    for change in writes.iter().rev() {
        if change.apply(true).is_err() {
            failures += 1;
        }
    }
    if failures == 0 {
        Ok(())
    } else {
        Err(l10n::text(
            "部分 CLI 设置在保存后发生变化，未覆盖这些文件，状态栏改动未能全部恢复。",
            "Some CLI settings changed after the write and were left untouched; not all status-line edits could be rolled back.",
        ))
    }
}

fn plan_file(
    context: &InstallContext,
    data: &Path,
    path: &Path,
    enabled: bool,
) -> Result<Option<FileChange>, String> {
    let target = configuration(path)?;
    let original = install::contents(&target)?;
    let current: Value = match original.as_deref() {
        None | Some([]) => json!({}),
        Some(bytes) => {
            serde_json::from_slice(bytes).map_err(|_| "CLI settings are not valid JSON")?
        }
    };
    let mut updated = current
        .as_object()
        .cloned()
        .ok_or("CLI settings must be a JSON object")?;
    let command = callback(context, data, &target)?;
    let mut remove_file = false;
    if let Some(owned) = current.get(METADATA) {
        let had = owned["hadStatusLine"].as_bool().ok_or_else(conflict)?;
        let created = owned["createdSettings"].as_bool().ok_or_else(conflict)?;
        let previous = had.then(|| &owned["originalStatusLine"]);
        let installed = installed_line(previous, &command).map_err(|_| conflict())?;
        if *owned != metadata(previous, &installed, &command, created)
            || current.get("statusLine") != Some(&installed)
        {
            return Err(conflict());
        }
        if enabled {
            return Ok(None);
        }
        if let Some(previous) = previous {
            updated.insert("statusLine".into(), previous.clone());
        } else {
            updated.remove("statusLine");
        }
        updated.remove(METADATA);
        remove_file = created && updated.is_empty();
    } else if enabled {
        let previous = current.get("statusLine");
        if previous.is_some_and(|line| {
            line["command"]
                .as_str()
                .is_some_and(|command| command.contains(FLAG))
        }) {
            return Err(conflict());
        }
        let installed = installed_line(previous, &command)?;
        updated.insert(
            METADATA.into(),
            metadata(previous, &installed, &command, original.is_none()),
        );
        updated.insert("statusLine".into(), installed);
    } else {
        return Ok(None);
    }
    let bytes = crate::json_text::rewrite(original.as_deref(), &Value::Object(updated)).unwrap();
    Ok(Some(FileChange {
        configuration: path.to_owned(),
        target,
        original,
        updated: (!remove_file).then_some(bytes),
    }))
}

fn plan(
    context: &InstallContext,
    data: &Path,
    previous: &[PathBuf],
    next: &[PathBuf],
) -> Result<AntigravityCliChange, String> {
    if !supported() {
        return Err(unavailable());
    }
    install::canonical(context)?;
    let expand = |path: &Path| {
        let text = path
            .to_str()
            .ok_or("The CLI settings path is not valid UTF-8")?;
        Ok::<_, String>(if text == "~" {
            context.home.clone()
        } else if let Some(relative) = text.strip_prefix("~/") {
            context.home.join(relative)
        } else {
            path.to_owned()
        })
    };
    let next = next
        .iter()
        .map(|path| expand(path))
        .collect::<Result<Vec<_>, _>>()?;
    let previous = previous
        .iter()
        .map(|path| expand(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut settings = Vec::new();
    for path in &next {
        let path = configuration(path)?;
        if settings.contains(&path) {
            return Err("The same CLI settings file was selected more than once".into());
        }
        settings.push(path);
    }
    let mut paths = Vec::new();
    // Plan through the original aliases so a symlink retargeted between plan
    // and apply cannot redirect a settings write.
    for path in previous.iter().chain(&next) {
        let target = configuration(path)?;
        if !paths.iter().any(|(_, existing)| *existing == target) {
            paths.push((path, target));
        }
    }
    let mut writes = Vec::new();
    for (path, target) in paths {
        if let Some(change) = plan_file(context, data, path, settings.contains(&target))? {
            writes.push(change);
        }
    }
    Ok(AntigravityCliChange { settings, writes })
}

fn apply(plan: AntigravityCliChange) -> Result<AntigravityCliChange, String> {
    for (index, change) in plan.writes.iter().enumerate() {
        if let Err(error) = change.apply(false) {
            return match rollback(&plan.writes[..index]) {
                Ok(()) => Err(error),
                Err(rollback) => Err(format!("{error} {rollback}")),
            };
        }
    }
    Ok(plan)
}

pub(super) fn set_accounts(
    context: Option<&InstallContext>,
    data: &Path,
    previous: &[PathBuf],
    next: &[PathBuf],
) -> Result<AntigravityCliChange, String> {
    let context = context.ok_or_else(|| l10n::text(
        "尚未指定固定的回调程序，Antigravity CLI 设置未作改动。",
        "No fixed hook-owner executable has been delegated; Antigravity CLI settings were not changed.",
    ))?;
    apply(plan(context, data, previous, next)?)
}

#[cfg(all(test, unix))]
#[path = "statusline_install_tests_unix.rs"]
mod tests;
