//! A session's family: its log, or the segments of one Codex split across
//! several rollouts, and the sub-agent logs under it, with the tokens and
//! money they add up to.
use crate::*;

/// The selected logs by the session that started them, so a session's
/// sub-agents can be found at any depth. Decided 2026-10-21, as
/// agent-hud-native-open does: a session counts its sub-agents in its tokens,
/// its breakdown and its money.
pub(crate) struct Family<'a> {
    children: HashMap<(&'a str, &'a str), Vec<&'a LogHead>>,
    /// The segments of each session Codex split across several rollouts
    /// (`usage::segment`), earliest first.
    segments: HashMap<(&'a str, &'a str), Vec<&'a LogHead>>,
}

impl<'a> Family<'a> {
    pub(crate) fn new(logs: &[&'a LogHead]) -> Self {
        let mut children = HashMap::<(&str, &str), Vec<&LogHead>>::new();
        let mut segments = HashMap::<(&str, &str), Vec<&LogHead>>::new();
        for log in logs {
            if !crate::usage::segment(log).is_empty() {
                segments
                    .entry((log.source.as_str(), log.id.as_str()))
                    .or_default()
                    .push(log);
            }
            if let Some(parent) = log
                .live
                .parent_id
                .as_deref()
                .filter(|parent| *parent != log.id)
            {
                children
                    .entry((log.source.as_str(), parent))
                    .or_default()
                    .push(log);
            }
        }
        segments.retain(|_, logs| logs.len() > 1);
        for logs in segments.values_mut() {
            logs.sort_unstable_by_key(|log| crate::usage::segment(log));
        }
        Self { children, segments }
    }

    /// The logs of a session's own calls: its log, or each segment of a
    /// session Codex split across several rollouts, earliest first.
    pub(crate) fn own<'b>(&'b self, head: &'b LogHead) -> Vec<&'b LogHead> {
        match self.segments.get(&(head.source.as_str(), head.id.as_str())) {
            Some(segments) => segments.to_vec(),
            None => vec![head],
        }
    }

    /// A session's own tokens, of every segment it has, without its
    /// sub-agents'.
    pub(crate) fn own_usage(&self, head: &LogHead) -> Usage {
        let mut total = Usage::default();
        for log in self.own(head) {
            total.add(log.usage);
        }
        total
    }

    /// The sub-agent logs under a session, at any depth.
    pub(crate) fn descendants(&self, source: &str, id: &str) -> Vec<&'a LogHead> {
        let mut found = Vec::<&LogHead>::new();
        let mut seen = HashSet::<&Path>::new();
        // The names are borrowed, never copied: the Sessions page's day
        // totals walk every matching session.
        let mut queue = vec![(source, id)];
        while let Some((source, id)) = queue.pop() {
            for child in self.children.get(&(source, id)).into_iter().flatten() {
                if seen.insert(child.path.as_path()) {
                    found.push(child);
                    queue.push((child.source.as_str(), child.id.as_str()));
                }
            }
        }
        found
    }

    /// A session's tokens with its sub-agents' at any depth, and the
    /// sub-agents' part of them; None when no sub-agent spent any. Each log
    /// still counts once in totals that add every log; only the session's
    /// row and page add its sub-agents to it.
    pub(crate) fn usage(&self, head: &LogHead) -> (Usage, Option<Usage>) {
        let mut subagents = None::<Usage>;
        for log in self.descendants(&head.source, &head.id) {
            if log.usage.total() > 0 {
                subagents.get_or_insert_with(Usage::default).add(log.usage);
            }
        }
        let mut total = self.own_usage(head);
        if let Some(part) = subagents {
            total.add(part);
        }
        (total, subagents)
    }

    /// The part of a session's tokens its sub-agents spent, apart from that
    /// of the internal logs under it (Codex's auto-reviews of approval
    /// requests), which no list shows and so cannot be counted as sub-agents:
    /// each None when it spent nothing, the reviews with how many there were.
    pub(crate) fn parts(&self, head: &LogHead) -> (Option<Usage>, Option<(usize, Usage)>) {
        let mut subagents = None::<Usage>;
        let mut reviews = None::<(HashSet<&str>, Usage)>;
        for log in self.descendants(&head.source, &head.id) {
            if log.usage.total() == 0 {
                continue;
            }
            if log.live.is_internal {
                let (ids, usage) = reviews.get_or_insert_with(Default::default);
                ids.insert(log.id.as_str());
                usage.add(log.usage);
            } else {
                subagents.get_or_insert_with(Usage::default).add(log.usage);
            }
        }
        (subagents, reviews.map(|(ids, usage)| (ids.len(), usage)))
    }

    /// What a session's calls and its sub-agents' would cost at API list
    /// prices, in US dollars; None when one with calls has no list price.
    pub(crate) fn list_cost(&self, head: &LogHead) -> Option<f64> {
        let mut total = None::<i128>;
        for log in self
            .own(head)
            .into_iter()
            .chain(self.descendants(&head.source, &head.id))
        {
            match log.list_cost {
                Some(cost) => total = Some(total.unwrap_or(0) + cost),
                None if log.usage.total() == 0 => {}
                None => return None,
            }
        }
        total.map(|pico| pico as f64 / 1e12)
    }

    /// What the calls with a list price, of a session and its sub-agents,
    /// cost at API list prices in US dollars (None when no call has one),
    /// and the tokens of the calls with none.
    pub(crate) fn priced_cost(&self, head: &LogHead) -> (Option<f64>, u64) {
        let mut priced = None::<i128>;
        let mut unpriced = 0;
        for log in self
            .own(head)
            .into_iter()
            .chain(self.descendants(&head.source, &head.id))
        {
            // A log priced before the priced part was kept has only its whole
            // cost until it is priced again.
            let part = match log.unpriced_tokens {
                Some(_) => log.priced_cost,
                None => log.list_cost,
            };
            if let Some(cost) = part {
                priced = Some(priced.unwrap_or(0) + cost);
            }
            unpriced += log.unpriced_tokens.unwrap_or(0);
        }
        (priced.map(|pico| pico as f64 / 1e12), unpriced)
    }
}
