//! The words a send puts into the session: the person's note, then each
//! thing they copied, numbered when there are several, under a line that
//! says what it is and where it was copied, with their comment below it.
//! Copied words sit in a code fence, so the agent reads them as material,
//! not as the person's request; a picture goes as the path of its file.
use super::content::Content;
use super::draft::Item;
use agent_hud_runtime::l10n::{fill, tr};

pub(crate) fn message(items: &[Item], note: &str) -> String {
    let mut parts = Vec::new();
    let note = note.trim();
    if !note.is_empty() {
        parts.push(note.to_owned());
    }
    let numbered = items.len() > 1;
    for (index, item) in items.iter().enumerate() {
        let mut block = String::new();
        if numbered {
            block.push_str(&format!("{}. ", index + 1));
        }
        block.push_str(&label(&item.content, item.app.as_deref()));
        match &item.content {
            Content::Page(page) => block.push_str(&page.message(&item.marks)),
            Content::Text(text) => {
                let fence = fence_for(text);
                block.push_str(&format!("\n{fence}\n{}\n{fence}", text.trim_end()));
            }
            Content::Image(picture) => {
                block.push_str(&format!(" {}", picture.path.display()));
                if !item.marks.is_empty() {
                    block.push('\n');
                    block.push_str(&tr("The numbers marked on the picture:"));
                    for (number, mark) in item.marks.iter().enumerate() {
                        block.push_str(&format!("\n{}. {mark}", number + 1));
                    }
                }
            }
            Content::Files(paths) => {
                for path in paths {
                    block.push_str(&format!("\n{path}"));
                }
            }
        }
        let comment = item.comment.trim();
        if !comment.is_empty() {
            block.push('\n');
            block.push_str(comment);
        }
        parts.push(block);
    }
    parts.join("\n\n")
}

fn label(content: &Content, app: Option<&str>) -> String {
    let from = |template: String, app: &str| fill(&template, &[("app", &app)]);
    match (content, app.filter(|app| !app.is_empty())) {
        (Content::Text(_), Some(app)) => from(tr("Text copied from {{app}}:"), app),
        (Content::Text(_), None) => tr("Copied text:"),
        (Content::Image(_), Some(app)) => from(tr("Picture copied from {{app}}:"), app),
        (Content::Image(_), None) => tr("Copied picture:"),
        (Content::Files(_), Some(app)) => from(tr("Files copied from {{app}}:"), app),
        (Content::Files(_), None) => tr("Copied files:"),
        // A pick from a page says what and where itself.
        (Content::Page(_), _) => String::new(),
    }
}

/// Backticks enough to fence `text`: one more than its longest run, at
/// least three.
pub(super) fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for character in text.chars() {
        run = if character == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    "`".repeat((longest + 1).max(3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::content::Picture;

    fn item(content: Content, comment: &str) -> Item {
        Item {
            id: "x".into(),
            app: Some("Terminal".into()),
            content,
            comment: comment.into(),
            marks: Vec::new(),
        }
    }

    #[test]
    fn one_copy_goes_alone_unnumbered() {
        let text = message(&[item(Content::Text("error: boom\n".into()), "")], "");
        assert_eq!(text, "Text copied from Terminal:\n```\nerror: boom\n```");
    }

    #[test]
    fn a_marked_picture_lists_its_numbered_comments() {
        let picture = std::env::temp_dir().join("m.png");
        let marked = Item {
            marks: vec!["too wide".into(), "wrong colour".into()],
            ..item(
                Content::Image(Picture {
                    path: picture.clone(),
                    thumbnail: None,
                    width: 1,
                    height: 1,
                }),
                "fix both",
            )
        };
        assert_eq!(
            message(&[marked], ""),
            format!(
                "Picture copied from Terminal: {}\n\
                 The numbers marked on the picture:\n1. too wide\n2. wrong colour\nfix both",
                picture.display()
            )
        );
    }

    #[test]
    fn several_go_numbered_under_the_note_with_their_comments() {
        let picture = std::env::temp_dir().join("p.png");
        let items = [
            item(Content::Text("a ``` b".into()), "why this"),
            item(
                Content::Image(Picture {
                    path: picture.clone(),
                    thumbnail: None,
                    width: 1,
                    height: 1,
                }),
                "",
            ),
            Item {
                app: None,
                ..item(Content::Files(vec!["/a".into(), "/b".into()]), "both")
            },
        ];
        let text = message(&items, "  fix these  ");
        assert_eq!(
            text,
            format!(
                "fix these\n\n\
                 1. Text copied from Terminal:\n````\na ``` b\n````\nwhy this\n\n\
                 2. Picture copied from Terminal: {}\n\n\
                 3. Copied files:\n/a\n/b\nboth",
                picture.display()
            )
        );
    }
}
