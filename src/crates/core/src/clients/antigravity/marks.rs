//! Where a read of an Antigravity database stands: the rows each table's
//! next read begins from, and the cursor and watermark that carry them.
use serde_json::{json, Value};

/// Rows read again before where the last read of a table ended. Antigravity
/// rewrites the rows of a turn in progress (its usage, its steps' messages),
/// so a read that resumes takes the last of them up again; each row counts
/// under its own key, so a row read twice counts once.
pub(super) const TAIL: i64 = 200;

/// Where a read stands besides its stage and row: where steps and the second
/// pass over the turns begin (the tables' start on a whole read, the tail of
/// the last read on a resumed one), and the last row read of each table.
#[derive(Clone, Copy)]
pub(super) struct Marks {
    pub(super) steps_from: i64,
    pub(super) turns_from: i64,
    pub(super) steps_last: i64,
    pub(super) turns_last: i64,
}

impl Marks {
    pub(super) fn of(cursor: &Value) -> Self {
        let at = |key: &str| cursor[key].as_i64().unwrap_or(i64::MIN);
        Self {
            steps_from: at("steps"),
            turns_from: at("turns"),
            steps_last: at("stepsLast"),
            turns_last: at("turnsLast"),
        }
    }

    pub(super) fn cursor(&self, stage: i64, after: i64) -> Value {
        let mut cursor = json!({ "stage": stage });
        for (key, value) in [
            ("after", after),
            ("steps", self.steps_from),
            ("turns", self.turns_from),
            ("stepsLast", self.steps_last),
            ("turnsLast", self.turns_last),
        ] {
            if value != i64::MIN {
                cursor[key] = json!(value);
            }
        }
        cursor
    }

    /// Where the next read begins once the database changes: the last TAIL
    /// rows of each table and what follows them, never the whole file again.
    pub(super) fn watermark(&self) -> Value {
        let tail = |last: i64| {
            if last == i64::MIN {
                i64::MIN
            } else {
                last.saturating_sub(TAIL)
            }
        };
        Self {
            steps_from: tail(self.steps_last),
            turns_from: tail(self.turns_last),
            ..*self
        }
        .cursor(0, tail(self.turns_last))
    }
}
