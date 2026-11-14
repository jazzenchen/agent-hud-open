//! The languages the app is written in: their codes, how a tag or a setting
//! names one, their catalogs and their counted forms.
use crate::system_language;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// A language the app is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    ZhHans,
    ZhHant,
    Ja,
    Ko,
    Fr,
    De,
    Es,
    PtBr,
}

impl Language {
    pub const ALL: [Language; 9] = [
        Language::En,
        Language::ZhHans,
        Language::ZhHant,
        Language::Ja,
        Language::Ko,
        Language::Fr,
        Language::De,
        Language::Es,
        Language::PtBr,
    ];

    /// The code settings, snapshots and catalogs use.
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhHans => "zh-Hans",
            Language::ZhHant => "zh-Hant",
            Language::Ja => "ja",
            Language::Ko => "ko",
            Language::Fr => "fr",
            Language::De => "de",
            Language::Es => "es",
            Language::PtBr => "pt-BR",
        }
    }

    /// The language a BCP 47 tag or setting names ("zh-CN", "zh-Hant-TW",
    /// "pt_PT", "ja-JP" …), if the app is written in it.
    pub fn from_tag(tag: &str) -> Option<Language> {
        let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
        let base = tag.split(['-', '.', '@']).next().unwrap_or("");
        Some(match base {
            "en" => Language::En,
            "zh" => {
                let traditional =
                    tag.contains("hant") || ["-tw", "-hk", "-mo"].iter().any(|r| tag.contains(r));
                if traditional && !tag.contains("hans") {
                    Language::ZhHant
                } else {
                    Language::ZhHans
                }
            }
            "ja" => Language::Ja,
            "ko" => Language::Ko,
            "fr" => Language::Fr,
            "de" => Language::De,
            "es" => Language::Es,
            "pt" => Language::PtBr,
            _ => return None,
        })
    }

    /// The language a setting chooses: "system" (or nothing) is the
    /// system's; an unknown language is English.
    pub fn of_setting(setting: &Value) -> Language {
        match setting.as_str().unwrap_or("system") {
            "system" | "" => system_language(),
            tag => Language::from_tag(tag).unwrap_or(Language::En),
        }
    }

    pub fn is_chinese(self) -> bool {
        matches!(self, Language::ZhHans | Language::ZhHant)
    }

    /// Whether words are written without spaces between them.
    pub fn unspaced(self) -> bool {
        matches!(self, Language::ZhHans | Language::ZhHant | Language::Ja)
    }

    pub(crate) fn index(self) -> u8 {
        Language::ALL.iter().position(|l| *l == self).unwrap_or(0) as u8
    }

    pub(crate) fn from_index(index: u8) -> Language {
        Language::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(Language::En)
    }

    fn source(self) -> &'static str {
        match self {
            Language::En => include_str!("../../../packages/protocol/locales/en.json"),
            Language::ZhHans => include_str!("../../../packages/protocol/locales/zh-Hans.json"),
            Language::ZhHant => include_str!("../../../packages/protocol/locales/zh-Hant.json"),
            Language::Ja => include_str!("../../../packages/protocol/locales/ja.json"),
            Language::Ko => include_str!("../../../packages/protocol/locales/ko.json"),
            Language::Fr => include_str!("../../../packages/protocol/locales/fr.json"),
            Language::De => include_str!("../../../packages/protocol/locales/de.json"),
            Language::Es => include_str!("../../../packages/protocol/locales/es.json"),
            Language::PtBr => include_str!("../../../packages/protocol/locales/pt-BR.json"),
        }
    }

    /// The language's catalog, read the first time it is asked for.
    pub(crate) fn catalog(self) -> &'static HashMap<String, String> {
        static CATALOGS: [OnceLock<HashMap<String, String>>; 9] = [const { OnceLock::new() }; 9];
        CATALOGS[self.index() as usize]
            .get_or_init(|| serde_json::from_str(self.source()).expect("checked-in locale catalog"))
    }

    /// The CLDR plural category of `count`, as i18next names it.
    pub(crate) fn plural(self, count: i64) -> &'static str {
        let n = count.unsigned_abs();
        match self {
            Language::ZhHans | Language::ZhHant | Language::Ja | Language::Ko => "other",
            Language::Fr | Language::PtBr if n <= 1 => "one",
            Language::Fr if n != 0 && n.is_multiple_of(1_000_000) => "many",
            Language::Es | Language::PtBr if n != 0 && n.is_multiple_of(1_000_000) => "many",
            _ if n == 1 => "one",
            _ => "other",
        }
    }
}
