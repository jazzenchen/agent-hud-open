//! The folders Claude Code trusts, read from its settings, and the one entry
//! Agent HUD adds there: its own folder for tasks with no project.
use super::*;
use serde_json::json;

impl SessionStarts {
    /// Claude Code's settings file, which also records the folders it trusts.
    fn claude_settings(&self) -> PathBuf {
        if self.config.claude_config == self.config.home.join(".claude") {
            self.config.home.join(".claude.json")
        } else {
            self.config.claude_config.join(".claude.json")
        }
    }
    /// Marks Agent HUD's Claude task folder as trusted in Claude Code's
    /// settings, as accepting Claude Code's trust prompt there would. Claude
    /// Code refuses to start in a folder it does not trust, and trust passes
    /// down to subfolders; this folder holds only the folders Agent HUD makes
    /// for tasks with no project. Runs at launch and again before each such
    /// start. Only that one entry changes; nothing is written when Claude
    /// Code is not installed or set up, and settings that do not read as JSON
    /// are left alone.
    pub fn trust_task_folder(&self) -> Result<()> {
        if self.config.claude_executable.is_none() {
            return Ok(());
        }
        let root = self.config.task_root.join("claude");
        let unreadable = || Failure::Unavailable("Claude Code's settings could not be read".into());
        let file = self.claude_settings();
        let original = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let mut settings = serde_json::from_slice::<Value>(&original).map_err(|_| unreadable())?;
        if trusts(&settings, &root) {
            return Ok(());
        }
        let project = settings
            .as_object_mut()
            .ok_or_else(unreadable)?
            .entry("projects")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(unreadable)?
            .entry(root.to_string_lossy().into_owned())
            // The entry Claude Code writes when its trust prompt is accepted.
            .or_insert_with(|| {
                json!({
                    "allowedTools": [],
                    "mcpContextUris": [],
                    "enabledMcpjsonServers": [],
                    "disabledMcpjsonServers": [],
                    "hasClaudeMdExternalIncludesApproved": false,
                    "hasClaudeMdExternalIncludesWarningShown": false,
                })
            })
            .as_object_mut()
            .ok_or_else(unreadable)?;
        project.insert("hasTrustDialogAccepted".into(), json!(true));
        // Only the one entry is written; the rest of the file keeps its bytes.
        let bytes = crate::json_text::rewrite(Some(&original), &settings)
            .map_err(|e| Failure::Protocol(e.to_string()))?;
        replace_file(&file, &bytes)
    }
    pub(super) fn accepts(&self, provider: Provider, folder: &Path) -> bool {
        if provider != Provider::Claude {
            return true;
        }
        trusts(&read_json(&self.claude_settings()), folder)
    }
}

/// Whether Claude Code's `settings` trust `folder` or a folder above it.
fn trusts(settings: &Value, folder: &Path) -> bool {
    let Some(projects) = settings["projects"].as_object() else {
        return false;
    };
    folder.ancestors().any(|path| {
        projects
            .get(path.to_string_lossy().as_ref())
            .is_some_and(|project| project["hasTrustDialogAccepted"] == true)
    })
}
/// Replaces `file` through a private temporary file beside it. Unlike
/// `atomic_json`, it leaves the folder's permissions as they are: Claude
/// Code's settings sit in the home folder.
fn replace_file(file: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let (Some(parent), Some(name)) = (file.parent(), file.file_name()) else {
        return Err(Failure::InvalidInput("path".into()));
    };
    let temp = parent.join(format!(
        "{}.agent-hud-{}",
        name.to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| {
        let mut handle = host::new_file_options().open(&temp)?;
        handle.write_all(bytes)?;
        handle.sync_all()?;
        fs::rename(&temp, file)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
