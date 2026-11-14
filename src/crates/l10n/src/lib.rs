//! The language the app writes its own text in: notices, statuses, reasons,
//! menus. Quota window names stay in English in every language, and what a
//! vendor or a client wrote passes through.
//!
//! Text is written in English in the code and looked up in the catalog of the
//! current language, `src/packages/protocol/locales/<code>.json`, which the
//! web views read too (through i18next). A key missing from a catalog shows
//! in English. Placeholders are written `{{name}}`, as i18next writes them;
//! a counted text has `_one`/`_other` (and, for some languages, `_few`,
//! `_many`) forms under its key, chosen as i18next chooses them.
//!
//! Text is written in the language of the moment it is made: a notice made
//! before the language changed stays as it was until it is made again. The
//! host tells the system's language once at launch; Core follows the
//! Language setting at every snapshot.
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

mod language;
pub use language::Language;

static SYSTEM: AtomicU8 = AtomicU8::new(0);
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The system's preferred languages, first first, which the "system" setting
/// follows: the first the app is written in wins.
pub fn set_system_languages<S: AsRef<str>>(tags: &[S]) {
    let language = tags
        .iter()
        .find_map(|tag| Language::from_tag(tag.as_ref()))
        .unwrap_or(Language::En);
    SYSTEM.store(language.index(), Ordering::Relaxed);
}

/// The system's language, told at launch.
pub fn system_language() -> Language {
    Language::from_index(SYSTEM.load(Ordering::Relaxed))
}

/// Follows the Language setting: "system", or a language's code.
pub fn follow(setting: &Value) {
    CURRENT.store(Language::of_setting(setting).index(), Ordering::Relaxed);
}

/// The language text is written in now.
pub fn current() -> Language {
    Language::from_index(CURRENT.load(Ordering::Relaxed))
}

/// Whether the app is written in Chinese now (either script).
pub fn chinese() -> bool {
    current().is_chinese()
}

/// `key`, an English text, in `language`.
pub fn tr_in(language: Language, key: &str) -> String {
    lookup(language, key).unwrap_or(key).to_owned()
}

/// `key`, an English text, in the current language.
pub fn tr(key: &str) -> String {
    tr_in(current(), key)
}

fn lookup(language: Language, key: &str) -> Option<&'static str> {
    language
        .catalog()
        .get(key)
        .or_else(|| extra(language).and_then(|catalog| catalog.get(key)))
        .map(String::as_str)
        .filter(|text| !text.is_empty())
}

type Sources = Vec<(Language, &'static str)>;
static EXTRA_SOURCES: Mutex<Sources> = Mutex::new(Vec::new());

/// Adds catalogs an app built on this one carries for its own text, as
/// `(language, JSON)`, before any text is looked up. A key in both is the
/// shared catalog's.
pub fn add_catalogs(sources: &[(Language, &'static str)]) {
    if let Ok(mut extra) = EXTRA_SOURCES.lock() {
        extra.extend_from_slice(sources);
    }
}

fn extra(language: Language) -> Option<&'static HashMap<String, String>> {
    static EXTRA: [OnceLock<HashMap<String, String>>; 9] = [const { OnceLock::new() }; 9];
    let catalog = EXTRA[language.index() as usize].get_or_init(|| {
        let sources = EXTRA_SOURCES.lock().map(|s| s.clone()).unwrap_or_default();
        let mut catalog = HashMap::new();
        for (_, source) in sources.iter().filter(|(l, _)| *l == language) {
            let entries: HashMap<String, String> =
                serde_json::from_str(source).expect("an added locale catalog");
            catalog.extend(entries);
        }
        catalog
    });
    (!catalog.is_empty()).then_some(catalog)
}

/// The counted text under `key` for `count` in `language`, with `{{count}}`
/// and the other placeholders filled.
pub fn tr_count_in(
    language: Language,
    key: &str,
    count: i64,
    values: &[(&str, &dyn Display)],
) -> String {
    let form = |language: Language| {
        let category = language.plural(count);
        lookup(language, &format!("{key}_{category}"))
            .or_else(|| lookup(language, &format!("{key}_other")))
    };
    let template = form(language)
        .or_else(|| lookup(language, key))
        .or_else(|| form(Language::En))
        .unwrap_or(key);
    let count = count.to_string();
    let mut all: Vec<(&str, &dyn Display)> = vec![("count", &count)];
    all.extend_from_slice(values);
    fill(template, &all)
}

/// Fills the `{{name}}` placeholders of `template`. A placeholder with an
/// i18next format, `{{count, number}}`, takes the value as it is.
pub fn fill(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut text = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let Some(length) = rest[start..].find("}}") else {
            break;
        };
        let inner = &rest[start + 2..start + length];
        let name = inner.split(',').next().unwrap_or("").trim();
        text.push_str(&rest[..start]);
        match values.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => text.push_str(&value.to_string()),
            None => text.push_str(&rest[start..start + length + 2]),
        }
        rest = &rest[start + length + 2..];
    }
    text.push_str(rest);
    text
}

/// The text of an English key in the current language, with its
/// placeholders filled: `tr!("Waiting {{minutes}} min", minutes = m)`.
/// `tr!(count = n, "{{count}} sessions")` picks the counted form.
#[macro_export]
macro_rules! tr {
    (count = $count:expr, $key:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::tr_count_in(
            $crate::current(),
            $key,
            ($count) as i64,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),*],
        )
    };
    ($key:expr) => { $crate::tr($key) };
    ($key:expr $(, $name:ident = $value:expr)+ $(,)?) => {
        $crate::fill(
            &$crate::tr($key),
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+],
        )
    };
}

/// Transitional: the Chinese or English text. Being replaced by `tr!`;
/// removed once no caller is left.
pub fn text(zh: &str, en: &str) -> String {
    if chinese() { zh } else { en }.to_owned()
}

/// The separator between items of a list ("a, b" or "a、b").
pub fn list_separator() -> &'static str {
    match current() {
        Language::ZhHans | Language::ZhHant | Language::Ja => "、",
        _ => ", ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tags_name_the_languages_the_app_is_written_in() {
        assert_eq!(Language::from_tag("zh-CN"), Some(Language::ZhHans));
        assert_eq!(Language::from_tag("zh-Hans-HK"), Some(Language::ZhHans));
        assert_eq!(Language::from_tag("zh-Hant-TW"), Some(Language::ZhHant));
        assert_eq!(Language::from_tag("zh_TW.UTF-8"), Some(Language::ZhHant));
        assert_eq!(Language::from_tag("pt-PT"), Some(Language::PtBr));
        assert_eq!(Language::from_tag("ja-JP"), Some(Language::Ja));
        assert_eq!(Language::from_tag("it-IT"), None);
    }

    #[test]
    fn the_setting_follows_the_system_language() {
        set_system_languages(&["it-IT", "de-DE"]);
        follow(&json!("system"));
        assert_eq!(current(), Language::De);
        follow(&json!("zh-CN"));
        assert!(chinese());
        follow(&json!("ko"));
        assert_eq!(current(), Language::Ko);
        set_system_languages(&["it-IT"]);
        follow(&json!("system"));
        assert_eq!(current(), Language::En);
    }

    #[test]
    fn missing_keys_stay_in_english_and_placeholders_fill() {
        assert_eq!(
            tr_in(Language::Ja, "A text no catalog has"),
            "A text no catalog has"
        );
        assert_eq!(
            fill("{{a}} of {{b}}", &[("a", &1), ("b", &"two")]),
            "1 of two"
        );
        assert_eq!(
            fill("{{count, number}} left {{x}}", &[("count", &3)]),
            "3 left {{x}}"
        );
    }

    #[test]
    fn counted_forms_follow_each_language() {
        assert_eq!(Language::En.plural(1), "one");
        assert_eq!(Language::En.plural(0), "other");
        assert_eq!(Language::Fr.plural(0), "one");
        assert_eq!(Language::Ja.plural(1), "other");
        assert_eq!(
            tr_count_in(Language::En, "{{count}} hidden", 2, &[]),
            "2 hidden"
        );
    }
}
