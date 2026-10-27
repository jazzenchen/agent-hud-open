use agent_hud_core::l10n;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const CONFIGURATION_LIMIT: u64 = 16 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    updated: Value,
    target: PathBuf,
}
impl InstallPlan {
    /// Never return the full settings object: client configuration may contain credentials.
    pub fn summary(&self) -> Value {
        json!({"source":self.source,"flag":self.flag,"enabled":self.enabled,"changed":self.changed,"configuration":self.configuration})
    }
}
pub(super) fn canonical(context: &InstallContext) -> Result<(), String> {
    let fixed=context.canonical_executable.as_ref().ok_or_else(|| l10n::text("预览版还没有指定固定的回调程序，在迁移指定正式安装路径之前请保持客户端回调关闭。", "Next has no approved fixed hook-owner executable yet. Keep hooks disabled until an explicit migration supplies its canonical installation path."))?;
    let text = fixed.to_string_lossy();
    if !fixed.is_absolute()
        || text.starts_with("/Volumes/")
        || text.contains("/AppTranslocation/")
        || fixed.components().any(|p| p.as_os_str() == "target")
    {
        return Err(l10n::text("客户端回调不能指向编译目录、磁盘映像或被系统转移的副本，请使用指定的固定安装程序。", "Hooks cannot point at target builds, disk images or translocated copies. Use the approved fixed installed executable."));
    }
    let resolved = fixed.canonicalize().map_err(|_| {
        l10n::text(
            "指定的固定回调程序不可用",
            "The approved fixed hook executable is unavailable",
        )
    })?;
    let physical = resolved.to_string_lossy();
    if physical.starts_with("/Volumes/")
        || physical.contains("/AppTranslocation/")
        || resolved.components().any(|p| p.as_os_str() == "target")
    {
        return Err(l10n::text("固定回调路径指向一个临时编译版本，客户端设置未作改动。", "The fixed hook path resolves to a transient target build; client settings were not changed."));
    }
    if context.executable != *fixed || !fixed.is_file() {
        return Err(l10n::text("当前程序不是指定的固定回调程序，客户端设置未作改动。", "This executable is not the approved fixed hook owner; client settings were not changed."));
    }
    Ok(())
}
/// The client's hook settings file: where its descriptor puts it, or its
/// default place under home.
fn configuration(context: &InstallContext, source: &str, relative: &str) -> PathBuf {
    agent_hud_agents::hooks::hooks_by_id(source)
        .and_then(|hooks| hooks.settings)
        .and_then(|settings| {
            crate::os::resolve(&context.home, &context.environment, &settings)
                .into_iter()
                .next()
        })
        .unwrap_or_else(|| context.home.join(relative))
}
pub(crate) fn target(path: &Path) -> Result<PathBuf, String> {
    let mut path = path.to_owned();
    for _ in 0..32 {
        match fs::read_link(&path) {
            Ok(link) => {
                path = if link.is_absolute() {
                    link
                } else {
                    path.parent()
                        .ok_or("Hook settings path unavailable")?
                        .join(link)
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(_) if !fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) => {
                return Ok(path)
            }
            Err(_) => return Err("Hook settings symlink could not be resolved".into()),
        }
    }
    Err("Hook settings contain too many symbolic links".into())
}
pub(super) fn contents(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::File::open(path) {
        Ok(file) => {
            if file
                .metadata()
                .map_err(|_| l10n::text("无法读取客户端的回调设置", "Hook settings unavailable"))?
                .len()
                > CONFIGURATION_LIMIT
            {
                return Err(l10n::text(
                    "客户端的回调设置超过 16 MiB",
                    "Hook settings exceed 16 MiB",
                ));
            }
            let mut bytes = Vec::new();
            file.take(CONFIGURATION_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| l10n::text("无法读取客户端的回调设置", "Hook settings unavailable"))?;
            if bytes.len() as u64 > CONFIGURATION_LIMIT {
                return Err(l10n::text(
                    "客户端的回调设置超过 16 MiB",
                    "Hook settings exceed 16 MiB",
                ));
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(l10n::text(
            "无法读取客户端的回调设置",
            "Hook settings unavailable",
        )),
    }
}
pub fn install_plan(
    context: &InstallContext,
    source: &str,
    flag: &str,
    enabled: bool,
) -> Result<InstallPlan, String> {
    if !super::supported() {
        return Err(l10n::text(
            "Windows 版暂不支持安装客户端回调，设置未作改动。",
            "Windows hook execution/ACL adapter is not implemented; no settings were changed.",
        ));
    }
    if enabled {
        canonical(context)?;
    }
    let description = agent_hud_agents::configuration(source, flag)?;
    let configuration = configuration(context, &description.source, &description.relative_path);
    let target = target(&configuration)?;
    let original = contents(&target)?;
    let current = match original.as_deref() {
        None | Some([]) => json!({}),
        Some(bytes) => serde_json::from_slice::<Value>(bytes).map_err(|_| {
            l10n::text(
                "客户端的回调设置不是有效的 JSON",
                "Hook settings are not valid JSON",
            )
        })?,
    };
    if !current.is_object() {
        return Err(l10n::text(
            "客户端的回调设置必须是 JSON 对象",
            "Hook settings must be a JSON object",
        ));
    }
    let updated = crate::os::hook_configuration(
        &description.source,
        &description.flag,
        &current,
        enabled.then_some(context.executable.as_path()),
    )?;
    let changed = updated != current && (enabled || original.is_some());
    Ok(InstallPlan {
        configuration,
        source: description.source,
        flag: description.flag,
        enabled,
        changed,
        original,
        updated,
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
        return Err(l10n::text(
            "准备回调改动后客户端设置又发生了变化，请重新操作。",
            "Client settings changed after this hook plan was prepared; prepare a new plan.",
        ));
    }
    let bytes = crate::json_text::rewrite(plan.original.as_deref(), &plan.updated)
        .map_err(|_| "Hook settings could not be saved")?;
    write_contents(&plan.target, Some(&bytes))?;
    Ok(true)
}

/// Atomic settings replacement shared by reversible explicit installers.
/// The caller checks its plan immediately before writing or removing the file.
pub(crate) fn write_contents(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    let Some(bytes) = bytes else {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Hook settings could not be removed".into()),
        };
    };
    let directory = path.parent().ok_or("Hook settings directory unavailable")?;
    fs::create_dir_all(directory).map_err(|_| "Hook settings directory unavailable")?;
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let temporary = directory.join(format!(
        ".agent-hud-hook-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = crate::os::private_file(&temporary, true).map_err(|_| {
            l10n::text(
                "无法保存客户端的回调设置",
                "Hook settings could not be saved",
            )
        })?;
        file.write_all(bytes).map_err(|_| {
            l10n::text(
                "无法保存客户端的回调设置",
                "Hook settings could not be saved",
            )
        })?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions).map_err(|_| {
                l10n::text(
                    "无法保留客户端回调设置的文件权限",
                    "Hook settings permissions could not be preserved",
                )
            })?;
        }
        fs::rename(&temporary, path).map_err(|_| {
            l10n::text(
                "无法保存客户端的回调设置",
                "Hook settings could not be saved",
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(home: &Path, env: &[(&str, &str)]) -> InstallContext {
        InstallContext {
            home: home.into(),
            environment: env
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            executable: PathBuf::new(),
            canonical_executable: None,
        }
    }

    #[test]
    fn settings_follow_each_clients_folder_variable() {
        let home = Path::new("/h");
        let path = |source: &str, env: &[(&str, &str)]| {
            let relative = agent_hud_agents::configuration(source, "--permission-hook")
                .or_else(|_| agent_hud_agents::configuration(source, "--completion-hook"))
                .unwrap()
                .relative_path;
            configuration(&context(home, env), source, &relative)
        };
        assert_eq!(path("claude", &[]), home.join(".claude/settings.json"));
        assert_eq!(
            path("claude", &[("CLAUDE_CONFIG_DIR", "/c")]),
            Path::new("/c/settings.json")
        );
        assert_eq!(
            path("codex", &[("CODEX_HOME", " /x ")]),
            Path::new("/x/hooks.json")
        );
        assert_eq!(
            path("antigravity", &[("GEMINI_CLI_HOME", "/g")]),
            Path::new("/g/config/hooks.json")
        );
        assert_eq!(
            path("codebuddy", &[("CODEBUDDY_CONFIG_DIR", "/h/.workbuddy")]),
            home.join(".codebuddy/settings.json"),
            "WorkBuddy's folder is not CodeBuddy's"
        );
        assert_eq!(
            path("workbuddy", &[]),
            home.join(".workbuddy/settings.json")
        );
        assert_eq!(
            path("qoder-work", &[("QWEN_HOME", "/q")]),
            home.join(".qoderwork/settings.json")
        );
    }
}
