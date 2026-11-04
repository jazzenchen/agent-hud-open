//! The projects the panel's sessions belong to, and what a proposal's
//! `sessions.projects` name.
use super::*;
use agent_hud_core::projects;

/// The projects the panel's sessions belong to now, by id, in Finder's order with the
/// sessions of no folder last: what `sessions.projects` lists and get_hud offers.
pub(in crate::presentation) fn offered_projects(snapshot: &Value) -> Vec<String> {
    let mut ids: Vec<Option<String>> = vec![];
    for session in snapshot["sessions"].as_array().into_iter().flatten() {
        let id = session["project"]
            .as_str()
            .filter(|s| !s.is_empty() && *s != projects::UNASSIGNED)
            .map(str::to_owned);
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.sort_by(projects::compare);
    ids.into_iter()
        .map(|id| id.unwrap_or_else(|| projects::UNASSIGNED.into()))
        .collect()
}

/// A project's name as the panel writes it: its folder's.
pub(in crate::presentation) fn project_title(id: &str) -> String {
    if id == projects::UNASSIGNED {
        return "Unassigned".into();
    }
    id.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(id)
        .to_owned()
}

/// A folder written with `~` as the home folder.
fn expanded(path: &str) -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    match (path.strip_prefix('~'), home) {
        (Some(rest), Ok(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            format!("{home}{rest}")
        }
        _ => path.to_owned(),
    }
}

/// What a proposal's `sessions.projects` name (PanelLayout.projectMatch): each
/// entry's project by its id, the entries that name one by a path, and the issues of
/// those that name none.
pub(in crate::presentation) struct ProjectMatch {
    pub ids: Vec<String>,
    /// An entry given as a path, with the id of the project it names.
    pub paths: Vec<(String, String)>,
    pub issues: Vec<String>,
}

/// An entry names the project whose id it is, of those the sessions belong to now or
/// a `held` layout lists; else, where it is a path, absolute or written with `~`, the
/// innermost of them it lies in, as written or as Git places it (its repository's
/// checkout for a worktree), so an agent names the project it works in by its own
/// working directory. An entry that names none is an issue, and so is one that names
/// a project by its name in place of its id.
pub(in crate::presentation) fn project_match(
    value: &Value,
    snapshot: &Value,
    held: &[Value],
) -> ProjectMatch {
    let offered = offered_projects(snapshot);
    let mut candidates = offered.clone();
    for layout in held {
        for id in strings(&layout["sessions"]["projects"]) {
            push_unique(&mut candidates, &id);
        }
    }
    let listed = if offered.is_empty() {
        "No project has a session now.".to_owned()
    } else {
        format!("Projects with sessions now: {}.", quoted(&offered))
    };
    let mut found = ProjectMatch {
        ids: vec![],
        paths: vec![],
        issues: vec![],
    };
    for (index, entry) in entries(value) {
        if entry.is_empty() {
            continue;
        }
        if candidates.iter().any(|id| id == entry) {
            found.ids.push(entry.to_owned());
            continue;
        }
        if entry.starts_with('~') || std::path::Path::new(entry).is_absolute() {
            let spellings = [expanded(entry), expanded(&projects::normalize(entry))];
            let innermost = candidates
                .iter()
                .filter(|id| *id != projects::UNASSIGNED)
                .filter(|id| {
                    let folder = expanded(id);
                    let folder = folder.trim_end_matches(['/', '\\']);
                    spellings.iter().any(|path| {
                        path == folder
                            || path
                                .strip_prefix(folder)
                                .is_some_and(|rest| rest.starts_with(['/', '\\']))
                    })
                })
                .max_by_key(|id| expanded(id).len());
            if let Some(id) = innermost {
                found.ids.push(id.clone());
                found.paths.push((entry.to_owned(), id.clone()));
                continue;
            }
        }
        let named: Vec<&String> = offered
            .iter()
            .filter(|id| *id != projects::UNASSIGNED && project_title(id) == entry)
            .collect();
        let message = if named.is_empty() {
            format!("\"{entry}\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. {listed}")
        } else {
            format!(
                "\"{entry}\" is a project's name, not its id. Name it by its id: {}.",
                quoted(&named)
            )
        };
        found
            .issues
            .push(format!("sessions.projects[{index}]: {message}"));
    }
    found
}
