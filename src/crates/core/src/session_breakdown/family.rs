//! A session's family: its log and the sub-agent logs under it, with the
//! tokens and money they add up to.
use crate::*;

/// The selected logs by the session that started them, so a session's
/// sub-agents can be found at any depth. Decided 2026-10-21, as
/// agent-hud-native-open does: a session counts its sub-agents in its tokens,
/// its breakdown and its money.
pub(crate) struct Family<'a> {
    children: HashMap<(&'a str, &'a str), Vec<&'a LogHead>>,
}

impl<'a> Family<'a> {
    pub(crate) fn new(logs: &[&'a LogHead]) -> Self {
        let mut children = HashMap::<(&str, &str), Vec<&LogHead>>::new();
        for log in logs {
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
        Self { children }
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
        let mut total = head.usage;
        if let Some(part) = subagents {
            total.add(part);
        }
        (total, subagents)
    }

    /// What a session's calls and its sub-agents' would cost at API list
    /// prices, in US dollars; None when one with calls has no list price.
    pub(crate) fn list_cost(&self, head: &LogHead) -> Option<f64> {
        let mut total = None::<i128>;
        for log in std::iter::once(head).chain(self.descendants(&head.source, &head.id)) {
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
        for log in std::iter::once(head).chain(self.descendants(&head.source, &head.id)) {
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
