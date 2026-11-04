//! Finds where each part of a JSON text starts and ends.
use super::{Entry, Kind, Node, Parser};

impl<'a> Parser<'a> {
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    pub(super) fn space(&mut self) -> &'a str {
        let start = self.at;
        while matches!(self.byte(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
        &self.text[start..self.at]
    }

    fn string(&mut self) -> Option<&'a str> {
        let start = self.at;
        if self.byte()? != b'"' {
            return None;
        }
        self.at += 1;
        loop {
            match self.byte()? {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return self.text.get(start..self.at);
                }
                _ => self.at += 1,
            }
        }
    }

    pub(super) fn value(&mut self) -> Option<Node<'a>> {
        let start = self.at;
        let kind = match self.byte()? {
            b'{' | b'[' => self.container()?,
            b'"' => {
                self.string()?;
                Kind::Scalar
            }
            _ => {
                while !matches!(
                    self.byte(),
                    None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
                ) {
                    self.at += 1;
                }
                Kind::Scalar
            }
        };
        Some(Node {
            raw: self.text.get(start..self.at)?,
            kind,
        })
    }

    fn container(&mut self) -> Option<Kind<'a>> {
        let object = self.byte()? == b'{';
        let close = if object { b'}' } else { b']' };
        let finish = |entries, tail| {
            if object {
                Kind::Object(entries, tail)
            } else {
                Kind::Array(entries, tail)
            }
        };
        self.at += 1;
        let mut entries = Vec::new();
        let mut lead = self.space();
        if self.byte()? == close {
            self.at += 1;
            return Some(finish(entries, lead));
        }
        loop {
            let key = if object {
                let raw = self.string()?;
                let name: String = serde_json::from_str(raw).ok()?;
                let colon = self.at;
                self.space();
                if self.byte()? != b':' {
                    return None;
                }
                self.at += 1;
                self.space();
                Some((raw, name, self.text.get(colon..self.at)?))
            } else {
                None
            };
            let value = self.value()?;
            let space = self.space();
            let comma = match self.byte()? {
                b',' => true,
                byte if byte == close => false,
                _ => return None,
            };
            self.at += 1;
            entries.push(Entry {
                lead,
                key,
                value,
                trail: if comma { space } else { "" },
            });
            if !comma {
                return Some(finish(entries, space));
            }
            lead = self.space();
        }
    }
}
