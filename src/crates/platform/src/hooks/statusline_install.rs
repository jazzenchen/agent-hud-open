//! An explicit, reversible status-line subscription to agy's own quota reports.
//! Settings retain their renderer and options; the silent callback receives the
//! same stdin before the existing renderer runs. No CLI or app is launched here.
use super::{install, InstallContext};
use agent_hud_core::l10n;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

mod line;

use line::plan_file;

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
