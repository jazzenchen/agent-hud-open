use super::{installed, installed_at, millis, ClaudeChannel, Refusal};
use agent_hud_core::{
    l10n,
    replies::{AnswerFix, SessionReplies, Unreachable},
};
use std::{path::PathBuf, time::SystemTime};

/// The HUD's answers to the Claude desktop app's questions, through the plugin.
pub struct ClaudeReplies {
    channel: ClaudeChannel,
    /// Claude Code's own folder, where it lists the plugins it installed.
    claude_config: PathBuf,
    /// The data folder whose hook inbox records that the HUD sent an answer
    /// (`hooks::record_delivery`); none records nothing.
    deliveries: Option<PathBuf>,
}

impl ClaudeReplies {
    pub fn new(channel: ClaudeChannel, claude_config: PathBuf) -> Self {
        Self {
            channel,
            claude_config,
            deliveries: None,
        }
    }

    /// Records each answer left for a session as typed in Agent HUD on this
    /// Mac (`desktop:agenthud`), so a session's latest tag names the HUD and
    /// not a phone that sent it something earlier (`naming::surfaces`).
    pub fn recording_deliveries(mut self, data: Option<PathBuf>) -> Self {
        self.deliveries = data;
        self
    }

    /// Why words would not reach `session`, which has not checked in lately,
    /// and what would change it: the plugin is not installed; the session
    /// ran it since it was installed, so the app has paused the session; or
    /// the session started before the install and has not loaded it.
    fn unreachable(&self, session: &str) -> Unreachable {
        if !installed(&self.claude_config) {
            return Unreachable::new(
                l10n::text(
                    "Claude Code 还没装 Agent HUD 插件，装好后就能在 Agent HUD 里回答 Claude 的提问（设置 → 智能体插件）。",
                    "Claude Code doesn't have Agent HUD's plugin yet. Install it to answer Claude's questions in Agent HUD (Settings → Agent plugins).",
                ),
                Some(AnswerFix::InstallPlugin),
            );
        }
        let paused = installed_at(&self.claude_config)
            .zip(self.channel.last_check_in(session))
            .is_some_and(|(installed, checked_in)| checked_in >= installed);
        if paused {
            return Unreachable::new(
                l10n::text(
                    "这个会话有一阵没回应 Agent HUD 插件了，可能被 Claude 暂停了。在 Claude 里打开这个会话就能回答。",
                    "This session hasn't answered Agent HUD's plugin for a while; Claude may have paused it. Open it in Claude to answer.",
                ),
                None,
            );
        }
        Unreachable::new(
            l10n::text(
                "这个会话还没加载 Agent HUD 插件。在会话里执行 /reload-plugins 后，就能在 Agent HUD 里回答。",
                "This session hasn't loaded Agent HUD's plugin. Run /reload-plugins in it to answer in Agent HUD.",
            ),
            Some(AnswerFix::ReloadPlugins),
        )
    }
}

/// Why words for another client's session never reach it: the plugin is Claude Code's.
fn not_claude() -> String {
    l10n::text(
        "Agent HUD 的插件只能把回答发进 Claude Code 会话。",
        "Agent HUD's plugin sends answers only into Claude Code sessions.",
    )
}

impl SessionReplies for ClaudeReplies {
    fn reaches(&self, source: &str, session: &str) -> Result<(), Unreachable> {
        if source != agent_hud_agents::claude::AGENT.id {
            return Err(Unreachable::new(not_claude(), None));
        }
        if self.channel.loaded(SystemTime::now()).contains(session) {
            Ok(())
        } else {
            Err(self.unreachable(session))
        }
    }
    fn reply(&self, source: &str, session: &str, words: &str) -> Result<(), String> {
        if source != agent_hud_agents::claude::AGENT.id {
            return Err(not_claude());
        }
        let now = SystemTime::now();
        let handed = self.channel.hand(session, words, now);
        if let (Ok(_), Some(data)) = (&handed, &self.deliveries) {
            use agent_hud_agents::naming::surfaces::{Category, Surface, AGENT_HUD};
            let at = millis(now) as i64;
            // A tag is a label: a failed write only leaves the log's tag.
            let _ = crate::hooks::record_delivery(
                data,
                source,
                session,
                &Surface::new(Category::Desktop, AGENT_HUD),
                at,
            );
        }
        handed.map(drop).map_err(|refusal| match refusal {
            Refusal::NotLoaded => self.unreachable(session).note,
            Refusal::Rejected(_) => l10n::text(
                "插件不能发送以 / 开头的话。",
                "The plugin can't send words that start with /.",
            ),
            Refusal::InvalidInput(_) => l10n::text("没有要发送的话。", "There is nothing to send."),
            _ => l10n::text(
                "回答没能交给 Claude Code 插件。",
                "The answer could not be left for the Claude Code plugin.",
            ),
        })
    }
}
