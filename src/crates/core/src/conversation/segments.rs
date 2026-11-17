//! A page of a session Codex split across several rollouts
//! (`usage::segment`): its segments end to end, earliest first, as one run of
//! cursors. Codex never writes to an earlier segment again, so a cursor taken
//! before it split the session still finds the same line after the split.
use super::*;
use crate::l10n::tr;

/// How long a segment's text is; a compressed one's comes from its frame
/// header, without decoding it.
fn text_length(path: &Path) -> Option<u64> {
    if compressed::is_archive(path) {
        compressed::text_length(path)
    } else {
        fs::metadata(path).ok().map(|metadata| metadata.len())
    }
}

/// Moves a page of the segment that begins at `base` to where it lies in the
/// whole: its cursors, and the ids its rows take from where their lines
/// begin (`o<offset>`), which would otherwise repeat from one segment to the
/// next.
fn shift(page: &mut Value, base: u64) {
    if base == 0 {
        return;
    }
    for key in ["messageCursor", "nextMessageCursor", "endMessageCursor"] {
        if let Some(at) = page[key].as_u64() {
            page[key] = json!(at + base);
        }
    }
    for row in page["messages"].as_array_mut().into_iter().flatten() {
        for key in ["id", "parentId"] {
            let at = row[key]
                .as_str()
                .and_then(|id| id.strip_prefix('o'))
                .and_then(|offset| offset.parse::<u64>().ok());
            if let Some(at) = at {
                row[key] = json!(format!("o{}", at + base));
            }
        }
    }
}

fn rows(page: &mut Value) -> Vec<Value> {
    match page["messages"].take() {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    }
}

/// A page of the session whose logs are `paths`, earliest segment first; one
/// log reads as `page` does. An earlier segment whose text cannot be read is
/// left out.
#[allow(clippy::too_many_arguments)]
pub(crate) fn segmented_page(
    paths: &[&Path],
    source: &str,
    id: &str,
    cursor: u64,
    limit: usize,
    tail: bool,
    archives: &mut compressed::Archives,
    contexts: &mut Contexts,
) -> Result<Value, String> {
    let Some((newest, earlier)) = paths.split_last() else {
        return Err(tr!("The selected session transcript is unavailable"));
    };
    // Each segment with where it begins in the whole.
    let mut parts = Vec::new();
    let mut base = 0;
    for path in earlier {
        if let Some(length) = text_length(path) {
            parts.push((*path, base));
            base += length;
        }
    }
    parts.push((*newest, base));
    if parts.len() == 1 {
        return page(newest, source, id, cursor, limit, tail, archives, contexts);
    }
    if tail {
        // The newest segment's end, then the ends of the ones before it while
        // the page has room and reaches back to its segment's start.
        let mut index = parts.len() - 1;
        let (path, base) = parts[index];
        let mut result = page(path, source, id, 0, limit, true, archives, contexts)?;
        shift(&mut result, base);
        let mut messages = rows(&mut result);
        while index > 0 && messages.len() < limit && result["messagesTruncatedBefore"] != true {
            index -= 1;
            let (path, base) = parts[index];
            let left = limit - messages.len();
            let mut before = page(path, source, id, 0, left, true, archives, contexts)?;
            shift(&mut before, base);
            let mut earlier = rows(&mut before);
            earlier.append(&mut messages);
            messages = earlier;
            result["messageCursor"] = before["messageCursor"].clone();
            result["messagesTruncatedBefore"] =
                json!(before["messagesTruncatedBefore"] == true || index > 0);
        }
        result["messages"] = json!(messages);
        return Ok(result);
    }
    let mut index = parts
        .iter()
        .rposition(|(_, start)| *start <= cursor)
        .unwrap_or(0);
    let mut local = cursor - parts[index].1;
    let mut messages = Vec::new();
    loop {
        let (path, base) = parts[index];
        let last = index + 1 == parts.len();
        let left = limit - messages.len();
        let mut part = page(path, source, id, local, left, false, archives, contexts)?;
        shift(&mut part, base);
        // A segment still decoding after rows of the one before: those rows
        // first, and the next page starts at this segment.
        if part["messagesPending"] == true && !messages.is_empty() {
            return Ok(
                json!({"messages":messages,"messageCursor":cursor,"nextMessageCursor":base,"endMessageCursor":base,"messagesTruncatedBefore":false,"messagesPending":false,"messagesComplete":false}),
            );
        }
        messages.extend(rows(&mut part));
        let ended = !last && part["messagesComplete"] == true;
        if ended && messages.len() < limit {
            index += 1;
            local = 0;
            continue;
        }
        part["messages"] = json!(messages);
        part["messageCursor"] = json!(cursor);
        if ended {
            // The page filled up right at an earlier segment's end: the next
            // one starts the segment after it.
            let next = parts[index + 1].1;
            part["nextMessageCursor"] = json!(next);
            part["endMessageCursor"] = json!(next);
            part["messagesComplete"] = json!(false);
        }
        return Ok(part);
    }
}
