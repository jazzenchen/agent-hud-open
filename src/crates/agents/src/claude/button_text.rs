//! Button text as the Claude desktop app reads it, for the words its next-step box sends.
use serde_json::Value;

fn is_default_ignorable(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

/// Text as the app reads a button's: without control, format or line and
/// paragraph separator characters other than the joiners, nor the filler
/// characters it drops, its spaces collapsed (agent-hud-native-open
/// ClaudeDesktopAsks.shown; this app does not compose the text first).
pub(super) fn shown(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    let kept = text
        .chars()
        .filter(|&c| {
            if c == '\u{200C}' || c == '\u{200D}' {
                return true;
            }
            !(c.is_control()
                || is_format(c)
                || c == '\u{2028}'
                || c == '\u{2029}'
                || matches!(
                    c,
                    '\u{115F}'
                        | '\u{1160}'
                        | '\u{3164}'
                        | '\u{FFA0}'
                        | '\u{2800}'
                        | '\u{FFFC}'
                        | '\u{1D159}'
                        | '\u{13441}'
                        | '\u{13442}'
                        | '\u{16910}'
                ))
        })
        .collect::<String>();
    Some(kept.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Unicode's format characters (general category Cf).
fn is_format(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Words the app will not send as a reply: a mention, a command, or an
/// invisible character.
pub(super) fn sends_otherwise(words: &str) -> bool {
    words.contains('@') || words.starts_with(['/', '!']) || words.chars().any(is_default_ignorable)
}
