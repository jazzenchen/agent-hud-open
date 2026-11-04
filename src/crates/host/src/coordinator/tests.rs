use super::*;

#[test]
fn callbacks_before_the_window_worker_starts_keep_their_initial_facts() {
    let owner = Coordinator::default();
    let initial = Arc::new(json!({"settings":{"hudVisible":false},"generatedAt":"first"}));
    owner.wake(SNAPSHOT, |pending| pending.snapshot = Some(initial));
    let notice = UpdateNotice {
        version: "0.7.0".into(),
        settings_page: "updates".into(),
    };
    owner.update_notice(Some(notice.clone()));
    // A later clock or a repeated version must not downgrade or discard
    // the first projection before its consumer has taken it.
    let later = Arc::new(json!({"settings":{"hudVisible":false},"generatedAt":"later"}));
    owner.wake(CLOCK, |pending| pending.snapshot = Some(later.clone()));
    owner.update_notice(Some(notice.clone()));
    let pending = owner.pending.lock().unwrap();
    assert_eq!(
        pending.changed & (SCREENS | DISPLAY | SNAPSHOT | UPDATE),
        SCREENS | DISPLAY | SNAPSHOT | UPDATE
    );
    assert_eq!(pending.snapshot.as_ref().unwrap().as_ref(), later.as_ref());
    assert_eq!(*owner.notice.lock().unwrap(), Some(notice));
}
