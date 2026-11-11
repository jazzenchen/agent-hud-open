//! What Qoder's builds count differently in the Claude Code lines they write
//! (`transcript::claude_lines`); Claude Code's parser reads everything else.
use crate::Usage;
use serde_json::Value;

/// The Qoder builds, each a client of its own.
const BUILDS: [&str; 3] = ["qoder", "qoder-cn", "qoder-work"];

/// Whether `source` is one of the Qoder builds.
pub fn is_build(source: &str) -> bool {
    BUILDS.contains(&source)
}

/// The words a person typed on a Qoder user line, which title its session.
/// Qoder CLI 1.1.85 keeps them in `humanInput.text` and writes the message
/// as text blocks, which Claude Code's rule (text content) passes over; a
/// line whose `origin` is not a person's, such as a message another session
/// sent, is no one's typing. A build that writes neither reads as Claude
/// Code does.
pub fn typed_words(value: &Value) -> Option<&str> {
    if let Some(text) = value["humanInput"]["text"].as_str() {
        return Some(text);
    }
    if value["origin"].is_object() && value["origin"]["kind"] != "human" {
        return None;
    }
    value["message"]["content"].as_str()
}

/// A response's counters as Claude Code's parser keeps them.
///
/// Qoder CLI 1.1.85 writes a reply's tokens only when the reply came from a
/// model of the user's own (a custom, BYOK model) or `QODER_EXPOSE_TOKEN_USAGE`
/// is 1, true or yes. Otherwise it writes each count as 0 beside what the
/// reply cost in Qoder's credits (`usage.credits`, `original_credits`,
/// `billable`), and leaves `usage` out of a reply that names no credits
/// either. Those zeros are no measurement: they change no count, so Claude
/// Code's parser meters no call for them.
///
/// The counts it does write are OpenAI's: `prompt_tokens` as `input_tokens`
/// and its `cached_tokens` as `cache_read_input_tokens`, so its input already
/// holds the cache reads, where Claude Code's does not (Token Monitor's Qoder
/// notes say the same), and the cached part is taken out of it. A line whose
/// cache reads exceed its input cannot hold them inside it and is kept as
/// written; other clients' counters pass unchanged.
pub fn counters(source: &str, raw: Usage) -> Usage {
    if !BUILDS.contains(&source) || raw.read > raw.input {
        return raw;
    }
    Usage {
        input: raw.input - raw.read,
        ..raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qoder_input_holds_its_cache_reads() {
        let raw = Usage {
            input: 1_000,
            output: 30,
            read: 800,
            write: 7,
            reasoning: 0,
        };
        for build in BUILDS {
            assert_eq!(counters(build, raw), Usage { input: 200, ..raw });
        }
        // Claude Code's own input leaves its cache reads out.
        assert_eq!(counters("claude", raw), raw);
        // More cache reads than input: not inside it.
        let apart = Usage {
            input: 3,
            read: 50_000,
            ..raw
        };
        assert_eq!(counters("qoder", apart), apart);
    }
}
