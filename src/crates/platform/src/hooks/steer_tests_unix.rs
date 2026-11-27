use super::*;
use serde_json::json;

const QUICK: Duration = Duration::from_millis(5);

fn me() -> Process {
    Process::of(std::process::id() as i32)
}
fn codex(event: &str, turn: &str) -> Call {
    call(
        "codex",
        &json!({"hook_event_name":event,"session_id":"s1","turn_id":turn,"source":"compact"}),
    )
    .unwrap()
}
/// A hook of the session, run to its end.
fn hook(data: &Path, event: &str, turn: &str) -> Option<String> {
    handle(data, &codex(event, turn), (me(), me()), QUICK)
}
/// A hook that waits beside the turn, as Codex runs it.
fn waiting(data: &Path, turn: &str) -> std::thread::JoinHandle<Option<String>> {
    let (data, turn) = (data.to_owned(), turn.to_owned());
    let steers = Steers::new(&data);
    let hook = std::thread::spawn({
        let (data, turn) = (data.clone(), turn.clone());
        move || hook(&data, "PreToolUse", &turn)
    });
    for _ in 0..400 {
        if steers.stands("s1", &turn, me()) {
            return hook;
        }
        std::thread::sleep(QUICK);
    }
    panic!("the hook never waited");
}

#[test]
fn a_waiting_hook_tells_the_turn_and_never_says_the_words() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let hook = waiting(&data, "t1");
    let steers = Steers::new(&data);
    steers.leave("s1", "m-1", "SECRET WORDS", 1).unwrap();
    let notice = hook.join().unwrap().unwrap();
    assert!(notice.contains("m-1") && notice.contains("read_message"));
    assert!(!notice.contains("SECRET"), "{notice}");
    // One-shot: the read's own tool call starts the next.
    assert!(!steers.stands("s1", "t1", me()));
    assert_eq!(steers.read("s1", Some("t1"))[0].text, "SECRET WORDS");
    let hook = waiting(&data, "t1");
    steers.leave("s1", "m-2", "more", 2).unwrap();
    assert!(hook.join().unwrap().unwrap().contains("m-2"));
}

#[test]
fn the_end_of_the_turn_carries_it_on_for_what_is_unread_and_ends_it_otherwise() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let steers = Steers::new(&data);
    steers.begin("s1", "t1", (me(), me()), 1);
    steers.leave("s1", "m-1", "words", 1).unwrap();
    assert!(hook(&data, "Stop", "t1").unwrap().contains("m-1"));
    assert_eq!(steers.in_progress("s1").as_deref(), Some("t1"));
    // Told and not read: once more, then the turn may end.
    assert!(hook(&data, "Stop", "t1").is_some());
    assert_eq!(hook(&data, "Stop", "t1"), None);
    assert_eq!(steers.in_progress("s1"), None);
    assert_eq!(steers.state("s1", "m-1"), crate::steers::Steered::Missed);
}

#[test]
fn a_waiting_hook_leaves_when_its_turn_ends_is_stopped_or_gives_way() {
    for event in ["Stop", "Interrupt"] {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        Steers::new(&data).turn_on(1);
        let waiter = waiting(&data, "t1");
        assert_eq!(hook(&data, event, "t1"), None);
        assert_eq!(waiter.join().unwrap(), None, "{event}");
    }
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let waiter = waiting(&data, "t1");
    Steers::new(&data).begin("s1", "t2", (me(), me()), 1);
    assert_eq!(waiter.join().unwrap(), None);
}

#[test]
fn a_hook_whose_client_quit_takes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let gone = Process {
        pid: i32::MAX - 7,
        started: Some(1),
    };
    let steers = Steers::new(&data);
    steers.begin("s1", "t1", (me(), me()), 1);
    steers.leave("s1", "m-1", "words", 1).unwrap();
    let told = handle(&data, &codex("PreToolUse", "t1"), (me(), gone), QUICK);
    assert_eq!(told, None);
    steers.begin("s1", "t1", (me(), me()), 1);
    assert_eq!(steers.tell("s1", "t1"), ["m-1"], "still untold");
}

#[test]
fn a_second_hook_of_the_turn_does_not_wait_beside_the_first() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let first = waiting(&data, "t1");
    let other = Process::of(std::os::unix::process::parent_id() as i32);
    assert_eq!(
        handle(
            &data,
            &codex("UserPromptSubmit", "t1"),
            (other, me()),
            QUICK
        ),
        None
    );
    Steers::new(&data).leave("s1", "m-1", "w", 1).unwrap();
    assert!(first.join().unwrap().is_some());
}

#[test]
fn a_compacted_turn_is_told_again() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let steers = Steers::new(&data);
    assert_eq!(hook(&data, "SessionStart", ""), None);
    steers.begin("s1", "t1", (me(), me()), 1);
    steers.leave("s1", "m-1", "words", 1).unwrap();
    steers.tell("s1", "t1");
    steers.read("s1", Some("t1"));
    let notice = hook(&data, "SessionStart", "").unwrap();
    assert!(notice.contains("compacted") && notice.contains("m-1"));
    assert_eq!(steers.read("s1", Some("t1")).len(), 1);
}

#[test]
fn one_hook_waits_for_a_turn_and_a_dead_one_is_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let steers = Steers::new(&temp.path().join("data"));
    steers.turn_on(1);
    assert!(steers.stand(
        "s1",
        "t1",
        Process {
            pid: i32::MAX - 7,
            started: Some(1),
        }
    ));
    assert!(steers.stand("s1", "t1", me()), "that one quit");
    let parent = Process::of(std::os::unix::process::parent_id() as i32);
    assert!(!steers.stand("s1", "t1", parent));
    assert!(steers.stand("s1", "t2", parent), "a newer turn's");
    assert!(!steers.stands("s1", "t1", me()));
    steers.stand_down("s1", "t1", me());
    assert!(steers.stands("s1", "t2", parent));
    steers.stand_down("s1", "t2", parent);
    assert!(steers.stand("s1", "t2", me()));
}

#[test]
fn the_turn_is_not_told_of_what_it_is_reading() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    Steers::new(&data).turn_on(1);
    let steers = Steers::new(&data);
    steers.begin("s1", "t1", (me(), me()), 1);
    steers.leave("s1", "m-1", "words", 1).unwrap();
    let mut call = codex("PreToolUse", "t1");
    call.reading = true;
    let hook = std::thread::spawn({
        let data = data.clone();
        move || handle(&data, &call, (me(), me()), QUICK)
    });
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(steers.read("s1", Some("t1")).len(), 1);
    std::thread::sleep(SETTLE);
    assert!(!hook.is_finished(), "nothing left to tell");
    steers.leave("s1", "m-2", "more", 2).unwrap();
    assert!(hook.join().unwrap().unwrap().contains("m-2"));
}

#[test]
fn turned_off_in_the_middle_of_a_turn_nothing_waits_tells_or_reads() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let steers = Steers::new(&data);
    steers.turn_on(1);
    let waiter = waiting(&data, "t1");
    steers.turned(false, 2);
    assert_eq!(waiter.join().unwrap(), None);
    assert!(!steers.stands("s1", "t1", me()));
    // One left before: not told at the turn's end, and not read any more.
    steers.turn_on(3);
    steers.leave("s1", "m-1", "words", 4).unwrap();
    steers.turned(false, 5);
    assert_eq!(steers.state("s1", "m-1"), crate::steers::Steered::Missed);
    assert!(steers.read("s1", Some("t1")).is_empty());
    assert_eq!(hook(&data, "Stop", "t1"), None);
    assert_eq!(hook(&data, "PreToolUse", "t1"), None);
    assert_eq!(hook(&data, "SessionStart", ""), None);
    assert_eq!(steers.leave("s1", "m-2", "words", 6).unwrap(), None);
}
