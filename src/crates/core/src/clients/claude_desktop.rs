//! Claude's desktop app on AgentHudCore: the questions it asks in its own
//! window, as cards, and how their answers go back.
use crate::l10n::tr;
use crate::*;
use agent_hud_agents::claude::{transcript::*, AGENT as CLAUDE, REPLIES};
use agent_hud_agents::replies::{AnswerFix, Unreachable};
use agent_hud_agents::transcript::live::DesktopAsk;

impl crate::AgentHudCore {
    /// Gives each desktop question that would show a card, and that a
    /// checkpoint gave back without its words, its words again from its
    /// call's line, once: one whose line cannot be read is not tried again
    /// until the log changes, and has no card while its session still waits.
    pub(crate) fn reword_desktop_asks(&mut self, now: i64) {
        let unworded = self
            .logs
            .values()
            .filter(|log| log.source == CLAUDE.id && !log.gone)
            .filter_map(|log| {
                let ask = log.live.activity.as_ref()?.desktop_ask.as_ref()?;
                (!ask.is_worded() && ask.offset.is_some() && now - ask.at < 1_800_000).then(|| {
                    (
                        log.path.clone(),
                        log.origin_path.clone().unwrap_or_else(|| log.path.clone()),
                        ask.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        fn asked<'a>(head: &'a mut crate::LogHead, id: &str) -> Option<&'a mut DesktopAsk> {
            head.live
                .activity
                .as_mut()?
                .desktop_ask
                .as_mut()
                .filter(|kept| kept.id == id)
        }
        for (key, file, ask) in unworded {
            let worded = reworded_desktop_ask(&file, &ask);
            let Some(head) = self.logs.get_mut(&key) else {
                continue;
            };
            match worded {
                Some(worded) => {
                    let mut read = head.clone();
                    if let Some(kept) = asked(&mut read, &ask.id) {
                        *kept = worded;
                    }
                    head.keep_words(&read);
                }
                None => {
                    if let Some(kept) = asked(head, &ask.id) {
                        kept.offset = None;
                    }
                }
            }
        }
    }
}

impl crate::AgentHudCore {
    /// Why Claude's questions are not answered here: the user turned off
    /// "Answer questions" on the Claude Code plugin's row in Settings.
    pub(crate) fn claude_answers_off(&self) -> Option<Unreachable> {
        (self.settings[REPLIES.setting] == false).then(|| {
            Unreachable::new(
                tr!("Answering Claude's questions in Agent HUD is off (Settings → Agent plugins)."),
                Some(AnswerFix::TurnOnAnswers),
            )
        })
    }

    /// Whether a card for `ask` in the session `session` can take its answer
    /// here; Err gives why it can only be answered in the app. An offer's
    /// buttons are the app's own actions, which no words stand for; a box
    /// needs answering on and a host that reaches its session.
    pub(crate) fn desktop_ask_way(
        &self,
        session: &str,
        ask: &DesktopAsk,
    ) -> Result<(), Unreachable> {
        if ask.kind == "offer" {
            return Err(Unreachable::new(
                tr!("Its buttons work only in Claude's own window."),
                None,
            ));
        }
        if let Some(off) = self.claude_answers_off() {
            return Err(off);
        }
        match &self.session_replies {
            Some(replies) => replies.reaches(CLAUDE.id, session),
            None => Err(Unreachable::new(
                tr!("Agent HUD can't send answers into Claude sessions."),
                None,
            )),
        }
    }

    /// Gives the answer to the Claude desktop app's question `id`, if it is
    /// one, before its card goes: the words go into its session through the
    /// host, and a refusal leaves the card waiting with the host's reason.
    /// Answered or put away, its question has no card again while it stands.
    pub(crate) fn answer_desktop_ask(
        &mut self,
        id: &str,
        decision: &Value,
        now: i64,
    ) -> Result<(), String> {
        let Some((key, _)) = self.claude_desktop.asks.get(id) else {
            return Ok(());
        };
        let key = key.clone();
        let view = self.requests.checked(id, decision, now)?;
        if let Some(words) = desktop_ask_words(&view, decision) {
            if let Some(off) = self.claude_answers_off() {
                return Err(off.note);
            }
            let replies = self
                .session_replies
                .as_ref()
                .ok_or_else(|| tr!("Agent HUD can't send answers into Claude sessions."))?;
            replies.reply(
                CLAUDE.id,
                view["sessionId"].as_str().unwrap_or_default(),
                &words,
            )?;
        }
        self.claude_desktop.settled.insert(key);
        Ok(())
    }
}

/// What the Claude desktop app knows that its logs don't say: its sessions
/// and archive, its scheduled jobs, and the questions it asks in its own window.
#[derive(Default)]
pub(crate) struct ClaudeDesktop {
    pub(crate) index: ClaudeDesktopIndex,
    pub(crate) jobs: crate::claude_jobs::ClaudeJobs,
    /// Claude Code's live processes, desktop and terminal alike: which
    /// sessions lost theirs.
    pub(crate) processes: agent_hud_agents::claude::processes::ClaudeProcesses,
    /// The cards of the app's questions, by request id: the question's key
    /// (its session and call) and the ticket that keeps the card answerable.
    pub(crate) asks: HashMap<String, (String, crate::requests::Ticket)>,
    /// The keys of questions answered or put away on the HUD or a phone: no
    /// card comes back for them, answerable or not, while they stand.
    pub(crate) settled: HashSet<String>,
    /// The keys of questions whose session the host has reached since they
    /// were asked: their cards stay answerable while they stand.
    pub(crate) reached: HashSet<String>,
}

impl crate::AgentHudCore {
    /// Puts the questions the Claude desktop app asks in its own window among
    /// the requests the user is asked about, in place of the ones asked
    /// before. A next-step box whose session the host reaches is answerable
    /// (a card of its own id, which replaces the display-only one, as a host
    /// that starts sending replaces Open's); any other has the reason it can
    /// only be answered in the app (agent-hud-native-open
    /// ClaudeDesktopAsks.show/requests).
    pub(crate) fn sync_claude_desktop_asks(&mut self, now: i64) {
        let mut active = HashMap::new();
        let mut standing = HashSet::new();
        let claude = agent_hud_agents::claude::AGENT.id;
        if self.enabled.get(claude).copied().unwrap_or(true) {
            self.reword_desktop_asks(now);
            // Every pass prepares the projection first; its selection saves
            // choosing among each session's copies again.
            let candidates = self
                .listed_sessions()
                .into_iter()
                .filter(|log| log.source == claude)
                .filter_map(|log| {
                    let ask = log.live.activity.as_ref()?.desktop_ask.as_ref()?;
                    // A question whose words this run could not read has no card.
                    if !ask.is_worded() {
                        return None;
                    }
                    if now - ask.at >= 1_800_000
                        || !matches!(
                            log.live.session_state(now, &log.source, None),
                            "running" | "waiting"
                        )
                    {
                        return None;
                    }
                    let key = crate::grok_cache::hash(&[&log.id, &ask.id]);
                    Some((key, log.id.clone(), log.cwd.clone(), ask.clone()))
                })
                .collect::<Vec<_>>();
            for (key, session, cwd, ask) in candidates {
                standing.insert(key.clone());
                if self.claude_desktop.settled.contains(&key) {
                    continue;
                }
                // Once answerable, a box stays so while it stands: a session
                // the host stops reaching for a while (the app pauses a
                // hidden session) refuses the answer with the host's reason
                // instead of its card being replaced back and forth, here
                // and on the phone.
                // Turning answering off takes effect at once, reached or not.
                let way = if let Some(off) = self.claude_answers_off() {
                    Err(off)
                } else if self.claude_desktop.reached.contains(&key) {
                    Ok(())
                } else {
                    self.desktop_ask_way(&session, &ask)
                };
                if way.is_ok() {
                    self.claude_desktop.reached.insert(key.clone());
                }
                let req_id = format!(
                    "claude-desktop-ask:{}{key}",
                    if way.is_ok() { "reply:" } else { "" }
                );
                let view = agent_hud_agents::claude::transcript::desktop_ask_view(
                    &req_id,
                    &session,
                    cwd.as_deref(),
                    &ask,
                    way.err(),
                );
                match self
                    .requests
                    .register_native(req_id.clone(), view.clone(), now)
                {
                    Some(ticket) => {
                        active.insert(req_id, (key, ticket));
                    }
                    None => {
                        // Shown already, or put away by its wait running out.
                        self.requests.refresh_native(&req_id, view);
                        if let Some(kept) = self.claude_desktop.asks.remove(&req_id) {
                            active.insert(req_id, kept);
                        }
                    }
                }
            }
        }
        for old_id in self.claude_desktop.asks.keys() {
            if !active.contains_key(old_id) {
                self.requests.settle_native(old_id);
            }
        }
        self.claude_desktop.asks = active;
        self.claude_desktop
            .settled
            .retain(|key| standing.contains(key));
        self.claude_desktop
            .reached
            .retain(|key| standing.contains(key));
    }
}
