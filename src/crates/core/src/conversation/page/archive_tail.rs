//! A tail page of a compressed transcript, read by decoding it from its start.
use super::*;

/// A tail page of a compressed transcript (every DeepSeek Harness log, a
/// cold Codex rollout): the page a plain transcript of the same text gives.
/// Compressed text has no seek, so it is decoded once from its start; the
/// lines before the last `TAIL` bytes are only looked at for the turn they
/// leave open. Text longer than the tail plus `reach`, as far back as a
/// plain tail looks for its turn, is not read: each look decodes it all.
pub(in crate::conversation) fn archive_tail(
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
