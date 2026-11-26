//! A link's messages: what a reader on one long connection asks, and what it
//! is told. The same three kinds whatever carries them (this crate's listener
//! over TLS, or a host's relay): a request, its answer, and the events of a
//! feed it subscribed to.
//!
//! A request is `{"id","method","path","cursor"?,"etag"?,"scope"?,"session"?,
//! "body"?}` and its answer `{"id","status","etag"?,"body"?}`. The paths are
//! the listener's own: `/v1/snapshot`, `/v1/changes`, `/v1/events`, and
//! `/v1/conversation` for one session's records; anything else goes to the
//! host. `scope` asks for less than everything (`Scope`): "list" or "light",
//! with `session` naming the one conversation that comes whole. A feed's events are `{"id","event","data","revision"?,"cursor"?}`,
//! `event` one of `revision`, `changes`, `reset`. `{"ping":n}` is answered
//! `{"pong":n}`, for a reader to learn the link is alive and how slow it is.
use crate::{cache::SnapshotCache, Cursor, Record};
use serde_json::{json, Value};
use std::sync::{Mutex, PoisonError};

/// How much of what is published a reader asked to be sent. A phone or a
/// browser takes the list first and one session's conversation when it is
/// opened: the turns of every session are most of a snapshot, and token usage
/// by the day is drawn nowhere on a list.
#[derive(Clone, Default)]
pub struct Scope {
    /// Leave out token usage, and every turn but the watched session's.
    list: bool,
    /// Send every record, but every turn other than the watched session's
    /// without its words (`Record::light`): for a reader that needs each
    /// turn's facts and reads one conversation at a time.
    light: bool,
    /// The session whose turns go along: its provider and its id.
    session: Option<(String, String)>,
}
impl Scope {
    fn of(request: &Value) -> Self {
        let session = &request["session"];
        Self {
            list: request["scope"] == "list",
            light: request["scope"] == "light",
            session: match (session["provider"].as_str(), session["sessionID"].as_str()) {
                (Some(provider), Some(id)) => Some((provider.into(), id.into())),
                _ => None,
            },
        }
    }
    fn turn_of<R: Record>(&self, record: &R) -> bool {
        self.session.as_ref().is_some_and(|(provider, id)| {
            record.kind() == "turn" && record.in_session(provider, id)
        })
    }
    fn sends<R: Record>(&self, record: &R) -> bool {
        !self.list || !matches!(record.kind(), "turn" | "usage") || self.turn_of(record)
    }
    /// The records as this reader takes them: some left out, some lightened.
    fn shape<R: Record>(&self, records: &mut Vec<R>) {
        records.retain(|record| self.sends(record));
        if self.light {
            for record in records.iter_mut() {
                if record.kind() == "turn" && !self.turn_of(record) {
                    *record = record.light();
                }
            }
        }
    }
}

/// A feed a reader subscribed to: the request it answers to, what the reader
/// was last told, and how much it asked for.
pub struct Feed {
    id: Value,
    revision: String,
    cursor: Option<Cursor>,
    scope: Scope,
}

/// What a message changes about its connection's feed.
pub enum Subscription {
    Same,
    Began(Feed),
    Ended,
}

/// The host's answer to a request that is not the feed's own: its status and
/// body, or none to leave it unclaimed (404).
pub type Host<'a> = dyn FnMut(&str, &str, &[u8]) -> Option<(u16, Option<Value>)> + 'a;

/// The answer to one message, and what it changes about the connection's feed.
pub fn answer<R: Record>(
    cache: &Mutex<SnapshotCache<R>>,
    message: &[u8],
    host: &mut Host<'_>,
) -> (Vec<u8>, Subscription) {
    let request: Value = serde_json::from_slice(message).unwrap_or_default();
    if !request["ping"].is_null() {
        return (
            json!({"pong":request["ping"]}).to_string().into_bytes(),
            Subscription::Same,
        );
    }
    let id = request["id"].clone();
    let plain = |reply: Value| (reply.to_string().into_bytes(), Subscription::Same);
    let status = |status: u16| plain(json!({"id":id,"status":status}));
    let (Some(method), Some(path)) = (request["method"].as_str(), request["path"].as_str()) else {
        return status(400);
    };
    let cursor = serde_json::from_value::<Cursor>(request["cursor"].clone())
        .ok()
        .filter(Cursor::valid);
    let scope = Scope::of(&request);
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    match (method, path) {
        ("GET", "/v1/snapshot") if scope.list || scope.light => {
            let Some(mut snapshot) = cache.snapshot() else {
                return status(503);
            };
            if request["etag"].as_str() == Some(snapshot.revision.as_str()) {
                return plain(json!({"id":id,"status":304,"etag":snapshot.revision}));
            }
            scope.shape(&mut snapshot.records);
            plain(json!({"id":id,"status":200,"etag":snapshot.revision,"body":snapshot}))
        }
        // One session's turns, as the snapshot would have listed them.
        ("GET", "/v1/conversation") => {
            if scope.session.is_none() {
                return status(400);
            }
            let Some(mut snapshot) = cache.snapshot() else {
                return status(503);
            };
            snapshot.records.retain(|record| scope.turn_of(record));
            plain(json!({"id":id,"status":200,"etag":snapshot.revision,"body":snapshot}))
        }
        ("GET", "/v1/snapshot") => {
            let Some(head) = cache.head() else {
                return status(503);
            };
            if request["etag"].as_str() == Some(head.revision.as_str()) {
                return plain(json!({"id":id,"status":304,"etag":head.revision}));
            }
            let Ok(Some(body)) = cache.body() else {
                return status(503);
            };
            // The snapshot is spliced in as the cache encoded it, not read again.
            let mut reply = json!({"id":id,"status":200,"etag":head.revision}).to_string();
            reply.pop();
            let mut reply = reply.into_bytes();
            reply.extend_from_slice(b",\"body\":");
            reply.extend_from_slice(&body);
            reply.push(b'}');
            (reply, Subscription::Same)
        }
        ("GET", "/v1/changes") => {
            let Some(cursor) = cursor else {
                return status(400);
            };
            match (cache.head(), cache.changes(&cursor)) {
                (Some(head), Some(mut changes)) => {
                    scope.shape(&mut changes.records);
                    plain(json!({"id":id,"status":200,"etag":head.revision,"body":changes}))
                }
                _ => status(410),
            }
        }
        ("GET", "/v1/events") => (
            json!({"id":id,"status":200}).to_string().into_bytes(),
            Subscription::Began(Feed {
                id,
                // Whatever the reader holds, the first event says where the
                // computer stands now.
                revision: String::new(),
                cursor,
                scope,
            }),
        ),
        ("DELETE", "/v1/events") => (
            json!({"id":id,"status":200}).to_string().into_bytes(),
            Subscription::Ended,
        ),
        _ => {
            drop(cache);
            let body = serde_json::to_vec(&request["body"]).unwrap_or_default();
            match host(method, path, &body) {
                Some((status, Some(body))) => plain(json!({"id":id,"status":status,"body":body})),
                Some((code, None)) => status(code),
                None => status(404),
            }
        }
    }
}

/// A feed's next event, once the published version moved past what the reader
/// was last told: the changes from where it stands, or `reset` when the cache
/// cannot go on from there and the reader takes the snapshot again.
pub fn next_event<R: Record>(cache: &Mutex<SnapshotCache<R>>, feed: &mut Feed) -> Option<Vec<u8>> {
    let cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    let head = cache.head()?;
    if head.revision == feed.revision {
        return None;
    }
    let notice = |event: &str| json!({"id":feed.id,"event":event,"data":{"revision":head.revision,"cursor":head.cursor}});
    let event = match &feed.cursor {
        None => notice("revision"),
        Some(since) if *since == head.cursor => notice("revision"),
        Some(since) => match cache.changes(since) {
            None => notice("reset"),
            Some(mut changes) => {
                feed.scope.shape(&mut changes.records);
                json!({"id":feed.id,"event":"changes","data":changes,"revision":head.revision,"cursor":head.cursor})
            }
        },
    };
    feed.revision = head.revision.clone();
    feed.cursor = Some(head.cursor.clone());
    Some(event.to_string().into_bytes())
}
