//! A page of a session's transcript: its messages from the live log, or from
//! the tail of a compressed archive.
use super::*;

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
    cursor: u64,
    limit: usize,
    tail: bool,
    archives: &mut compressed::Archives,
    contexts: &mut Contexts,
) -> Result<Value, String> {
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

/// How much of a transcript's end a tail page reads.
pub(super) const TAIL: u64 = 2 * 1024 * 1024;

/// A tail page of a compressed transcript (every DeepSeek Harness log, a
/// cold Codex rollout): the page a plain transcript of the same text gives.
/// Compressed text has no seek, so it is decoded once from its start; the
/// lines before the last `TAIL` bytes are only looked at for the turn they
/// leave open. Text longer than the tail plus `reach`, as far back as a
/// plain tail looks for its turn, is not read: each look decodes it all.
pub(super) fn archive_tail(
    path: &Path,
    source: &str,
    limit: usize,
    contexts: &mut Contexts,
    reach: u64,
) -> Result<Value, String> {
    let undecodable = || {
        l10n::text(
            "无法解码所选会话的日志",
            "Could not decode the selected session transcript",
        )
    };
    let file = File::open(path).map_err(|_| {
        l10n::text(
            "无法读取所选会话的日志",
            "Could not read the selected session transcript",
        )
    })?;
    let mut decoder = zstd::stream::read::Decoder::new(file).map_err(|_| undecodable())?;
    decoder.window_log_max(27).map_err(|_| undecodable())?; // As `Archives` reads.
    let mut reader = BufReader::with_capacity(64 * 1024, decoder);
    let rule = turns::of(source);
    // The turn open before the first line kept, and the lines that may yet
    // fall within the tail, each with where it starts.
    let mut context = Context {
        path: path.into(),
        ..Context::default()
    };
    let mut kept = VecDeque::<(u64, Vec<u8>)>::new();
    let pass = |context: &mut Context, bytes: &[u8]| {
        if bytes.len() <= MAX_LINE && (rule.may_name)(bytes) {
            if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                context.read(rule, &value);
            }
        }
    };
    let mut position = 0;
    let mut length = 0;
    loop {
        let (bytes, count, ended) = match line(&mut reader) {
            Ok(read) => read,
            // A frame still being written: the text ends where it begins.
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err(undecodable()),
        };
        length = position + count;
        if count == 0 || !ended {
            break;
        }
        if length > reach + TAIL {
            return Err(l10n::text(
                "所选会话的压缩日志太长，无法从末尾查看消息",
                "The selected session's compressed transcript is too long to page from its end",
            ));
        }
        kept.push_back((position, bytes));
        position = length;
        // A line that starts a whole tail before what is read so far lies
        // before the tail.
        while kept
            .front()
            .is_some_and(|(start, _)| start + TAIL < position)
        {
            let (_, bytes) = kept.pop_front().unwrap();
            pass(&mut context, &bytes);
        }
    }
    // A plain tail starts `TAIL` before the end, after the line holding that
    // byte.
    let cursor = length.saturating_sub(TAIL);
    while cursor > 0 && kept.front().is_some_and(|(start, _)| *start <= cursor) {
        let (_, bytes) = kept.pop_front().unwrap();
        pass(&mut context, &bytes);
    }
    let reader_items = items::of(source);
    let mut messages = Vec::new();
    for (start, bytes) in kept {
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
            if messages.len() > limit {
                messages.drain(..messages.len() - limit);
            }
        }
    }
    context.cursor = position;
    contexts.keep(context);
    Ok(
        json!({"messages":messages,"messageCursor":cursor,"nextMessageCursor":position,"endMessageCursor":position,"messagesTruncatedBefore":cursor>0,"messagesPending":false,"messagesComplete":true}),
    )
}
