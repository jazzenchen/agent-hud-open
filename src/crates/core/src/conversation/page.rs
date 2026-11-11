//! A page of a session's transcript: its messages from the live log, or from
//! the tail of a compressed archive.
use super::*;

mod archive_tail;

pub(super) use archive_tail::archive_tail;

/// A row of a page as the apps read it.
fn row(item: LineItem) -> Value {
    // A call that names no tool shows no name.
    let tool = item.tool.map(|tool| {
        let name = (!tool.name.is_empty()).then_some(tool.name);
        json!({"id":tool.id,"name":name,"arguments":tool.arguments})
    });
    json!({"id":item.id,"parentId":item.parent,"role":item.role.as_str(),"text":item.text,"timestamp":item.timestamp,"toolCallId":item.tool_call_id,"toolCall":tool,"isPrompt":item.is_prompt})
}
pub(crate) fn page(
    path: &Path,
    source: &str,
    id: &str,
    cursor: u64,
    limit: usize,
    tail: bool,
    archives: &mut compressed::Archives,
    contexts: &mut Contexts,
) -> Result<Value, String> {
    // ZCode keeps every session's messages in one database; a page is a run
    // of them in order (`agents::zcode::conversation`), its cursor a place.
    if source == "zcode" {
        return database_page(path, source, id, cursor, limit, tail);
    }
    if source_db::accepts(path, source) {
        return Err(l10n::text("此来源的压缩日志不支持分页查看消息；用量和调用仍可查看", "Message paging is not available for this compact source; usage and calls remain available"));
    }
    let metadata = fs::metadata(path).map_err(|_| {
        l10n::text(
            "所选会话的日志不可用",
            "The selected session transcript is unavailable",
        )
    })?;
    let stamp = FileStamp {
        source: "conversation",
        length: metadata.len(),
        modified: metadata.modified().ok(),
    };
    let mut budget = 2 * 1024 * 1024u64;
    let archive = compressed::is_archive(path);
    if tail && archive {
        return archive_tail(path, source, limit, contexts, RECOVER_LIMIT);
    }
    let cursor = if tail {
        metadata.len().saturating_sub(TAIL)
    } else {
        cursor
    };
    // A page that goes on where this transcript's last page ended keeps its
    // turn; any other page looks back for it once it knows its first line.
    let remembered = (!tail).then(|| contexts.take(path, cursor)).flatten();
    let look_back = remembered.is_none() && !archive;
    let rule = turns::of(source);
    let reader_items = items::of(source);
    let mut context = remembered.unwrap_or_else(|| Context {
        path: path.into(),
        cursor,
        turn_id: None,
        started: None,
    });
    if archive
        && !archives
            .prepare(path, stamp, cursor, &mut budget)
            .map_err(|_| {
                l10n::text(
                    "无法解码所选会话的日志",
                    "Could not decode the selected session transcript",
                )
            })?
    {
        return Ok(
            json!({"messages":[],"messageCursor":cursor,"nextMessageCursor":cursor,"messagesPending":true,"messagesComplete":false}),
        );
    }
    let mut file;
    if archive {
        file = None;
    } else {
        let mut opened = File::open(path).map_err(|_| {
            l10n::text(
                "无法读取所选会话的日志",
                "Could not read the selected session transcript",
            )
        })?;
        opened
            .seek(SeekFrom::Start(cursor))
            .map_err(|_| "Invalid transcript cursor")?;
        file = Some(BufReader::new(opened));
    }
    let reader: &mut dyn BufRead = if archive {
        archives.reader()
    } else {
        file.as_mut().unwrap()
    };
    let mut position = cursor;
    if tail && cursor > 0 {
        let (_, consumed, _) =
            line(reader).map_err(|_| "Could not read transcript tail boundary")?;
        position += consumed;
        budget = budget.saturating_sub(consumed);
    }
    if look_back && position > 0 {
        context.recover(rule, path, position);
    }
    let mut messages = Vec::new();
    let mut complete = false;
    while budget > 0 && (tail || messages.len() < limit) {
        let (bytes, count, ended) = line(reader).map_err(|_| {
            l10n::text(
                "无法读取所选会话的日志",
                "Could not read the selected session transcript",
            )
        })?;
        if count == 0 || !ended {
            complete = true;
            break;
        }
        let start = position;
        position += count;
        budget = budget.saturating_sub(count);
        if bytes.len() > MAX_LINE {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            context.read(rule, &value);
            for item in (reader_items.read)(&value, start) {
                let mut entry = row(item);
                entry["turnId"] = json!(context.turn_id);
                entry["turnStartedAt"] = json!(context.started);
                messages.push(entry);
            }
            if tail && messages.len() > limit {
                messages.drain(..messages.len() - limit);
            }
        }
    }
    context.cursor = position;
    contexts.keep(context);
    if archive {
        archives.position(position);
        if complete {
            archives.forget(path);
        }
    }
    Ok(
        json!({"messages":messages,"messageCursor":cursor,"nextMessageCursor":(tail||!complete).then_some(position),"endMessageCursor":position,"messagesTruncatedBefore":tail&&cursor>0,"messagesPending":false,"messagesComplete":complete}),
    )
}

/// A page of a session whose messages a database holds (ZCode): `cursor` is
/// a message's place, and a tail page is the last `limit` messages.
fn database_page(
    path: &Path,
    source: &str,
    id: &str,
    cursor: u64,
    limit: usize,
    tail: bool,
) -> Result<Value, String> {
    use agent_hud_agents::zcode::conversation;
    let unreadable = || {
        l10n::text(
            "无法读取所选会话的日志",
            "Could not read the selected session transcript",
        )
    };
    let database =
        agent_hud_agents::sqlite::ReadOnlyDatabase::open(path).map_err(|_| unreadable())?;
    let read = if tail {
        conversation::last_messages(&database.connection, id, limit)
    } else {
        conversation::messages(&database.connection, id, cursor, limit)
    }
    .map_err(|_| unreadable())?;
    let rule = turns::of(source);
    let reader_items = items::of(source);
    let mut context = Context {
        path: path.into(),
        cursor,
        turn_id: None,
        started: None,
    };
    let mut messages = Vec::new();
    let first = read.lines.first().map_or(read.next, |(place, _)| *place);
    for (place, value) in &read.lines {
        context.read(rule, value);
        for item in (reader_items.read)(value, *place) {
            let mut entry = row(item);
            entry["turnId"] = json!(context.turn_id);
            entry["turnStartedAt"] = json!(context.started);
            messages.push(entry);
        }
    }
    let start = if tail { first } else { cursor };
    Ok(json!({
        "messages":messages,
        "messageCursor":start,
        "nextMessageCursor":(tail || !read.complete).then_some(read.next),
        "endMessageCursor":read.next,
        "messagesTruncatedBefore":tail && read.earlier,
        "messagesPending":false,
        "messagesComplete":read.complete,
    }))
}

/// How much of a transcript's end a tail page reads.
pub(super) const TAIL: u64 = 2 * 1024 * 1024;
