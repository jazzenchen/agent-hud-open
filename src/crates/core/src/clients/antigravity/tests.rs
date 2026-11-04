use super::*;
use std::fs;

#[test]
fn annotation_titles_decode_escaped_utf8_and_skip_other_title_words() {
    let temp = tempfile::tempdir().unwrap();
    let conversations = temp.path().join("conversations");
    fs::create_dir_all(temp.path().join("annotations")).unwrap();
    fs::create_dir_all(&conversations).unwrap();
    fs::write(
        temp.path().join("annotations/c1.pbtxt"),
        "subtitle_hint { }\ntitle \"x\"\ntitle: \"\\344\\275\\240\\345\\245\\275 \\xe4\\xb8\\x96\\u754c\"\n",
    )
    .unwrap();
    assert_eq!(
        title(&conversations.join("c1.db")).as_deref(),
        Some("你好 世界")
    );
}
