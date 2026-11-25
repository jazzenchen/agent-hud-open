use super::*;

pub(super) const CODEX_WITHOUT_DESKTOP: Option<&str> = None;

fn mock_codex_projects(
    fixture: &StartFixture,
    starts: &mut SessionStarts,
    catalog: Result<Vec<codex::Project>>,
) {
    let executable = fixture.config().codex_executable.unwrap();
    crate::os::private_directory(executable.parent().unwrap()).unwrap();
    fs::write(executable, b"Codex fixture, never run").unwrap();
    starts.codex_projects.set_fixture(catalog);
}

pub(super) fn mock_empty_codex_projects(fixture: &StartFixture, starts: &mut SessionStarts) {
    mock_codex_projects(fixture, starts, Ok(Vec::new()));
}

#[test]
fn desktop_project_offers_and_lookup_choose_a_unique_deepest_root() {
    let fixture = StartFixture::new();
    let mut starts = fixture.starts();
    fixture.install("ChatGPT.app", false, true);
    let parent = fixture.home().join("project");
    let child = parent.join("child");
    let nested = child.join("nested");
    crate::os::private_directory(&nested).unwrap();
    let shallow_a = codex::Project::new("parent-a".into(), vec![parent.clone()]);
    let shallow_b = codex::Project::new("parent-b".into(), vec![parent.clone()]);
    let deep = codex::Project::new("child".into(), vec![child.clone()]);
    for projects in [
        vec![shallow_a.clone(), shallow_b.clone(), deep.clone()],
        vec![deep.clone(), shallow_a, shallow_b],
    ] {
        mock_codex_projects(&fixture, &mut starts, Ok(projects));
        let offers = starts.offers_for(&[json!({
            "source": "codex",
            "cwd": nested,
            "parentId": null
        })]);
        let offer = offers
            .iter()
            .find(|offer| offer.provider == Provider::Codex)
            .unwrap();
        assert_eq!(offer.folders, ["~/project/child/nested", "~/project/child"]);
        assert!(offer.without_folder);
        let request = StartRequest {
            provider: Provider::Codex,
            folder: starts.written(&nested),
            ..fixture.request()
        };
        assert_eq!(
            starts
                .codex_start_project(&nested, &request)
                .unwrap()
                .as_deref(),
            Some("child")
        );
        let parent_request = StartRequest {
            folder: starts.written(&parent),
            ..request
        };
        assert!(matches!(
            starts.codex_start_project(&parent, &parent_request),
            Err(Failure::Unavailable(reason)) if reason.contains("More than one Codex Desktop project")
        ));
    }
}

#[test]
fn stale_project_offers_and_explicit_models_are_refused_before_recording_a_start() {
    let fixture = StartFixture::new();
    let mut starts = fixture.starts();
    fixture.install("ChatGPT.app", false, true);
    let folder = fixture.home().join("project");
    let known = codex::Project::new("project".into(), vec![folder.clone()]);
    mock_codex_projects(&fixture, &mut starts, Ok(vec![known]));
    let offers = starts.offers_for(&[]);
    let request = StartRequest {
        provider: Provider::Codex,
        model: Some("gpt-6".into()),
        ..fixture.request()
    };
    assert!(matches!(
        starts.start_with(request.clone(), &offers),
        Err(Failure::Unsupported(_))
    ));
    starts.codex_projects.set_fixture(Ok(Vec::new()));
    assert!(matches!(
        starts.start_with(
            StartRequest { model: None, ..request.clone() },
            &offers
        ),
        Err(Failure::Unavailable(reason)) if reason.contains("No Codex Desktop project")
    ));
    starts
        .codex_projects
        .set_fixture(Err(Failure::Unavailable("catalog unavailable".into())));
    assert!(matches!(
        starts.start_with(
            StartRequest { model: None, ..request.clone() },
            &offers
        ),
        Err(Failure::Unavailable(reason)) if reason.contains("catalog unavailable")
    ));
    let independent = StartRequest {
        model: None,
        folder: String::new(),
        ..request
    };
    assert_eq!(starts.codex_start_project(&folder, &independent), Ok(None));
    let offers = starts.offers_for(&[]);
    let codex = offers
        .iter()
        .find(|offer| offer.provider == Provider::Codex)
        .unwrap();
    assert!(codex.folders.is_empty());
    assert!(codex.without_folder);
    assert_eq!(fs::read_dir(fixture.receipts()).unwrap().count(), 0);
}
