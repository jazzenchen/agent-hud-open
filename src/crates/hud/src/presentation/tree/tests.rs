use super::*;

#[test]
fn a_control_is_put_back_where_it_stood() {
    let mut list = vec!["blocks".to_owned(), "sessions".into(), "tokens".into()];
    keep_places(
        &mut list,
        &[
            "blocks".into(),
            "note".into(),
            "tokens".into(),
            "sessions".into(),
        ],
        &["note".into()],
    );
    assert_eq!(list, ["blocks", "note", "sessions", "tokens"]);
}
