use super::*;

/// The stat-only look between passes reads no file and changes nothing:
/// it says whether a held log no longer matches the listing.
#[test]
fn a_stale_look_sees_a_grown_or_gone_log_and_leaves_the_listing_alone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects");
    let path = root.join("p/s.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{}\n").unwrap();
    let roots = vec![(root, "claude")];
    let mut files = LogFiles::default();
    files.refresh(&roots, &[]);
    let active = vec![path.clone()];
    assert!(!files.stale(&roots, &active));
    assert!(files.next_reconcile().is_some());
    fs::write(&path, "{}\n{}\n").unwrap();
    assert!(files.stale(&roots, &active));
    assert!(files.stale(&roots, &active), "the look changed nothing");
    files.refresh(&roots, &active);
    assert!(files.changed.contains(&path));
    assert!(!files.stale(&roots, &active));
    fs::remove_file(&path).unwrap();
    assert!(files.stale(&roots, &active));
    assert!(
        !files.stale(&roots, &[]),
        "only the held logs are looked at"
    );
}

#[test]
fn claude_and_codex_list_no_hidden_files_and_codex_only_rollouts() {
    let roots = vec![
        (PathBuf::from("/h/.claude/projects"), "claude"),
        (PathBuf::from("/h/.codex/sessions"), "codex"),
    ];
    assert!(accepts(Path::new("/h/.claude/projects/p/s.jsonl"), &roots));
    assert!(!accepts(
        Path::new("/h/.claude/projects/p/.s.jsonl"),
        &roots
    ));
    assert!(!accepts(
        Path::new("/h/.claude/projects/.trash/s.jsonl"),
        &roots
    ));
    assert!(accepts(
        Path::new("/h/.codex/sessions/2026/10/21/rollout-a.jsonl"),
        &roots
    ));
    assert!(accepts(
        Path::new("/h/.codex/sessions/2026/10/21/rollout-a.jsonl.zst"),
        &roots
    ));
    assert!(!accepts(
        Path::new("/h/.codex/sessions/2026/10/21/notes.jsonl"),
        &roots
    ));
}

#[test]
fn claude_lists_sessions_and_subagents_but_not_workflow_journals() {
    let roots = vec![(PathBuf::from("/h/.claude/projects"), "claude")];
    let session = "/h/.claude/projects/p/a13ae0b7-3ba4-4e35-af3f-80f8fb300c67";
    assert!(accepts(Path::new(&format!("{session}.jsonl")), &roots));
    assert!(accepts(
        Path::new(&format!("{session}/subagents/agent-a1.jsonl")),
        &roots
    ));
    assert!(accepts(
        Path::new(&format!(
            "{session}/subagents/workflows/wf_93f53f2f-2ce/agent-a2.jsonl"
        )),
        &roots
    ));
    assert!(!accepts(
        Path::new(&format!(
            "{session}/subagents/workflows/wf_93f53f2f-2ce/journal.jsonl"
        )),
        &roots
    ));
}
