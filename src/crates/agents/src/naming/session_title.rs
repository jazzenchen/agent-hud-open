//! Session titles: a prompt's first line or a client-kept name, at most 60
//! characters. Follows agent-hud-native-open `SessionTitle`.
use super::characters;
use unicode_segmentation::UnicodeSegmentation;

const LIMIT: usize = 60;

/// The title a prompt gives its session: its first line, trimmed, at most 60
/// characters. Slash-command or meta prompts (`<…>`) and interruption notices
/// give none.
///
/// Better than Open: a Windows `\r\n` ends the first line too. Swift treats
/// `\r\n` as one character, so Open kept the whole prompt.
pub fn from_prompt(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(is_whitespace_or_newline);
    if trimmed.is_empty() || trimmed.starts_with('<') || trimmed.starts_with("[Request interrupted")
    {
        return None;
    }
    let first = trimmed.split(is_line_break).next().unwrap_or(trimmed);
    let line = first.trim_matches(is_whitespace);
    (!line.is_empty()).then(|| clipped(line))
}

/// A title the client keeps for the session, whether the user gave it or the
/// client generated it: its first non-empty line, trimmed, at most 60
/// characters.
pub fn named(text: Option<&str>) -> Option<String> {
    text?
        .split(is_line_break)
        .map(|line| line.trim_matches(is_whitespace))
        .find(|line| !line.is_empty())
        .map(clipped)
}

fn clipped(text: &str) -> String {
    if characters(text) <= LIMIT {
        return text.to_owned();
    }
    let mut out: String = text.graphemes(true).take(LIMIT - 1).collect();
    out.push('…');
    out
}

/// Foundation's `CharacterSet.whitespaces`: space separators and tab.
fn is_whitespace(c: char) -> bool {
    c == '\t' || (c.is_whitespace() && !is_line_break(c))
}

/// Foundation's `CharacterSet.whitespacesAndNewlines`.
fn is_whitespace_or_newline(c: char) -> bool {
    c.is_whitespace()
}

/// Swift's `Character.isNewline`.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

/// What a session's own records say about its name. Readers record evidence;
/// `display` turns it into the title people see, with each client's fallbacks.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TitleEvidence {
    /// The best name the client keeps for the session, with its rank: 0 is a
    /// name the user gave, higher ranks are titles the client generated. A
    /// later name of the same or a stronger rank replaces it.
    #[serde(default)]
    named: Option<(u8, String)>,
    /// The title the first real prompt gives the session.
    #[serde(default)]
    prompt: Option<String>,
    /// The name of the agent a client spawned for the session, which titles
    /// it before its folder does (a Codex sub-agent's path or nickname).
    #[serde(default)]
    agent: Option<String>,
    /// A client-specific last resort, such as "OpenClaw · main".
    #[serde(default)]
    fallback: Option<String>,
}

impl TitleEvidence {
    /// A name the client keeps, cleaned by `named`. Nothing changes when the
    /// text has no usable line.
    pub fn name(&mut self, rank: u8, text: Option<&str>) {
        if let Some(title) = named(text) {
            if self.named.as_ref().is_none_or(|(old, _)| rank <= *old) {
                self.named = Some((rank, title));
            }
        }
    }

    /// The client cleared the session's name, so the prompt titles it again.
    pub fn clear_name(&mut self) {
        self.named = None;
    }

    /// The first prompt that gives a title; later prompts are ignored.
    pub fn prompt(&mut self, text: &str) {
        if self.prompt.is_none() {
            self.prompt = from_prompt(text);
        }
    }

    pub fn has_name(&self) -> bool {
        self.named.is_some()
    }

    pub fn agent(&mut self, name: &str) {
        self.agent = named(Some(name));
    }

    pub fn fallback(&mut self, text: impl Into<String>) {
        self.fallback = Some(text.into());
    }
}

/// The title people see: the client's name for the session, else its first
/// prompt, else the name of the agent spawned for it, else (for clients whose
/// sessions belong to a folder) the working folder's name, else the client's
/// own last resort.
pub fn display(
    source: &str,
    id: &str,
    evidence: &TitleEvidence,
    cwd: Option<&str>,
    stored: Option<&str>,
) -> String {
    if let Some((_, name)) = &evidence.named {
        return name.clone();
    }
    if let Some(prompt) = &evidence.prompt {
        return prompt.clone();
    }
    // A checkpoint written before evidence existed kept its title as text.
    if let Some(title) = stored
        .filter(|title| *title != id && !title.starts_with(&format!("{source}:")))
        .and_then(|title| named(Some(title)))
    {
        return title;
    }
    if let Some(agent) = &evidence.agent {
        return agent.clone();
    }
    if folder_named(source) {
        if let Some(folder) = cwd.and_then(folder_name) {
            return folder;
        }
    }
    if let Some(fallback) = &evidence.fallback {
        return fallback.clone();
    }
    let raw = id.strip_prefix(&format!("{source}:")).unwrap_or(id);
    let short: String = raw.graphemes(true).take(8).collect();
    match source {
        "claude" => "Claude Code".into(),
        "codex" => "Codex".into(),
        "deepseek" => "DeepSeek Harness".into(),
        "copilot" => "Copilot CLI".into(),
        "grok-cli" => "Grok CLI".into(),
        "grok-bot" => "Grok Bot".into(),
        "opencode" => "OpenCode".into(),
        "pi" => "Pi".into(),
        "kimi" => "Kimi".into(),
        "openclaw" => "OpenClaw".into(),
        _ => format!("{} · {short}", client_name(source)),
    }
}

/// Clients whose untitled sessions read as their working folder.
fn folder_named(source: &str) -> bool {
    matches!(
        source,
        "claude"
            | "codex"
            | "deepseek"
            | "codebuddy"
            | "workbuddy"
            | "qwen"
            | "zcode"
            | "copilot"
            | "devin"
            | "grok-cli"
            | "opencode"
            | "qoder"
            | "qoder-cn"
            | "qoder-work"
            | "kilo"
            | "mimocode"
            | "teleagent"
            | "droid"
            | "cline"
            | "roo-code"
            | "goose"
            | "trae"
            | "trae-cn"
    )
}

/// The last component of a working folder, on either path convention.
fn folder_name(path: &str) -> Option<String> {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty() && !name.ends_with(':'))
        .map(str::to_owned)
}

fn client_name(source: &str) -> &str {
    match source {
        "codebuddy" => "CodeBuddy",
        "workbuddy" => "WorkBuddy",
        "qwen" => "Qwen",
        "zcode" => "ZCode",
        "devin" => "Devin",
        "hermes" => "Hermes",
        "antigravity" => "Antigravity",
        "cursor" => "Cursor",
        other => crate::by_id(other).map_or(other, |agent| agent.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(named: &[(u8, &str)], prompts: &[&str]) -> TitleEvidence {
        let mut evidence = TitleEvidence::default();
        for (rank, text) in named {
            evidence.name(*rank, Some(text));
        }
        for text in prompts {
            evidence.prompt(text);
        }
        evidence
    }

    #[test]
    fn a_kept_name_beats_the_prompt_and_stronger_ranks_win() {
        let titled = evidence(
            &[(1, "Generated"), (0, "Given"), (1, "Generated again")],
            &["fix it"],
        );
        assert_eq!(display("claude", "claude:x", &titled, None, None), "Given");
        let generated = evidence(&[(1, "First"), (1, "Latest")], &["fix it"]);
        assert_eq!(
            display("claude", "claude:x", &generated, None, None),
            "Latest"
        );
        let prompted = evidence(
            &[],
            &[
                "<command-name>/init</command-name>",
                "  fix the build\nthen test",
                "later",
            ],
        );
        assert_eq!(
            display("codex", "codex:x", &prompted, None, None),
            "fix the build"
        );
    }

    #[test]
    fn a_cleared_name_gives_the_prompt_back() {
        let mut named = evidence(&[(0, "Renamed")], &["first prompt"]);
        named.clear_name();
        assert_eq!(display("pi", "pi:x", &named, None, None), "first prompt");
    }

    #[test]
    fn untitled_sessions_fall_back_like_their_clients() {
        let none = TitleEvidence::default();
        let folder = Some("/Users/me/work/agent-hud");
        assert_eq!(
            display("claude", "claude:1", &none, folder, None),
            "agent-hud"
        );
        assert_eq!(
            display("claude", "claude:1", &none, None, None),
            "Claude Code"
        );
        assert_eq!(
            display("codex", "codex:1", &none, Some(r"C:\Users\me\repo\"), None),
            "repo"
        );
        assert_eq!(
            display("qwen", "qwen:0123456789", &none, None, None),
            "Qwen · 01234567"
        );
        assert_eq!(
            display("workbuddy", "workbuddy:abcdefghij", &none, None, None),
            "WorkBuddy · abcdefgh"
        );
        assert_eq!(
            display("copilot", "copilot:1", &none, None, None),
            "Copilot CLI"
        );
        assert_eq!(
            display("pi", "pi:1", &none, folder, None),
            "Pi",
            "Pi sessions are not named after a folder"
        );
        assert_eq!(
            display("grok-bot", "grok-bot:1", &none, None, None),
            "Grok Bot"
        );
        let mut hermes = TitleEvidence::default();
        hermes.fallback("Hermes · telegram");
        assert_eq!(
            display("hermes", "hermes:abc", &hermes, folder, None),
            "Hermes · telegram"
        );
        assert_eq!(
            display("hermes", "hermes:abcdefghijk", &none, None, None),
            "Hermes · abcdefgh"
        );
    }

    #[test]
    fn a_title_stored_before_evidence_existed_is_kept_but_never_an_id() {
        let none = TitleEvidence::default();
        assert_eq!(
            display("claude", "claude:x", &none, None, Some("fix the build")),
            "fix the build"
        );
        assert_eq!(
            display(
                "claude",
                "claude:x",
                &none,
                Some("/w/repo"),
                Some("claude:x")
            ),
            "repo"
        );
        assert_eq!(
            display("claude", "abc", &none, None, Some("abc")),
            "Claude Code"
        );
    }
}
