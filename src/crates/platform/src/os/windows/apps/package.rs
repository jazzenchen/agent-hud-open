//! A Microsoft Store (MSIX) app runs inside its package only when started
//! through its Application User Model ID. Started from its folder under
//! `WindowsApps` it would run outside it, with other data folders (the
//! Store Claude keeps its own under `Packages\Claude_*`). The ID is read from
//! the package folder's name and its manifest, with no PowerShell.
use std::path::Path;

/// `<package family name>!<application id>` for a program inside an
/// installed package, or None for any other program.
pub(super) fn app_user_model_id(program: &Path) -> Option<String> {
    let package = program.ancestors().skip(1).find(|dir| {
        dir.parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case("WindowsApps"))
    })?;
    let family = family_name(package.file_name()?.to_str()?)?;
    let entry = program.strip_prefix(package).ok()?.to_str()?.to_owned();
    let manifest = std::fs::read_to_string(package.join("AppxManifest.xml")).ok()?;
    let application = application_id(&manifest, &entry)?;
    Some(format!("{family}!{application}"))
}

/// A package's full name is `Name_Version_Architecture_ResourceId_PublisherId`
/// (no part holds `_`); its family name keeps the first and the last.
fn family_name(full_name: &str) -> Option<String> {
    let parts = full_name.split('_').collect::<Vec<_>>();
    let [name, _, _, _, publisher] = parts[..] else {
        return None;
    };
    (!name.is_empty() && !publisher.is_empty()).then(|| format!("{name}_{publisher}"))
}

/// The `Id` of the manifest's `<Application>` whose `Executable` is `entry`,
/// a path inside the package; the only one when there is a single entry.
fn application_id(manifest: &str, entry: &str) -> Option<String> {
    let normal = |path: &str| path.replace('/', "\\").to_ascii_lowercase();
    let entry = normal(entry);
    let applications = manifest
        .match_indices("<Application")
        .filter_map(|(at, _)| {
            let tag = &manifest[at + "<Application".len()..];
            if !tag.starts_with(char::is_whitespace) {
                return None; // <Applications>
            }
            let tag = &tag[..tag.find('>')?];
            Some((attribute(tag, "Id")?, attribute(tag, "Executable")))
        })
        .collect::<Vec<_>>();
    let matching = applications
        .iter()
        .find(|(_, executable)| executable.as_deref().map(normal).as_ref() == Some(&entry));
    match (matching, &applications[..]) {
        (Some((id, _)), _) | (None, [(id, _)]) => Some(id.clone()),
        _ => None,
    }
}

/// An attribute's value inside a start tag, in either kind of quotes.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=').map(str::trim_start) else {
            continue;
        };
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &value[1..];
        return Some(value[..value.find(quote)?].to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
  <Identity Name="OpenAI.Codex" Version="26.1106.6628.0" />
  <Applications>
    <Application Id="Helper" Executable="app\resources\helper.exe" EntryPoint="Windows.FullTrustApplication" />
    <Application
        Id='App' Executable="app/Codex.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Codex" />
    </Application>
  </Applications>
</Package>"#;

    #[test]
    fn a_family_name_keeps_the_package_name_and_publisher() {
        assert_eq!(
            family_name("OpenAI.Codex_26.1106.6628.0_x64__2p2nqsd0c76g0").as_deref(),
            Some("OpenAI.Codex_2p2nqsd0c76g0")
        );
        assert_eq!(family_name("Codex"), None);
        assert_eq!(family_name("a_b_c_d_e_f"), None);
    }

    #[test]
    fn the_application_is_the_one_that_runs_the_program() {
        assert_eq!(
            application_id(MANIFEST, "app\\Codex.exe").as_deref(),
            Some("App")
        );
        assert_eq!(
            application_id(MANIFEST, "APP\\RESOURCES\\helper.exe").as_deref(),
            Some("Helper")
        );
        assert_eq!(application_id(MANIFEST, "app\\other.exe"), None);
        let single =
            r#"<Applications><Application Id="Claude" Executable="Claude.exe"/></Applications>"#;
        assert_eq!(
            application_id(single, "app\\claude.exe").as_deref(),
            Some("Claude")
        );
    }

    #[test]
    fn a_program_inside_a_package_gets_its_model_id() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp
            .path()
            .join("WindowsApps/OpenAI.Codex_26.1106.6628.0_x64__2p2nqsd0c76g0");
        let program = package.join("app/Codex.exe");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap(); // plain directory
        std::fs::write(&program, b"").unwrap();
        std::fs::write(package.join("AppxManifest.xml"), MANIFEST).unwrap();
        assert_eq!(
            app_user_model_id(&program).as_deref(),
            Some("OpenAI.Codex_2p2nqsd0c76g0!App")
        );
        let outside = temp.path().join("Programs/Codex/Codex.exe");
        assert_eq!(app_user_model_id(&outside), None);
    }
}
