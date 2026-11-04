//! An event stream's frames: the change set from where the phone stands, or a
//! revision notice when it takes no changes or the change set is too large.
use crate::{cache::SnapshotCache, Record};
use serde_json::json;

/// A stream's change sets larger than this go as a revision notice instead, so
/// the phone fetches them from `/v1/changes`, compressed.
const STREAMED_CHANGES_LIMIT: usize = 256 * 1024;

/// What an event stream last told its phone.
pub(super) struct Told {
    pub(super) revision: String,
    pub(super) cursor: Option<crate::Cursor>,
}

/// The next frame of an event stream, once the published version moved past
/// what it last told. A stream that takes changes gets the change set from
/// where it stands (`changes`, its id the new cursor), or `reset` when the
/// cache cannot go on from there; any other stream, or the first frame of one
/// with no cursor, gets the revision notice (`revision`).
pub(super) fn next_frame<R: Record>(
    cache: &SnapshotCache<R>,
    changes: bool,
    told: &mut Told,
) -> Option<Vec<u8>> {
    let head = cache.head()?;
    if head.revision == told.revision {
        return None;
    }
    let notice = |event: &str| {
        format!(
            "event: {event}\ndata: {}\n\n",
            json!({"revision":head.revision,"cursor":head.cursor})
        )
        .into_bytes()
    };
    let frame = match told.cursor.as_ref().filter(|_| changes) {
        None => notice("revision"),
        Some(since) if *since == head.cursor => notice("revision"),
        Some(since) => match cache.changes(since) {
            None => notice("reset"),
            Some(range) => match serde_json::to_vec(&range) {
                Ok(body) if body.len() <= STREAMED_CHANGES_LIMIT => {
                    let mut frame =
                        format!("id: {}\nevent: changes\ndata: ", head.cursor.query_value())
                            .into_bytes();
                    frame.extend(body);
                    frame.extend(b"\n\n");
                    frame
                }
                _ => notice("revision"),
            },
        },
    };
    told.revision = head.revision.clone();
    told.cursor = Some(head.cursor.clone());
    Some(frame)
}
