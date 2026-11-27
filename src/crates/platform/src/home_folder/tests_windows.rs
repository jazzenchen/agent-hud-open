use super::*;

#[test]
fn the_old_place_becomes_a_junction_to_the_new_one() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old folder");
    let new = temp.path().join("home/.agenthud/data");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("settings.json"), "{}").unwrap();
    assert!(relocate(&old, &new).unwrap());
    assert!(!unmoved(&old));
    // Through the junction, the old path reads the moved files and resolves
    // to the same folder, so a pipe named from it matches.
    assert_eq!(fs::read_to_string(old.join("settings.json")).unwrap(), "{}");
    assert_eq!(
        fs::canonicalize(&old).unwrap(),
        fs::canonicalize(&new).unwrap()
    );
}
