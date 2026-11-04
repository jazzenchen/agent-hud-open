use super::cards::unknown;
use super::patches::apply_patches;
use super::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};

struct Followed {
    owner: String,
    revision: i64,
    asked: Instant,
}
const RETRY: Duration = Duration::from_secs(30);

pub(super) fn work(shared: Arc<Shared>) {
    let mut backoff = Duration::from_secs(1);
    while shared.active.load(Ordering::Acquire) {
        if shared.wanted().is_empty() {
            shared.pause(Duration::from_secs(1));
            continue;
        }
        let began = Instant::now();
        if let Ok(mut connection) =
            DesktopIpc::connect(&endpoint(&shared.data), Duration::from_secs(5))
        {
            if let Ok(handle) = connection.shutdown_handle() {
                shared
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .connection = Some(handle);
            }
            let _ = serve(&shared, &mut connection);
        }
        shared.forget_connection();
        if !shared.active.load(Ordering::Acquire) {
            break;
        }
        backoff = if began.elapsed() > Duration::from_secs(30) {
            Duration::from_secs(1)
        } else {
            (backoff * 2).min(Duration::from_secs(15))
        };
        shared.pause(backoff);
    }
    shared.forget_connection();
}
fn follow(
    connection: &mut DesktopIpc,
    client: &str,
    thread: &str,
    owner: &str,
    following: bool,
) -> Result<(), IpcError> {
    connection.post(json!({"type":"broadcast","method":"thread-stream-following-changed","version":1,"sourceClientId":client,"targetClientIds":[owner],"params":{"hostId":"local","conversationId":thread,"following":following}}))
}
fn serve(shared: &Arc<Shared>, connection: &mut DesktopIpc) -> Result<(), IpcError> {
    let initialized = connection.request(
        "initialize",
        0,
        "initializing-client",
        None,
        json!({"clientType":"agent-hud"}),
        None,
    )?;
    let client = initialized["result"]["clientId"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or(IpcError::Protocol)?
        .to_string();
    let mut followed: HashMap<String, Followed> = HashMap::new();
    let mut searches: HashMap<String, (String, Instant)> = HashMap::new();
    let mut search_after: HashMap<String, Instant> = HashMap::new();
    let mut foreign = HashSet::new();
    while shared.active.load(Ordering::Acquire) {
        let now = Instant::now();
        let wanted = shared.wanted().into_iter().collect::<HashSet<_>>();
        if wanted.is_empty() {
            break;
        }
        for thread in followed
            .keys()
            .filter(|thread| !wanted.contains(*thread))
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Some(entry) = followed.remove(&thread) {
                let _ = follow(connection, &client, &thread, &entry.owner, false);
            }
            shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .known
                .remove(&thread);
            shared.publish_waiting();
        }
        searches.retain(|_, (thread, sent)| {
            wanted.contains(thread) && now.duration_since(*sent) < RETRY
        });
        search_after.retain(|thread, _| wanted.contains(thread));
        foreign.retain(|thread| wanted.contains(thread));
        let searching = searches
            .values()
            .map(|(thread, _)| thread.clone())
            .collect::<HashSet<_>>();
        for thread in &wanted {
            if foreign.contains(thread)
                || searching.contains(thread)
                || search_after.get(thread).is_some_and(|at| *at > now)
            {
                continue;
            }
            let request = super::super::desktop_io::request_id()?;
            connection.renew(Duration::from_secs(5));
            connection.post(json!({"type":"request","requestId":request,"sourceClientId":client,"version":1,"method":"thread-owner-discovery","timeoutMs":30_000,"params":{"hostId":"local","conversationId":thread}}))?;
            searches.insert(request, (thread.clone(), now));
        }
        for (thread, entry) in &mut followed {
            if !shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .known
                .contains_key(thread)
                && now.duration_since(entry.asked) >= RETRY
            {
                connection.renew(Duration::from_secs(5));
                follow(connection, &client, thread, &entry.owner, true)?;
                entry.asked = now;
            }
        }
        shared.answers();
        if !connection.wait_for_message(Duration::from_millis(200))? {
            continue;
        }
        connection.renew(Duration::from_secs(120));
        let mut message = connection.receive_trimmed(1024 * 1024 * 1024, 16 * 1024)?;
        connection.renew(Duration::from_secs(5));
        if connection.decline_discovery(&message)? {
            continue;
        }
        match message["type"].as_str() {
            Some("response") => {
                let Some(request) = message["requestId"].as_str() else {
                    continue;
                };
                let Some((thread, _)) = searches.remove(request) else {
                    continue;
                };
                let owner = message["handledByClientId"]
                    .as_str()
                    .filter(|v| !v.is_empty());
                if message["resultType"] != "success"
                    || owner.is_none()
                    || !wanted.contains(&thread)
                {
                    // Only the hub's word that nobody owns the thread ends a
                    // following, and the old owner is told it is no longer
                    // followed (agent-hud-native-open CodexDesktopStream.unfollow).
                    if message["error"] == "no-client-found" {
                        if let Some(previous) = followed.remove(&thread) {
                            let _ = follow(connection, &client, &thread, &previous.owner, false);
                        }
                        unknown(shared, &thread);
                    }
                    search_after.insert(thread, now + RETRY);
                    continue;
                }
                let owner = owner.unwrap();
                search_after.insert(thread.clone(), now + Duration::from_secs(60));
                if followed
                    .get(&thread)
                    .is_some_and(|entry| entry.owner == owner)
                {
                    continue;
                }
                if let Some(previous) = followed.remove(&thread) {
                    let _ = follow(connection, &client, &thread, &previous.owner, false);
                }
                unknown(shared, &thread);
                follow(connection, &client, &thread, owner, true)?;
                followed.insert(
                    thread,
                    Followed {
                        owner: owner.into(),
                        revision: 0,
                        asked: now,
                    },
                );
            }
            Some("broadcast") => {
                let params = &message["params"];
                match message["method"].as_str() {
                    Some("thread-stream-state-changed") => {
                        if params["hostId"] != "local" {
                            continue;
                        }
                        let Some(thread) = params["conversationId"].as_str() else {
                            continue;
                        };
                        let Some(entry) = followed.get_mut(thread) else {
                            continue;
                        };
                        if message["sourceClientId"] != entry.owner {
                            continue;
                        }
                        if message["version"] != 11 {
                            let owner = entry.owner.clone();
                            let _ = follow(connection, &client, thread, &owner, false);
                            followed.remove(thread);
                            foreign.insert(thread.to_owned());
                            unknown(shared, thread);
                            continue;
                        }
                        let change = &params["change"];
                        match change["type"].as_str() {
                            Some("snapshot") => {
                                if !change["conversationState"].is_object() {
                                    return Err(IpcError::Protocol);
                                }
                                let revision =
                                    change["revision"].as_i64().ok_or(IpcError::Protocol)?;
                                entry.revision = revision;
                                let thread = thread.to_string();
                                let owner = entry.owner.clone();
                                let state = message["params"]["change"]["conversationState"].take();
                                shared.reconcile(&thread, &owner, state);
                            }
                            Some("patches") => {
                                let revision = change["revision"].as_i64();
                                if revision.is_some_and(|revision| revision <= entry.revision) {
                                    continue;
                                }
                                let Some(known) = shared
                                    .state
                                    .lock()
                                    .unwrap_or_else(PoisonError::into_inner)
                                    .known
                                    .remove(thread)
                                else {
                                    continue;
                                };
                                let mut state = known.state;
                                if let Some(revision) = revision.filter(|_| {
                                    change["baseRevision"].as_i64() == Some(entry.revision)
                                        && apply_patches(&change["patches"], &mut state).is_ok()
                                }) {
                                    entry.revision = revision;
                                    shared.reconcile(thread, &entry.owner, state);
                                } else {
                                    unknown(shared, thread);
                                    if now.duration_since(entry.asked) >= RETRY {
                                        follow(connection, &client, thread, &entry.owner, true)?;
                                        entry.asked = now;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Some("client-status-changed") => {
                        if params["status"] == "connected" {
                            continue;
                        }
                        let Some(owner) = params["clientId"].as_str() else {
                            continue;
                        };
                        for thread in followed
                            .iter()
                            .filter(|(_, entry)| entry.owner == owner)
                            .map(|(thread, _)| thread.clone())
                            .collect::<Vec<_>>()
                        {
                            followed.remove(&thread);
                            search_after.insert(thread.clone(), now + Duration::from_secs(5));
                            unknown(shared, &thread);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    connection.renew(Duration::from_secs(1));
    for (thread, entry) in followed {
        let _ = follow(connection, &client, &thread, &entry.owner, false);
    }
    Ok(())
}
