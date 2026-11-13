//! Folding a session's calls into its prompt turns, or its 15-minute periods.
use super::{
    prompt, recached, Builder, Call, ListTally, SessionUsage, Turn, TurnSums, PERIOD_MS, TURN_LIMIT,
};
use crate::Usage;
use std::collections::BTreeMap;

impl Builder {
    /// `prompts` and `compactions`: the session's own marks.
    pub(crate) fn new(mut prompts: Vec<i64>, compactions: Vec<i64>) -> Self {
        prompts.sort_unstable();
        Self {
            prompts,
            compactions,
            models: BTreeMap::new(),
            periods: BTreeMap::new(),
            subagents: None,
            calls: 0,
            context: None,
            window: None,
            latest: None,
            tally: ListTally::default(),
            previous_prompt: None,
            turns: BTreeMap::new(),
            prompt_index: -1,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.calls == 0
    }

    /// Calls must arrive oldest first.
    pub(crate) fn add(&mut self, call: Call<'_>) {
        let row = call.row;
        let tokens = row.usage;
        let model = (row.source.clone(), row.model.clone());
        self.calls += 1;
        self.models.entry(model.clone()).or_default().add(tokens);
        let period = self
            .periods
            .entry(row.at.div_euclid(PERIOD_MS) * PERIOD_MS)
            .or_default()
            .entry(model.clone())
            .or_default();
        period.tokens.add(tokens);
        period.calls += 1;
        if !call.own {
            self.subagents
                .get_or_insert_with(Usage::default)
                .add(tokens);
        }
        let sent = prompt(tokens);
        let mut again = 0;
        if call.own {
            again = recached(tokens, self.previous_prompt);
            self.previous_prompt = Some(tokens.into());
            self.context = Some(sent);
            if let Some(reported) = row.context_window {
                self.window = Some(reported);
            }
            self.latest = Some(model);
        }
        self.tally.add(&row.source, &row.model, tokens);
        if self.prompts.is_empty() {
            return;
        }
        while self.prompt_index + 1 < self.prompts.len() as i64
            && self.prompts[(self.prompt_index + 1) as usize] <= row.at
        {
            self.prompt_index += 1;
        }
        let start = if self.prompt_index >= 0 {
            self.prompts[self.prompt_index as usize]
        } else {
            row.at
        };
        let turn = self
            .turns
            .entry(self.prompt_index)
            .or_insert_with(|| TurnSums {
                start,
                end: row.at,
                tokens: Usage::default(),
                calls: 0,
                subagents: None,
                context: None,
                peak: 0,
                recached: 0,
                tally: ListTally::default(),
            });
        turn.end = turn.end.max(row.at);
        turn.tokens.add(tokens);
        turn.calls += 1;
        if !call.own {
            turn.subagents
                .get_or_insert_with(Usage::default)
                .add(tokens);
        }
        if call.own {
            turn.context = Some(sent);
            turn.peak = turn.peak.max(sent);
        }
        turn.recached += again;
        turn.tally.add(&row.source, &row.model, tokens);
    }

    /// `window` resolves the latest own model's context window from what the
    /// client reported (`pricing::context_window`).
    pub(crate) fn build(
        self,
        window: impl FnOnce(&str, &str, Option<u64>) -> Option<u64>,
    ) -> SessionUsage {
        let compacted = self
            .compactions
            .iter()
            .map(|at| {
                self.prompts
                    .iter()
                    .rposition(|prompt| prompt <= at)
                    .map_or(-1, |i| i as i64)
            })
            .collect::<std::collections::HashSet<_>>();
        let turn_count = self.turns.len();
        // Only the turns a breakdown keeps are priced.
        let turns = self
            .turns
            .iter()
            .skip(turn_count.saturating_sub(TURN_LIMIT))
            .map(|(index, turn)| Turn {
                start: turn.start,
                end: turn.end,
                tokens: turn.tokens,
                calls: turn.calls,
                context: turn.context,
                compacted: compacted.contains(index),
                subagents: turn.subagents,
                peak_context: turn
                    .context
                    .and_then(|context| (turn.peak > context).then_some(turn.peak)),
                recached: (turn.recached > 0).then_some(turn.recached),
                list_cost: turn.tally.cost(),
            })
            .collect();
        let mut models = self.models.into_iter().collect::<Vec<_>>();
        models.sort_by(|(a_id, a), (b_id, b)| {
            (b.input + b.write + b.output + b.reasoning)
                .cmp(&(a.input + a.write + a.output + a.reasoning))
                .then_with(|| a_id.cmp(b_id))
        });
        let model_periods = self
            .periods
            .iter()
            .flat_map(|(start, models)| {
                models
                    .iter()
                    .map(move |(model, sums)| (*start, model.clone(), sums.tokens))
            })
            .collect();
        let periods = self
            .periods
            .into_iter()
            .map(|(start, models)| {
                let mut tokens = Usage::default();
                let mut calls = 0;
                for sums in models.values() {
                    tokens.add(sums.tokens);
                    calls += sums.calls;
                }
                (start, tokens, calls)
            })
            .collect();
        let context_window = self
            .latest
            .as_ref()
            .and_then(|(source, model)| window(source, model, self.window));
        SessionUsage {
            models,
            periods,
            model_periods,
            subagents: self.subagents,
            calls: self.calls,
            context: self.context,
            turns,
            turn_count,
            context_window,
            list_cost: self.tally.cost(),
        }
    }
}
