//! What Qoder's builds count differently in the Claude Code lines they write
//! (`transcript::claude_lines`); Claude Code's parser reads everything else.
use crate::Usage;

/// The Qoder builds, each a client of its own.
const BUILDS: [&str; 3] = ["qoder", "qoder-cn", "qoder-work"];

/// A response's counters as Claude Code's parser keeps them. Qoder CLI
/// (1.1.82) writes an OpenAI-style `prompt_tokens` as `input_tokens` and its
/// `cached_tokens` as `cache_read_input_tokens`, so its input already holds
/// the cache reads, where Claude Code's does not (Token Monitor's Qoder
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
