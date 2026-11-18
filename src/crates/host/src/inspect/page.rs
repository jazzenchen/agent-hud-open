//! What the browser extension picks on a page: an element, with its
//! selector, HTML, words and the styles it is drawn with, or an area; either
//! with a picture of it. The message carries it as material for the agent:
//! where it is, what it holds, and its place in the source when the page's
//! dev server says; anything further the agent finds for itself.
use super::content::Picture;
use agent_hud_runtime::l10n::{fill, tr};
use serde_json::{json, Value};

/// How much of an element's HTML, words and styles a message carries.
const HTML_CHARS: usize = 20_000;
const TEXT_CHARS: usize = 4_000;
const STYLE_CHARS: usize = 4_000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Page {
    pub url: String,
    pub title: String,
    /// An area dragged out, rather than an element picked.
    pub area: bool,
    pub selector: Option<String>,
    /// Its place in the source (`file:line:column`), when a dev server marks it.
    pub source: Option<String>,
    pub html: Option<String>,
    /// Marks a build adds to scope styles, left out of the HTML.
    pub omitted: Vec<Omitted>,
    pub text: Option<String>,
    /// The computed styles that matter for its look, one `name: value;` a line.
    pub styles: Option<String>,
    pub picture: Option<Picture>,
}

/// Scope marks one tool adds (Astro's `data-astro-cid-…`, say), which no
/// source file holds; the HTML goes without them and the message names them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Omitted {
    pub by: String,
    pub marks: Vec<String>,
}

impl Omitted {
    fn list(params: &Value) -> Vec<Self> {
        let Some(list) = params["omitted"].as_array() else {
            return Vec::new();
        };
        list.iter()
            .take(4)
            .filter_map(|entry| {
                let by = field(entry, "by", 40)?;
                let marks: Vec<String> = entry["marks"]
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .take(5)
                    .map(|mark| mark.chars().take(120).collect())
                    .collect();
                (!marks.is_empty()).then_some(Self { by, marks })
            })
            .collect()
    }
}

fn field(params: &Value, key: &str, limit: usize) -> Option<String> {
    params[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(limit).collect())
}

impl Page {
    /// A pick as the extension sends it, with its picture once saved.
    pub(crate) fn from_pick(params: &Value, picture: Option<Picture>) -> Option<Self> {
        let url = field(params, "url", 2_000)?;
        Some(Self {
            title: field(params, "title", 300).unwrap_or_else(|| url.clone()),
            url,
            area: params["kind"] == "area",
            selector: field(params, "selector", 1_000),
            source: field(params, "source", 1_000),
            html: field(params, "html", HTML_CHARS),
            omitted: Omitted::list(params),
            text: field(params, "text", TEXT_CHARS),
            styles: field(params, "styles", STYLE_CHARS),
            picture,
        })
    }

    /// The page's host, as the window names it.
    pub(crate) fn host(&self) -> String {
        let rest = self.url.split("://").nth(1).unwrap_or(&self.url);
        rest.split(['/', '?', '#'])
            .next()
            .unwrap_or(rest)
            .to_owned()
    }

    pub(crate) fn view(&self) -> Value {
        let mut view = self
            .picture
            .as_ref()
            .map(Picture::view)
            .unwrap_or(json!({}));
        view["kind"] = json!("page");
        view["url"] = json!(self.url);
        view["title"] = json!(self.title);
        view["host"] = json!(self.host());
        view["area"] = json!(self.area);
        view["selector"] = json!(self.selector);
        view["text"] = json!(self
            .text
            .as_deref()
            .map(|text| { text.chars().take(600).collect::<String>() }));
        view
    }

    /// The pick in the message: what and where, the picture's path, its
    /// marks, then the element's place in the source, selector, words, HTML
    /// (with the scope marks left out of it) and styles.
    pub(crate) fn message(&self, marks: &[String]) -> String {
        let place = format!("{} ({})", self.title, self.url);
        let mut out = if self.area {
            fill(&tr("An area of {{page}}:"), &[("page", &place)])
        } else {
            fill(&tr("An element on {{page}}:"), &[("page", &place)])
        };
        if let Some(picture) = &self.picture {
            out.push_str(&format!("\n{} {}", tr("Picture:"), picture.path.display()));
            if !marks.is_empty() {
                out.push('\n');
                out.push_str(&tr("The numbers marked on the picture:"));
                for (number, mark) in marks.iter().enumerate() {
                    out.push_str(&format!("\n{}. {mark}", number + 1));
                }
            }
        }
        if let Some(source) = &self.source {
            out.push_str(&format!("\n{} {source}", tr("Source:")));
        }
        if let Some(selector) = &self.selector {
            out.push_str(&format!("\n{} `{selector}`", tr("Selector:")));
        }
        for (label, body, language) in [
            (tr("Its words:"), &self.text, "text"),
            (tr("Its HTML:"), &self.html, "html"),
            (tr("Its styles:"), &self.styles, "css"),
        ] {
            if let Some(body) = body {
                let fence = super::compose::fence_for(body);
                out.push_str(&format!("\n{label}\n{fence}{language}\n{body}\n{fence}"));
                if language == "html" {
                    out.push_str(&self.omissions());
                }
            }
        }
        out
    }

    /// A line for each tool whose scope marks were left out of the HTML.
    fn omissions(&self) -> String {
        self.omitted
            .iter()
            .map(|omitted| {
                let marks: Vec<String> =
                    omitted.marks.iter().map(|mark| format!("`{mark}`")).collect();
                let line = fill(
                    &tr("Left out of the HTML: {{marks}}, which {{maker}} adds to scope its styles."),
                    &[("marks", &marks.join(", ")), ("maker", &omitted.by)],
                );
                format!("\n{line}")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_reads_with_its_page_and_parts() {
        let page = Page::from_pick(
            &json!({
                "kind": "element",
                "url": "http://localhost:5173/settings?tab=1",
                "title": "Settings",
                "selector": "div.toggle-row",
                "html": "<div class=\"toggle-row\">On</div>",
                "styles": "display: flex;",
                "text": "  ",
            }),
            None,
        )
        .unwrap();
        assert_eq!(page.host(), "localhost:5173");
        assert_eq!(page.text, None);
        assert_eq!(
            page.message(&[]),
            "An element on Settings (http://localhost:5173/settings?tab=1):\n\
             Selector: `div.toggle-row`\n\
             Its HTML:\n```html\n<div class=\"toggle-row\">On</div>\n```\n\
             Its styles:\n```css\ndisplay: flex;\n```"
        );
        assert!(Page::from_pick(&json!({"kind": "area"}), None).is_none());
    }

    #[test]
    fn a_pick_names_its_source_and_the_scope_marks_left_out() {
        let page = Page::from_pick(
            &json!({
                "kind": "element",
                "url": "http://localhost:4321/",
                "title": "Home",
                "source": "/work/site/src/components/Pitch.astro:12:5",
                "html": "<ul class=\"pitch\"><li>One</li></ul>",
                "omitted": [
                    { "by": "Astro", "marks": ["data-astro-cid-ba372snk"] },
                    { "by": "Vue", "marks": [] },
                ],
            }),
            None,
        )
        .unwrap();
        assert_eq!(page.omitted.len(), 1);
        assert_eq!(
            page.message(&[]),
            "An element on Home (http://localhost:4321/):\n\
             Source: /work/site/src/components/Pitch.astro:12:5\n\
             Its HTML:\n```html\n<ul class=\"pitch\"><li>One</li></ul>\n```\n\
             Left out of the HTML: `data-astro-cid-ba372snk`, which Astro adds to scope its styles."
        );
    }
}
