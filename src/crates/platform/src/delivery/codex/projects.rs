//! Codex Desktop's project identities and the folders they cover.
use super::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct Project {
    id: String,
    roots: Vec<(PathBuf, PathBuf)>,
}

impl Project {
    pub(crate) fn new(id: String, roots: Vec<PathBuf>) -> Self {
        let roots = roots
            .into_iter()
            .map(|root| {
                let canonical = normalized(&root);
                (root, canonical)
            })
            .collect();
        Self { id, roots }
    }

    pub(crate) fn roots(&self) -> impl Iterator<Item = &Path> {
        self.roots.iter().map(|(path, _)| path.as_path())
    }
}

pub(crate) fn projects(config: &StartConfig) -> Result<Vec<Project>> {
    let route = config
        .codex_queue()
        .ok_or_else(|| Failure::Unavailable("Codex executable".into()))?;
    let mut server = process::StdioServer::open(
        &route.executable,
        &config.codex_data,
        &route.environment,
        Duration::from_secs(5),
    )?;
    let result = read_projects(|cursor| {
        server.request("project/list", json!({"cursor":cursor,"limit":100}))
    });
    server.end();
    result
}

fn read_projects(mut page: impl FnMut(Option<&str>) -> Result<Value>) -> Result<Vec<Project>> {
    let mut projects = Vec::new();
    let mut cursor = None::<String>;
    for _ in 0..5 {
        let response = page(cursor.as_deref())?;
        let rows = response["data"]
            .as_array()
            .ok_or_else(|| Failure::Protocol("Codex project list".into()))?;
        for row in rows {
            let id = row["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Failure::Protocol("Codex project identity".into()))?;
            let roots = row["roots"]
                .as_array()
                .ok_or_else(|| Failure::Protocol("Codex project roots".into()))?
                .iter()
                .map(|root| {
                    root["path"]
                        .as_str()
                        .map(PathBuf::from)
                        .filter(|path| path.is_absolute())
                        .ok_or_else(|| Failure::Protocol("Codex project root".into()))
                })
                .collect::<Result<Vec<_>>>()?;
            projects.push(Project::new(id.to_owned(), roots));
        }
        match &response["nextCursor"] {
            Value::Null => return Ok(projects),
            Value::String(next) if !next.is_empty() && Some(next) != cursor.as_ref() => {
                cursor = Some(next.clone());
            }
            _ => return Err(Failure::Protocol("Codex project cursor".into())),
        }
    }
    // A partial list cannot establish that the winning project is unique.
    Err(Failure::Unavailable(
        "Codex project list exceeded the page limit".into(),
    ))
}

pub(crate) fn project_for_folder<'a>(
    projects: &'a [Project],
    folder: &Path,
) -> Result<Option<&'a str>> {
    let canonical = normalized(folder);
    let mut best: Option<(usize, &str)> = None;
    let mut ambiguous = false;
    for project in projects {
        for (_, root) in &project.roots {
            if !canonical.starts_with(root) {
                continue;
            }
            let depth = root.components().count();
            match best {
                None => best = Some((depth, &project.id)),
                Some((current, _)) if depth > current => {
                    best = Some((depth, &project.id));
                    ambiguous = false;
                }
                Some((current, id)) if depth == current && id != project.id.as_str() => {
                    ambiguous = true;
                }
                _ => {}
            }
        }
    }
    if ambiguous {
        return Err(Failure::Unavailable(format!(
            "More than one Codex Desktop project covers {} at the same root depth",
            folder.display()
        )));
    }
    Ok(best.map(|(_, id)| id))
}

fn normalized(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_lookup_does_not_accept_an_incomplete_catalog() {
        let mut offset = 0;
        let result = read_projects(|cursor| {
            assert_eq!(cursor, (offset != 0).then(|| offset.to_string()).as_deref());
            offset += 1;
            Ok(json!({"data":[],"nextCursor":offset.to_string()}))
        });
        assert_eq!(offset, 5);
        assert!(matches!(result, Err(Failure::Unavailable(_))));
    }
}
