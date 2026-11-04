use super::*;
use std::cell::RefCell;

const ID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

/// Services 1..: `loaded` lists the ones with the conversation in memory,
/// `opens` the ones that can open it; every open is written down.
fn pick(
    services: &[u8],
    loaded: &[u8],
    lists: impl Fn(u8) -> Result<Value, String>,
    opens: &[u8],
) -> (Result<(u8, Value), MessageError>, Vec<u8>) {
    let opened = RefCell::new(Vec::new());
    let picked = pick_service(
        services,
        ID,
        |n| {
            if loaded.contains(n) {
                Ok(json!({"trajectorySummaries": {(ID): {}}}))
            } else {
                lists(*n)
            }
        },
        |n| {
            opened.borrow_mut().push(*n);
            if opens.contains(n) {
                Ok(json!({"trajectory": {"service": n}}))
            } else {
                Err(format!("service {n}: trajectory not found"))
            }
        },
    );
    (picked, opened.into_inner())
}

#[test]
fn a_conversation_no_service_has_loaded_is_opened_by_the_first_that_can() {
    // A service with nothing loaded leaves the map out.
    let (picked, opened) = pick(&[1], &[], |_| Ok(json!({})), &[1]);
    assert_eq!(picked, Ok((1, json!({"trajectory": {"service": 1}}))));
    assert_eq!(opened, [1]);
    let (picked, opened) = pick(&[1, 2, 3], &[], |_| Ok(json!({})), &[2, 3]);
    assert_eq!(picked.unwrap().0, 2);
    assert_eq!(opened, [1, 2]);
}

#[test]
fn a_service_that_has_it_open_comes_first() {
    let (picked, opened) = pick(&[1, 2], &[2], |_| Ok(json!({})), &[1, 2]);
    assert_eq!(picked.unwrap().0, 2);
    assert_eq!(opened, [2]);
}

#[test]
fn an_unreadable_list_never_refuses_a_service() {
    for list in [
        Err("offline".to_string()),
        Ok(json!([])),
        Ok(json!({"trajectorySummaries": null})),
        Ok(json!({"trajectorySummaries": []})),
    ] {
        let (picked, _) = pick(&[1], &[], |_| list.clone(), &[1]);
        assert_eq!(picked.unwrap().0, 1);
    }
}

#[test]
fn when_no_service_can_open_it_the_last_service_says_why() {
    let (picked, opened) = pick(&[1, 2], &[], |_| Ok(json!({})), &[]);
    assert_eq!(
        picked,
        Err(MessageError::Unavailable(
            "service 2: trajectory not found".into()
        ))
    );
    assert_eq!(opened, [1, 2]);
    assert_eq!(
        pick(&[], &[], |_| Ok(json!({})), &[]).0,
        Err(MessageError::Unavailable(
            "No running Antigravity service is available".into()
        ))
    );
}

#[test]
fn only_canonical_conversation_ids_are_accepted() {
    assert!(crate::hooks::origins::is_uuid(ID));
    assert!(!crate::hooks::origins::is_uuid("../../other"));
    assert!(!crate::hooks::origins::is_uuid(
        "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeeg"
    ));
}
