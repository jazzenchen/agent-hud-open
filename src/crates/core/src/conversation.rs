//! Explicit, bounded transcript pages. Message bodies are read only for the
//! selected session and never stored in ingestion checkpoints or snapshots.
use super::*;
use crate::l10n;
use agent_hud_agents::transcript::items::{self, LineItem};
use agent_hud_agents::transcript::turns::{self, TurnStart, Turns};
/// The turn a page's messages belong to where the page begins: the turn read
/// last before `cursor` in `path`. Which lines name a turn is the client's
/// own rule (`Agent::turns`); keeping and finding it is the same for all.
#[derive(Default)]
pub(super) struct Context {
    path: PathBuf,
    cursor: u64,
    turn_id: Option<String>,
    started: Option<Value>,
}
impl Context {
    fn read(&mut self, turns: &Turns, value: &Value) {
        let Some(start) = (turns.start)(value) else {
            return;
        };
        if start.whole {
            self.turn_id = start.turn;
            self.started = start.started;
            return;
        }
        if self.turn_id != start.turn {
            self.started = None;
        }
        self.turn_id = start.turn;
        if start.started.is_some() {
            self.started = start.started;
        }
    }
    /// The turn the line at `end` belongs to, read backwards from there: the
    /// nearest line that names a turn and, where that line does not start
    /// it, the line of the same turn that says when it began. Only lines the
    /// client says may name a turn are decoded; a turn that began more than
    /// `RECOVER_LIMIT` back stays unnamed.
    /// Returns where the line that starts that turn begins, when it was found.
    fn recover(&mut self, turns: &Turns, path: &Path, end: u64) -> Option<u64> {
        let Ok(mut file) = File::open(path) else {
            return None;
        };
        let mut found: Option<TurnStart> = None;
        let mut begins: Option<u64> = None;
        let mut settle = |line: &[u8], at: u64| -> bool {
            if line.is_empty() || line.len() > MAX_LINE || !(turns.may_name)(line) {
                return false;
            }
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                return false;
            };
            let Some(start) = (turns.start)(&value) else {
                return false;
            };
            match &mut found {
                None => {
                    let settled = start.whole || start.started.is_some();
                    found = Some(start);
                    if settled {
                        begins = Some(at);
                    }
                    settled
                }
                Some(seen) if !start.whole && start.turn == seen.turn => {
                    seen.started = start.started;
                    if seen.started.is_some() {
                        begins = Some(at);
                    }
                    seen.started.is_some()
                }
                Some(_) => true,
            }
        };
        // `rest` is the end of the line whose start is not read yet; a line
        // past MAX_LINE is skipped, as pages skip it.
        let mut rest = Vec::new();
        let mut oversized = false;
        let mut position = end;
        let mut done = false;
        while position > 0 && end - position < RECOVER_LIMIT && !done {
            let size = position.min(RECOVER_CHUNK);
            position -= size;
            let mut chunk = vec![0; size as usize];
            if file.seek(SeekFrom::Start(position)).is_err()
                || std::io::Read::read_exact(&mut file, &mut chunk).is_err()
            {
                return None;
            }
            let mut stop = chunk.len();
            while let Some(newline) = chunk[..stop].iter().rposition(|b| *b == b'\n') {
                if !oversized {
                    let mut line = chunk[newline + 1..stop].to_vec();
                    line.extend_from_slice(&rest);
                    if settle(&line, position + newline as u64 + 1) {
                        done = true;
                        break;
                    }
                }
                oversized = false;
                rest.clear();
                stop = newline;
            }
            if done {
                break;
            }
            if oversized || stop + rest.len() > MAX_LINE {
                oversized = true;
                rest.clear();
            } else {
                let mut line = chunk[..stop].to_vec();
                line.extend_from_slice(&rest);
                rest = line;
            }
        }
        if !done && position == 0 && !oversized {
            settle(&rest, 0);
        }
        if let Some(found) = found {
            self.turn_id = found.turn;
            self.started = found.started;
        }
        begins
    }
}
/// Where the line that starts the turn holding the line at `end` begins in
/// `path`, by `source`'s own rule; None when no such line lies within
/// `RECOVER_LIMIT` before it.
pub(crate) fn turn_start_before(source: &str, path: &Path, end: u64) -> Option<u64> {
    Context::default().recover(turns::of(source), path, end)
}
/// How far back a page looks for the turn it begins in, and in what steps.
const RECOVER_LIMIT: u64 = 128 * 1024 * 1024;
const RECOVER_CHUNK: u64 = 1024 * 1024;
/// Each transcript's turn where its last page ended, so that pages of other
/// sessions read in between leave it, and a page that goes on where the last
/// one ended needs no look back.
#[derive(Default)]
pub(super) struct Contexts {
    by_path: HashMap<PathBuf, (u64, Context)>,
    uses: u64,
}
impl Contexts {
    const KEPT: usize = 256;
    fn take(&mut self, path: &Path, cursor: u64) -> Option<Context> {
        match self.by_path.get(path) {
            Some((_, context)) if context.cursor == cursor => {
                self.by_path.remove(path).map(|(_, context)| context)
            }
            _ => None,
        }
    }
    fn keep(&mut self, context: Context) {
        self.uses += 1;
        self.by_path
            .insert(context.path.clone(), (self.uses, context));
        if self.by_path.len() > Self::KEPT {
            if let Some(oldest) = self
                .by_path
                .iter()
                .min_by_key(|(_, (used, _))| *used)
                .map(|(path, _)| path.clone())
            {
                self.by_path.remove(&oldest);
            }
        }
    }
}
/// A row of a page as the apps read it.
fn row(item: LineItem) -> Value {
    // A call that names no tool shows no name.
    let tool = item.tool.map(|tool| {
        let name = (!tool.name.is_empty()).then_some(tool.name);
        json!({"id":tool.id,"name":name,"arguments":tool.arguments})
    });
    json!({"id":item.id,"parentId":item.parent,"role":item.role.as_str(),"text":item.text,"timestamp":item.timestamp,"toolCallId":item.tool_call_id,"toolCall":tool,"isPrompt":item.is_prompt})
}
pub(super) fn page(
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
const TAIL: u64 = 2 * 1024 * 1024;

/// A tail page of a compressed transcript (every DeepSeek Harness log, a
/// cold Codex rollout): the page a plain transcript of the same text gives.
/// Compressed text has no seek, so it is decoded once from its start; the
/// lines before the last `TAIL` bytes are only looked at for the turn they
/// leave open. Text longer than the tail plus `reach`, as far back as a
/// plain tail looks for its turn, is not read: each look decodes it all.
fn archive_tail(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(rows: &[Value], filler: usize) -> (tempfile::TempDir, PathBuf, u64) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("log.jsonl");
        let mut file = File::create(&path).unwrap();
        let mut end = 0;
        for (n, row) in rows.iter().enumerate() {
            let line = format!("{row}\n");
            file.write_all(line.as_bytes()).unwrap();
            end += line.len() as u64;
            if n == 0 {
                // Lines that name no turn, past one look-back step, and one
                // line longer than any page reads.
                for _ in 0..filler {
                    let line = format!("{}\n", json!({"type":"other","pad":"x".repeat(4096)}));
                    file.write_all(line.as_bytes()).unwrap();
                    end += line.len() as u64;
                }
                let long = format!(
                    "{}\n",
                    json!({"type":"user","pad":"y".repeat(MAX_LINE + 10)})
                );
                file.write_all(long.as_bytes()).unwrap();
                end += long.len() as u64;
            }
        }
        (temp, path, end)
    }

    fn recovered(source: &str, path: &Path, end: u64) -> (Option<String>, Option<Value>) {
        let mut context = Context::default();
        context.recover(turns::of(source), path, end);
        (context.turn_id, context.started)
    }

    #[test]
    fn looking_back_finds_a_claude_prompt_far_behind() {
        let (_temp, path, end) = write(
            &[
                json!({"type":"user","timestamp":"T0","uuid":"u0","promptId":"p1","message":{"role":"user","content":"Go"}}),
                json!({"type":"assistant","timestamp":"T1","message":{"role":"assistant","content":[{"type":"text","text":"Working."}]}}),
                json!({"type":"user","timestamp":"T2","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"x","content":"ok"}]}}),
            ],
            600,
        );
        assert_eq!(
            recovered("claude", &path, end),
            (Some("p1".into()), Some(json!("T0")))
        );
    }

    #[test]
    fn looking_back_finds_when_a_codex_turn_began() {
        let (_temp, path, end) = write(
            &[
                json!({"type":"event_msg","timestamp":"T0","payload":{"type":"task_started","turn_id":"t1"}}),
                json!({"type":"response_item","timestamp":"T1","payload":{"type":"message","turn_id":"t1"}}),
            ],
            600,
        );
        assert_eq!(
            recovered("codex", &path, end),
            (Some("t1".into()), Some(json!("T0")))
        );
    }

    #[test]
    fn looking_back_stops_at_the_turn_before() {
        let (_temp, path, end) = write(
            &[
                json!({"type":"event_msg","timestamp":"T0","payload":{"type":"task_started","turn_id":"t0"}}),
                json!({"type":"response_item","timestamp":"T1","payload":{"type":"message","turn_id":"t1"}}),
            ],
            0,
        );
        assert_eq!(recovered("codex", &path, end), (Some("t1".into()), None));
    }

    #[test]
    fn a_line_at_the_file_start_counts() {
        let (_temp, path, end) = write(
            &[
                json!({"type":"user","timestamp":"T0","uuid":"u0","promptId":"p1","message":{"role":"user","content":"Go"}}),
            ],
            0,
        );
        assert_eq!(
            recovered("claude", &path, end).0,
            Some("p1".into()),
            "the long line after it is skipped, the prompt still counts"
        );
        let only = path.with_file_name("only.jsonl");
        let row = format!(
            "{}\n",
            json!({"type":"user","timestamp":"T0","uuid":"u0","promptId":"p1","message":{"role":"user","content":"Go"}})
        );
        fs::write(&only, &row).unwrap();
        assert_eq!(
            recovered("claude", &only, row.len() as u64).0,
            Some("p1".into())
        );
    }

    /// The client that writes these logs.
    const HARNESS: &str = agent_hud_agents::deepseek::AGENT.id;

    /// A DeepSeek Harness log's text: two turns, the second a message from
    /// another agent answered after its reasoning, with `before` and `after`
    /// lines that name no turn around that message.
    fn harness_log(before: usize, after: usize) -> String {
        let pad = |n: usize| json!({"type":"session-log-deepseek/delivery-accepted","seq":n,"time":1793127112000_i64,"data":{"pad":"x".repeat(4096)}});
        let mut rows = vec![
            json!({"type":"session","version":4,"id":"session-f8d6","createdAt":1793123855625_i64,"cwd":"/w"}),
            json!({"type":"turn/start","seq":5,"time":1793124207977_i64,"data":{"turn":1}}),
            json!({"type":"user/message","seq":8,"time":1793124208006_i64,"data":{"content":[{"type":"text","text":"Ask me one question."}],"source":{"kind":"user"},"role":"user"}}),
            json!({"type":"assistant/message","seq":25,"time":1793124282761_i64,"data":{"turn":1,"step":2,"message":{"role":"assistant","content":[{"type":"reasoning","text":""},{"type":"text","text":"You chose Blue."}]}}}),
            json!({"type":"turn/end","seq":27,"time":1793124282770_i64,"data":{"turn":1,"reason":{"kind":"completed"}}}),
        ];
        rows.extend((0..before).map(pad));
        rows.extend([
            json!({"type":"turn/start","seq":44,"time":1793127112393_i64,"data":{"turn":3}}),
            json!({"type":"user/message","seq":48,"time":1793127112417_i64,"data":{"content":[{"type":"text","text":"[Agent HUD · from Claude Code · Test · m-b0bfc3510306]\n\nWhich model are you?"}],"source":{"kind":"user","rpcId":"m-b0bfc3510306"},"role":"user"}}),
            json!({"type":"user/message","seq":50,"time":1793127112417_i64,"data":{"content":[{"type":"text","text":"Time sampled while preparing turn 3, step 1"}],"source":{"kind":"time-context"},"role":"user"}}),
        ]);
        rows.extend((0..after).map(pad));
        rows.extend([
            json!({"type":"assistant/message","seq":54,"time":1793127114066_i64,"data":{"turn":3,"step":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":"The user wants one sentence."},{"type":"text","text":"收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。"}]}}}),
            json!({"type":"turn/end","seq":56,"time":1793127114067_i64,"data":{"turn":3,"reason":{"kind":"completed"}}}),
        ]);
        rows.iter().map(|row| format!("{row}\n")).collect()
    }

    /// `text` compressed as Harness writes it: a frame every few lines.
    fn compress(text: &str) -> Vec<u8> {
        let lines = text.split_inclusive('\n').collect::<Vec<_>>();
        lines
            .chunks(7)
            .flat_map(|chunk| zstd::stream::encode_all(chunk.concat().as_bytes(), 3).unwrap())
            .collect()
    }

    /// The same text as a plain and a compressed DeepSeek log.
    fn both(text: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let plain = temp.path().join("session.v4.jsonl");
        let archive = temp.path().join("session.v4.jsonl.zstd");
        fs::write(&plain, text).unwrap();
        fs::write(&archive, compress(text)).unwrap();
        (temp, plain, archive)
    }

    fn tail(path: &Path, limit: usize) -> Result<Value, String> {
        page(
            path,
            HARNESS,
            0,
            limit,
            true,
            &mut compressed::Archives::default(),
            &mut Contexts::default(),
        )
    }

    fn reply(page: &Value) -> &Value {
        page["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|row| row["role"] == "assistant")
            .unwrap_or_else(|| panic!("no reply in {page:#}"))
    }

    #[test]
    fn a_compressed_tail_is_the_tail_its_plain_text_gives() {
        // The message's turn starting inside the tail, before it, and a log
        // shorter than a tail.
        for (before, after) in [(600, 0), (0, 600), (0, 0)] {
            let text = harness_log(before, after);
            let (_temp, plain, archive) = both(&text);
            for limit in [200, 1] {
                let mut compressed = tail(&archive, limit).unwrap();
                let mut same = tail(&plain, limit).unwrap();
                // A plain tail's budget runs out right at the end, before
                // it reads that nothing follows.
                assert_eq!(compressed["messagesComplete"], true);
                compressed["messagesComplete"].take();
                same["messagesComplete"].take();
                assert_eq!(compressed, same);
                assert_eq!(compressed["endMessageCursor"], text.len());
                assert_eq!(
                    compressed["messagesTruncatedBefore"],
                    text.len() as u64 > TAIL
                );
                let reply = reply(&compressed);
                assert_eq!(
                    reply["text"],
                    "收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。"
                );
                assert_eq!(reply["turnId"], "3", "{before} {after}");
                assert_eq!(reply["turnStartedAt"], 1793127112393_i64);
            }
            let rows = tail(&archive, 200).unwrap()["messages"].clone();
            let message = rows
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["text"].as_str().unwrap().contains("· m-b0bfc3510306]"));
            if after == 0 {
                let message = message.expect("the message lies within the tail");
                assert_eq!(message["isPrompt"], true);
                assert_eq!(message["turnId"], "3");
            }
        }
    }

    #[test]
    fn a_compressed_tail_ends_before_a_frame_still_being_written() {
        let text = harness_log(0, 0);
        let (_temp, _plain, archive) = both(&text);
        let mut bytes = compress(&text);
        let next = zstd::stream::encode_all(
            format!("{}\n", json!({"type":"turn/start","data":{"turn":4}})).as_bytes(),
            3,
        )
        .unwrap();
        bytes.extend_from_slice(&next[..next.len() / 2]);
        assert_eq!(
            zstd::stream::decode_all(&bytes[..]).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        fs::write(&archive, bytes).unwrap();
        let page = tail(&archive, 200).unwrap();
        assert_eq!(page["endMessageCursor"], text.len());
        assert_eq!(reply(&page)["turnId"], "3");
    }

    #[test]
    fn a_compressed_transcript_past_its_reach_has_no_tail() {
        let (_temp, _plain, archive) = both(&harness_log(600, 0));
        let mut contexts = Contexts::default();
        assert!(archive_tail(&archive, HARNESS, 200, &mut contexts, 4096).is_err());
        assert!(archive_tail(&archive, HARNESS, 200, &mut contexts, TAIL).is_ok());
    }

    #[test]
    fn a_compressed_log_pages_forward_to_its_reply() {
        let text = harness_log(600, 0);
        let (_temp, _plain, archive) = both(&text);
        let mut archives = compressed::Archives::default();
        let mut contexts = Contexts::default();
        let mut cursor = 0;
        let mut rows = Vec::new();
        loop {
            let page = page(
                &archive,
                HARNESS,
                cursor,
                100,
                false,
                &mut archives,
                &mut contexts,
            )
            .unwrap();
            assert_eq!(page["messagesPending"], false);
            rows.extend(page["messages"].as_array().unwrap().iter().cloned());
            match page["nextMessageCursor"].as_u64() {
                Some(next) => cursor = next,
                None => break,
            }
        }
        let seen = rows
            .iter()
            .map(|row| {
                (
                    row["role"].as_str().unwrap(),
                    row["turnId"].as_str().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            seen,
            [
                ("user", "1"),
                ("assistant", "1"),
                ("user", "3"),
                ("user", "3"),
                ("assistant", "3")
            ]
        );
        assert_eq!(
            rows.iter().filter(|row| row["isPrompt"] == true).count(),
            2,
            "Harness's context is no prompt"
        );
    }
}
