//! A pending Claude hook alone follows its own transcript. Initial results are
//! history; only a newly appended result for a matched tool-use settles it.
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
const HISTORY: u64 = 4 * 1024 * 1024;
pub(super) struct Transcript {
    file: File,
    tool: String,
    input: Value,
    calls: HashSet<String>,
    partial: Vec<u8>,
    dirty: Arc<AtomicBool>,
    _watcher: RecommendedWatcher,
}
impl Transcript {
    pub fn new(payload: &Value) -> Option<Self> {
        let path = Path::new(payload.get("transcript_path")?.as_str()?);
        let tool = payload.get("tool_name")?.as_str()?.to_owned();
        if tool.is_empty() {
            return None;
        }
        let mut file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }
        if !crate::os::user_transcript(path, &metadata) {
            return None;
        }
        let dirty = Arc::new(AtomicBool::new(true));
        let flag = dirty.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok() {
                    flag.store(true, Ordering::Release);
                }
            })
            .ok()?;
        watcher.watch(path, RecursiveMode::NonRecursive).ok()?;
        let start = metadata.len().saturating_sub(HISTORY);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut initial = Vec::new();
        (&mut file).take(HISTORY).read_to_end(&mut initial).ok()?;
        if start > 0 {
            if let Some(end) = initial.iter().position(|b| *b == b'\n') {
                initial.drain(..=end);
            } else {
                initial.clear();
            }
        }
        let mut state = Self {
            file,
            tool,
            input: payload["tool_input"].clone(),
            calls: HashSet::new(),
            partial: Vec::new(),
            dirty,
            _watcher: watcher,
        };
        state.lines(&initial);
        Some(state)
    }
    pub fn settled(&mut self) -> bool {
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return false;
        }
        let mut bytes = Vec::new();
        if (&mut self.file)
            .take(HISTORY)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return false;
        }
        if bytes.len() as u64 == HISTORY {
            self.dirty.store(true, Ordering::Release);
        }
        self.lines(&bytes)
    }
    fn lines(&mut self, bytes: &[u8]) -> bool {
        self.partial.extend_from_slice(bytes);
        let mut settled = false;
        let mut start = 0;
        while let Some(length) = self.partial[start..].iter().position(|b| *b == b'\n') {
            let end = start + length;
            if let Ok(record) = serde_json::from_slice::<Value>(&self.partial[start..end]) {
                if let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) {
                    for block in blocks {
                        match block["type"].as_str() {
                            Some("tool_use")
                                if block["name"].as_str() == Some(&self.tool)
                                    && block["input"] == self.input =>
                            {
                                if let Some(id) = block["id"].as_str() {
                                    self.calls.insert(id.into());
                                }
                            }
                            Some("tool_result") => {
                                if let Some(id) = block["tool_use_id"].as_str() {
                                    if self.calls.remove(id) {
                                        settled = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            start = end + 1;
        }
        self.partial.drain(..start);
        // No unbounded transcript retention in a hook. An oversized partial
        // line cannot prove settlement, so discard it rather than guessing.
        if self.partial.len() > HISTORY as usize {
            self.partial.clear();
        }
        settled
    }
}
