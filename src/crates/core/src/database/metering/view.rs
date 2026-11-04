//! Reading the meter view's buckets and reply spans, a log at a time.

use super::{
    price_tokens, replies::REPLY_ROWS, reply_span, unsigned, usage, Bucket, Part, Reply, View,
    WEEK_MS,
};
use crate::{pricing, Usage};
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, HashMap, HashSet};

impl Reply {
    /// A call read as at_ms, duration_ms, gap_ms, output + reasoning from
    /// column `at`; None when it does not count.
    fn read(key: u32, r: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<Option<Self>> {
        let call_at: i64 = r.get(at)?;
        let tokens = unsigned(r, at + 3)?;
        Ok(
            reply_span(tokens, r.get(at + 1)?, r.get(at + 2)?).map(|(span, timed)| Self {
                key,
                start: call_at - span as i64,
                at: call_at,
                tokens,
                span,
                timed,
            }),
        )
    }
}

impl View {
    pub fn load(
        db: &Connection,
        paths: &[String],
        now: i64,
        regions: &crate::pricing::PriceRegions,
    ) -> rusqlite::Result<Self> {
        {
            let transaction = db.unchecked_transaction()?;
            transaction.execute("DELETE FROM meter_selected", [])?;
            {
                let mut statement = transaction
                    .prepare_cached("INSERT INTO meter_selected(contribution) VALUES(?1)")?;
                for path in paths {
                    statement.execute([path])?;
                }
            }
            transaction.commit()?;
        }
        let mut selection = paths.to_vec();
        selection.sort_unstable();
        let mut view = Self {
            loaded_at: now,
            selection,
            ..Self::default()
        };
        {
            let mut s = db.prepare(
                "SELECT contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning
                 FROM usage_meter_bucket JOIN meter_selected USING(contribution)",
            )?;
            let mut rows = s.query([])?;
            while let Some(r) = rows.next()? {
                let key = view.key(r.get(1)?, r.get(2)?);
                let row = (key, r.get(3)?, usage(r, 4)?);
                view.parts.entry(r.get(0)?).or_default().buckets.push(row);
            }
        }
        {
            let mut s = db.prepare(&format!(
                "SELECT contribution,source,model,at_ms,duration_ms,gap_ms,output+reasoning
                 FROM meter_selected CROSS JOIN usage_meter_event USING(contribution)
                 WHERE at_ms>=?1 AND {REPLY_ROWS}"
            ))?;
            let mut rows = s.query([now - WEEK_MS])?;
            while let Some(r) = rows.next()? {
                let key = view.key(r.get(1)?, r.get(2)?);
                if let Some(reply) = Reply::read(key, r, 3)? {
                    view.parts.entry(r.get(0)?).or_default().replies.push(reply);
                }
            }
        }
        {
            let mut s = db.prepare(
                "SELECT contribution,at_ms FROM usage_meter_event JOIN meter_selected USING(contribution) WHERE at_ms>?1",
            )?;
            let mut rows = s.query([now])?;
            while let Some(r) = rows.next()? {
                view.parts
                    .entry(r.get(0)?)
                    .or_default()
                    .later
                    .push(r.get(1)?);
            }
        }
        view.aggregate(regions);
        view.refresh_clock(now);
        Ok(view)
    }

    /// Reads the changed logs again; another selection reads every log.
    pub fn update(
        &mut self,
        db: &Connection,
        paths: &[String],
        changed: &HashSet<String>,
        now: i64,
        regions: &crate::pricing::PriceRegions,
    ) -> rusqlite::Result<()> {
        // The selection comes ordered by activity; the view holds it sorted.
        let mut sorted = paths.iter().collect::<Vec<_>>();
        sorted.sort_unstable();
        if !sorted.iter().copied().eq(self.selection.iter()) {
            *self = Self::load(db, paths, now, regions)?;
            return Ok(());
        }
        for path in changed {
            if self.selection.binary_search(path).is_err() {
                continue;
            }
            let mut part = Part::default();
            {
                let mut s = db.prepare_cached(
                    "SELECT source,model,at_ms,input,output,cache_read,cache_write,reasoning
                     FROM usage_meter_bucket WHERE contribution=?1",
                )?;
                let mut rows = s.query([path])?;
                while let Some(r) = rows.next()? {
                    let key = self.key(r.get(0)?, r.get(1)?);
                    part.buckets.push((key, r.get(2)?, usage(r, 3)?));
                }
            }
            {
                let mut s = db.prepare_cached(&format!(
                    "SELECT source,model,at_ms,duration_ms,gap_ms,output+reasoning FROM usage_meter_event
                     WHERE contribution=?1 AND at_ms>=?2 AND {REPLY_ROWS}"
                ))?;
                let mut rows = s.query(params![path, now - WEEK_MS])?;
                while let Some(r) = rows.next()? {
                    let key = self.key(r.get(0)?, r.get(1)?);
                    if let Some(reply) = Reply::read(key, r, 2)? {
                        part.replies.push(reply);
                    }
                }
            }
            {
                let mut s = db.prepare_cached(
                    "SELECT at_ms FROM usage_meter_event WHERE contribution=?1 AND at_ms>?2",
                )?;
                let mut rows = s.query(params![path, now])?;
                while let Some(r) = rows.next()? {
                    part.later.push(r.get(0)?);
                }
            }
            self.parts.insert(path.clone(), part);
        }
        self.aggregate(regions);
        self.refresh_clock(now);
        Ok(())
    }

    fn key(&mut self, source: String, model: String) -> u32 {
        let pair = (source, model);
        if let Some(index) = self.key_index.get(&pair) {
            return *index;
        }
        let index = self.keys.len() as u32;
        self.keys.push(pair.clone());
        self.key_index.insert(pair, index);
        index
    }

    /// The selected logs' buckets summed by source, model and start, as
    /// `SUM … GROUP BY source,model,at_ms HAVING total>0 ORDER BY at_ms`, priced.
    fn aggregate(&mut self, regions: &crate::pricing::PriceRegions) {
        let mut sums = BTreeMap::<(i64, u32), Usage>::new();
        for part in self.parts.values() {
            for (key, at, u) in &part.buckets {
                sums.entry((*at, *key)).or_default().add(*u);
            }
        }
        self.buckets = sums
            .into_iter()
            .filter(|(_, u)| u.total() > 0)
            .map(|((at, key), u)| {
                let (source, model) = &self.keys[key as usize];
                let tokens = price_tokens(u);
                let cost = pricing::cost_at(
                    source,
                    model,
                    &tokens,
                    false,
                    regions.region(source, model),
                    Some(at),
                );
                Bucket {
                    source: source.clone(),
                    model: model.clone(),
                    at,
                    usage: u,
                    cost,
                }
            })
            .collect();
    }

    /// Reply spans of the last hour and week at `now`, and the next moment
    /// either sum changes: when a span leaves the hour or the week, or a call
    /// stamped ahead of the clock comes due.
    pub fn refresh_clock(&mut self, now: i64) {
        let hour = now - 3_600_000;
        let week = now - WEEK_MS;
        let mut sums = HashMap::<(u32, bool, bool), (u64, u64, u64)>::new();
        let mut valid_until: Option<i64> = None;
        let mut bound =
            |at: i64| valid_until = Some(valid_until.map_or(at, |old: i64| old.min(at)));
        for part in self.parts.values_mut() {
            // A span that has left the week never counts again.
            part.replies
                .retain(|reply| reply.start >= week && reply.at >= week);
            for reply in &part.replies {
                if reply.at > now {
                    continue;
                }
                let recent = reply.start >= hour;
                let sum = sums.entry((reply.key, recent, reply.timed)).or_default();
                sum.0 += reply.tokens;
                sum.1 += reply.span;
                sum.2 += 1;
                if recent {
                    bound(reply.start + 3_600_001);
                }
                bound(reply.start + 604_800_001);
            }
            if let Some(next) = part.later.iter().copied().filter(|at| *at > now).min() {
                bound(next);
            }
        }
        self.replies = sums
            .into_iter()
            .map(|((key, recent, timed), sum)| {
                let (source, model) = &self.keys[key as usize];
                ((source.clone(), model.clone(), recent, timed), sum)
            })
            .collect();
        self.valid_until = valid_until;
        self.loaded_at = now;
    }
}
