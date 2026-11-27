use super::*;

#[test]
fn a_link_at_the_old_place_counts_as_moved() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    let new = temp.path().join("new");
    fs::create_dir(&new).unwrap();
    std::os::unix::fs::symlink(&new, &old).unwrap();
    assert!(!unmoved(&old));
    assert_eq!(in_use(&old, &new), new);
}
