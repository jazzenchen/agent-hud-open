//! The ledger's data version: the release whose reading rules its logs were
//! read by (`agent_hud_agents::DATA_VERSIONS`). A ledger with no logs is
//! indexed; one recorded under another version is indexed again. Either run
//! is reported with how far it has got, in bytes and in days, until every
//! listed log has been read by this build's rules, and then this build's
//! version is recorded. What removed logs counted stays.
use super::*;
use rusqlite::OptionalExtension;
use std::time::SystemTime;

/// Why the ledger is being read through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Run {
    /// A ledger with no logs yet.
    First,
    /// A ledger read under another data version.
    Again,
}

impl Run {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Run::First => "first",
            Run::Again => "reindex",
        }
    }
}

/// The data version the ledger records; None before one was recorded.
pub(crate) fn recorded(db: &Connection) -> Option<String> {
    db.query_row(
        "SELECT value FROM ingestion_meta WHERE key='data_version'",
        [],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub(crate) fn record(db: &Connection, version: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO ingestion_meta(key,value) VALUES('data_version',?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [version],
    )
    .map(|_| ())
}

impl AgentHudCore {
    /// After a collection pass: once a pass has listed every log since launch
    /// and none waits to be read (a log pausing after a failed read does not
    /// hold the run open), the ledger holds this build's data version.
    pub(crate) fn finish_indexing_run(&mut self) {
        if self.reads.listing.reconciled {
            self.listed_once = true;
        }
        if !self.listed_once || self.waiting_reads() > 0 {
            return;
        }
        let Some(db) = &self.database else {
            return;
        };
        if self.data_version.as_deref() != Some(DATA_VERSION) {
            if record(db, DATA_VERSION).is_err() {
                return;
            }
            self.data_version = Some(DATA_VERSION.into());
        }
        self.indexing_run = None;
    }
}

/// The run a launch begins with. A ledger from before data versions were
/// recorded holds the rules its checkpoints name: it is read again only when
/// a log it still lists was read by other rules.
pub(crate) fn run_at_launch(
    recorded: Option<&str>,
    logs: &HashMap<PathBuf, LogHead>,
) -> Option<Run> {
    match recorded {
        Some(version) if version == DATA_VERSION => None,
        Some(_) => Some(Run::Again),
        None if logs.is_empty() => Some(Run::First),
        None => logs
            .values()
            .any(|head| !head.gone && head.parser_version != PARSER_VERSION)
            .then_some(Run::Again),
    }
}

/// How much of the listed logs this build's rules have read, in bytes
/// (decoded text for compressed logs): read and in all. A source database is
/// counted by its logs' heads, not its bytes, so it is left out.
pub(crate) fn progress(
    files: &HashMap<PathBuf, FileStamp>,
    logs: &HashMap<PathBuf, LogHead>,
) -> (u64, u64) {
    files
        .iter()
        .filter(|(path, file)| !source_db::accepts(path, file.source))
        .fold((0, 0), |(read, all), (path, file)| {
            let done = logs
                .get(path)
                .filter(|head| head.parser_version == PARSER_VERSION)
                .map_or(0, |head| head.offset.min(file.length));
            (read + done, all + file.length)
        })
}

/// How far back a run has read, by when the logs were last written, in ms:
/// the oldest listed log's last write, which is as far back as the history
/// goes, and the newest last write among the logs this build's rules have
/// not read to their end, after which every log has been read. Logs are
/// read newest first, so the second moves back toward the first; None once
/// none is left. A log that only grew since it was read through, or pauses
/// after a failed read, does not hold it back. A source database is left
/// out: its last write says nothing of how far back it goes.
pub(crate) fn reach(
    files: &HashMap<PathBuf, FileStamp>,
    logs: &HashMap<PathBuf, LogHead>,
    pausing: impl Fn(&Path) -> bool,
) -> (Option<i64>, Option<i64>) {
    let ms = |at: SystemTime| DateTime::<Utc>::from(at).timestamp_millis();
    let mut oldest = None::<i64>;
    let mut unread = None::<i64>;
    for (path, file) in files {
        let Some(at) = file.modified.map(ms) else {
            continue;
        };
        if source_db::accepts(path, file.source) {
            continue;
        }
        oldest = Some(oldest.map_or(at, |oldest| oldest.min(at)));
        let read_through = logs.get(path).is_some_and(|head| {
            head.parser_version == PARSER_VERSION
                && if compressed::is_archive(path) {
                    head.decoded_complete
                } else {
                    head.offset >= head.length || head.waiting_for_more
                }
        });
        if !read_through && !pausing(path) {
            unread = Some(unread.map_or(at, |unread| unread.max(at)));
        }
    }
    (oldest, unread)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn head(version: u32, offset: u64, length: u64) -> LogHead {
        let mut log = Log::new(PathBuf::from("/l.jsonl"), "claude");
        log.parser_version = version;
        log.offset = offset;
        log.length = length;
        log.modified = Some(SystemTime::now());
        LogHead::from_log(&log)
    }

    fn days_ago(days: u64) -> SystemTime {
        SystemTime::now() - Duration::from_secs(days * 86_400)
    }

    #[test]
    fn a_launch_indexes_a_new_ledger_and_reads_another_versions_again() {
        let none = HashMap::new();
        let current = HashMap::from([(PathBuf::from("/a"), head(PARSER_VERSION, 10, 10))]);
        let older = HashMap::from([(PathBuf::from("/a"), head(PARSER_VERSION - 1, 10, 10))]);
        assert_eq!(run_at_launch(None, &none), Some(Run::First));
        assert_eq!(run_at_launch(Some(DATA_VERSION), &older), None);
        assert_eq!(run_at_launch(Some("0.5.0"), &current), Some(Run::Again));
        assert_eq!(
            run_at_launch(None, &current),
            None,
            "a ledger from before versions were recorded holds this build's rules"
        );
        assert_eq!(run_at_launch(None, &older), Some(Run::Again));
        let mut old = head(PARSER_VERSION - 1, 10, 10);
        old.modified = Some(days_ago(400));
        assert_eq!(
            run_at_launch(None, &HashMap::from([(PathBuf::from("/a"), old.clone())])),
            Some(Run::Again),
            "a log of any age is read again"
        );
        old.gone = true;
        assert_eq!(
            run_at_launch(None, &HashMap::from([(PathBuf::from("/a"), old)])),
            None,
            "a removed log keeps what it counted"
        );
    }

    fn stamp(length: u64, modified: SystemTime) -> FileStamp {
        FileStamp {
            source: "claude",
            length,
            modified: Some(modified),
        }
    }

    #[test]
    fn progress_counts_the_bytes_this_builds_rules_read() {
        let now = SystemTime::now();
        let files = HashMap::from([
            (PathBuf::from("/done"), stamp(100, now)),
            (PathBuf::from("/half"), stamp(200, now)),
            (PathBuf::from("/older"), stamp(300, now)),
            (PathBuf::from("/new"), stamp(400, now)),
            (std::env::temp_dir().join("past"), stamp(500, days_ago(400))),
        ]);
        let logs = HashMap::from([
            (PathBuf::from("/done"), head(PARSER_VERSION, 100, 100)),
            (PathBuf::from("/half"), head(PARSER_VERSION, 50, 200)),
            (PathBuf::from("/older"), head(PARSER_VERSION - 1, 300, 300)),
        ]);
        assert_eq!(progress(&files, &logs), (150, 1_500));
    }

    #[test]
    fn reach_runs_from_the_oldest_log_to_the_newest_one_left() {
        let ms = |at: SystemTime| DateTime::<Utc>::from(at).timestamp_millis();
        let at = |name: &str| std::env::temp_dir().join(name);
        let (today, week, month, year) = (days_ago(0), days_ago(7), days_ago(30), days_ago(365));
        let files = HashMap::from([
            (at("today"), stamp(500, today)),
            (at("week"), stamp(200, week)),
            (at("month"), stamp(300, month)),
            (at("year"), stamp(400, year)),
        ]);
        let mut logs = HashMap::from([
            // Read through, then written to again: only new lines wait.
            (at("today"), head(PARSER_VERSION, 100, 100)),
            // Half read under these rules.
            (at("week"), head(PARSER_VERSION, 100, 200)),
            // Read through under older rules.
            (at("month"), head(PARSER_VERSION - 1, 300, 300)),
        ]);
        let none = |_: &Path| false;
        assert_eq!(reach(&files, &logs, none), (Some(ms(year)), Some(ms(week))));
        logs.insert(at("week"), head(PARSER_VERSION, 200, 200));
        assert_eq!(reach(&files, &logs, none).1, Some(ms(month)));
        assert_eq!(
            reach(&files, &logs, |path: &Path| path == at("month").as_path()).1,
            Some(ms(year)),
            "a log pausing after a failed read does not hold it back"
        );
        logs.insert(at("month"), head(PARSER_VERSION, 300, 300));
        logs.insert(at("year"), head(PARSER_VERSION, 400, 400));
        assert_eq!(reach(&files, &logs, none), (Some(ms(year)), None));
        assert_eq!(reach(&HashMap::new(), &logs, none), (None, None));
    }
}
