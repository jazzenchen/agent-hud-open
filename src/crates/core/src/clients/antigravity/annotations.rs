//! A conversation's annotation, the text-format file Antigravity keeps beside
//! it under `annotations/`: its title and whether it is archived.
use std::{fs, path::Path};

pub(super) fn annotation(path: &Path) -> (Option<String>, bool) {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return (None, false);
    };
    let Some(file) = path
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("annotations").join(format!("{stem}.pbtxt")))
    else {
        return (None, false);
    };
    let Ok(meta) = fs::metadata(&file) else {
        return (None, false);
    };
    if meta.len() > 65536 {
        return (None, false);
    }
    let Ok(text) = fs::read_to_string(file) else {
        return (None, false);
    };
    let archived = has_archived_flag(&text);
    (parse_title(&text), archived)
}

fn has_archived_flag(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(p) = text[at..].find("archived") {
        let start = at + p;
        at = start + 8;
        if start > 0 && !bytes[start - 1].is_ascii_whitespace() && bytes[start - 1] != b'{' {
            continue;
        }
        let Some(rest) = text[at..]
            .trim_start()
            .strip_prefix(':')
            .map(str::trim_start)
        else {
            continue;
        };
        if rest
            .strip_prefix("true")
            .is_some_and(|after| after.is_empty() || !after.as_bytes()[0].is_ascii_alphanumeric())
        {
            return true;
        }
    }
    false
}

fn parse_title(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(p) = text[at..].find("title") {
        let start = at + p;
        at = start + 5;
        if start > 0 && !bytes[start - 1].is_ascii_whitespace() && bytes[start - 1] != b'{' {
            continue;
        }
        let Some(rest) = text[at..]
            .trim_start()
            .strip_prefix(':')
            .map(str::trim_start)
        else {
            continue;
        };
        let quote = rest.as_bytes().first().copied()?;
        if quote != b'\'' && quote != b'"' {
            continue;
        }
        // Text-format escapes are bytes: "\344\275\240" is the UTF-8 of 你.
        let mut output = Vec::<u8>::new();
        let mut chars = rest[1..].chars();
        while let Some(c) = chars.next() {
            if c == quote as char {
                return String::from_utf8(output).ok();
            }
            if c != '\\' {
                output.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                continue;
            }
            let c = chars.next()?;
            match c {
                'n' => output.push(b'\n'),
                'r' => output.push(b'\r'),
                't' => output.push(b'\t'),
                'x' => {
                    let h: String = chars.by_ref().take(2).collect();
                    output.push(u8::from_str_radix(&h, 16).ok()?);
                }
                'u' | 'U' => {
                    let h: String = chars.by_ref().take(if c == 'u' { 4 } else { 8 }).collect();
                    let scalar = char::from_u32(u32::from_str_radix(&h, 16).ok()?)?;
                    output.extend_from_slice(scalar.encode_utf8(&mut [0; 4]).as_bytes());
                }
                '0'..='7' => {
                    let mut h = String::from(c);
                    for _ in 0..2 {
                        if chars.clone().next().is_some_and(|c| matches!(c, '0'..='7')) {
                            h.push(chars.next()?)
                        } else {
                            break;
                        }
                    }
                    output.push(u8::from_str_radix(&h, 8).ok()?);
                }
                _ => output.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
            }
        }
    }
    None
}
