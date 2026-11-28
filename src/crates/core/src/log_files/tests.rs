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

#[test]
fn unchanged_codex_archives_reuse_cached_decoded_length_without_reopening() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    let archive = root.join("2026/10/21/rollout-cache.jsonl.zst");
    fs::create_dir_all(archive.parent().unwrap()).unwrap();

    let write_zst = |content: &[u8]| {
        let mut encoder = zstd::Encoder::new(Vec::new(), 1).unwrap();
        encoder
            .set_pledged_src_size(Some(content.len() as u64))
            .unwrap();
        std::io::Write::write_all(&mut encoder, content).unwrap();
        fs::write(&archive, encoder.finish().unwrap()).unwrap();
    };

    write_zst(b"{\"line\":1}\n");
    let roots = vec![(root, "codex")];
    let mut files = LogFiles::default();

    let before = crate::compressed::header_reads();
    files.refresh(&roots, &[]);
    let after_first = crate::compressed::header_reads();
    assert_eq!(
        after_first - before,
        1,
        "first scan reads the zstd header once"
    );
    assert_eq!(files.files[&archive].length, 11);

    // Force a full reconcile (`changes = None`) without touching the archive on disk.
    files.note_changes(None);
    files.refresh(&roots, std::slice::from_ref(&archive));
    assert!(!files.stale(&roots, std::slice::from_ref(&archive)));
    assert_eq!(
        crate::compressed::header_reads(),
        after_first,
        "reconcile and stale checks reuse cached frame length with zero File::open calls"
    );

    // Rewriting the archive with a different decoded length invalidates the cache.
    write_zst(b"{\"line\":1}\n{\"line\":222}\n");
    files.note_changes(None);
    files.refresh(&roots, &[]);
    assert_eq!(files.files[&archive].length, 24);
    assert_eq!(crate::compressed::header_reads(), after_first + 1);
}
