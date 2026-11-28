//! Case-folded session search without per-field heap allocations.

pub(super) struct FoldedQuery {
    raw: String,
    chars: Vec<char>,
}

impl FoldedQuery {
    pub(super) fn new(raw: &str) -> Self {
        let lowered = raw.trim().to_lowercase();
        let chars = lowered.chars().collect();
        Self {
            raw: lowered,
            chars,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    pub(super) fn matches(&self, haystack: &str) -> bool {
        if self.raw.is_empty() {
            return true;
        }
        if haystack.is_ascii() && self.raw.is_ascii() {
            let needle = self.raw.as_bytes();
            return needle.len() <= haystack.len()
                && haystack
                    .as_bytes()
                    .windows(needle.len())
                    .any(|window| window.eq_ignore_ascii_case(needle));
        }
        let m = self.chars.len();
        if m == 0 {
            return true;
        }
        if m <= 64 {
            let mut ring = ['\0'; 64];
            let mut seen = 0usize;
            for ch in haystack.chars().flat_map(char::to_lowercase) {
                ring[seen % m] = ch;
                seen += 1;
                if seen >= m {
                    let start = seen % m;
                    if (0..m).all(|i| ring[(start + i) % m] == self.chars[i]) {
                        return true;
                    }
                }
            }
            false
        } else {
            haystack.to_lowercase().contains(&self.raw)
        }
    }

    pub(super) fn matches_any<'a>(
        &self,
        fields: impl IntoIterator<Item = Option<&'a str>>,
    ) -> bool {
        self.is_empty()
            || fields
                .into_iter()
                .flatten()
                .any(|field| self.matches(field))
    }
}

#[cfg(test)]
mod tests {
    use super::FoldedQuery;

    #[test]
    fn ascii_and_unicode_queries_match_case_insensitively_without_false_positives() {
        let q = FoldedQuery::new("  OpUs 4.5 ");
        assert!(!q.is_empty());
        assert!(q.matches("claude-OPUS 4.5-preview"));
        assert!(!q.matches("claude-sonnet-4.5"));

        let cjk = FoldedQuery::new("调研报告");
        assert!(cjk.matches("项目：AI 调研报告 v2"));
        assert!(!cjk.matches("项目：周报"));

        // Multi-codepoint Unicode folding: 'İ' lowercases to 'i' + U+0307.
        let unicode = FoldedQuery::new("i\u{307}stanbul");
        assert!(unicode.matches("Welcome to İSTANBUL!"));

        let ascii_in_cjk = FoldedQuery::new("codex");
        assert!(ascii_in_cjk.matches("会话 · CoDeX Desktop · 终端"));
        assert!(FoldedQuery::new("").matches_any([None, Some("anything")]));
    }
}
