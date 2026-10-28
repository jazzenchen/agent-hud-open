//! The one skill every client gets: `<skills>/customize-agent-hud/SKILL.md`.
//! Agent HUD's copy says so in its frontmatter (`managed-by: agent-hud`), so
//! a folder of that name that someone else wrote is never changed or removed.
use std::{
    fs,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

/// The folder name, the same in every client: clients that read each
/// other's folders then count it once.
pub const SKILL_FOLDER: &str = "customize-agent-hud";
pub(super) const TEXT: &str = include_str!("customize-agent-hud.md");
const MARK: &str = "managed-by: agent-hud";
const FILE: &str = "SKILL.md";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillState {
    Missing,
    /// Agent HUD's copy, as this version writes it.
    Current,
    /// Agent HUD's copy from another version.
    Older,
    /// A skill of the same name that Agent HUD did not write, or a file
    /// where its folder would go.
    Foreign,
}

/// Whether `text` is a SKILL.md Agent HUD wrote: the mark in its frontmatter.
fn ours(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("---\n") else {
        return false;
    };
    rest.split("\n---")
        .next()
        .is_some_and(|front| front.lines().any(|line| line.trim() == MARK))
}

pub(super) fn state(skills: &Path) -> SkillState {
    let folder = skills.join(SKILL_FOLDER);
    match fs::read_to_string(folder.join(FILE)) {
        Ok(text) if text == TEXT => SkillState::Current,
        Ok(text) if ours(&text) => SkillState::Older,
        Ok(_) => SkillState::Foreign,
        // A folder without SKILL.md, such as one our removal left with a
        // .DS_Store in it, holds no skill: it is filled again.
        Err(_) if fs::metadata(&folder).is_ok_and(|metadata| !metadata.is_dir()) => {
            SkillState::Foreign
        }
        Err(_) => SkillState::Missing,
    }
}

pub(super) fn add(skills: &Path) -> Result<bool, String> {
    let folder = skills.join(SKILL_FOLDER);
    match state(skills) {
        SkillState::Current => return Ok(false),
        SkillState::Foreign => {
            return Err(format!(
                "{} already holds a skill Agent HUD did not write; it was left as it is",
                folder.display()
            ))
        }
        SkillState::Missing | SkillState::Older => {}
    }
    fs::create_dir_all(&folder)
        .map_err(|_| format!("{} could not be created", folder.display()))?;
    // Written whole beside it, then renamed over it: a client never reads half a skill.
    let temporary = folder.join(format!(
        ".{FILE}.agent-hud-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(TEXT.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, folder.join(FILE))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|_| format!("The skill could not be written to {}", folder.display()))?;
    Ok(true)
}

pub(super) fn remove(skills: &Path) -> Result<(), String> {
    let folder = skills.join(SKILL_FOLDER);
    match state(skills) {
        SkillState::Missing | SkillState::Foreign => return Ok(()),
        SkillState::Current | SkillState::Older => {}
    }
    // Only what Agent HUD writes: a file someone added beside it keeps the folder.
    fs::remove_file(folder.join(FILE))
        .map_err(|_| format!("The skill could not be removed from {}", folder.display()))?;
    let _ = fs::remove_dir(&folder);
    Ok(())
}
