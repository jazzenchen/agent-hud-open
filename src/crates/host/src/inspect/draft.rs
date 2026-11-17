//! The clipboard window's state: the latest copy, in one place that each copy
//! takes over, and the draft, which holds only what the person added there,
//! each with a comment of its own, and a note for the whole message. With
//! nothing added, the latest copy is what goes.
use super::clipboard::ClipKind;
use super::content::Content;
use serde_json::{json, Value};

/// The latest copy. Its content is read when the person points at it.
#[derive(Clone, Debug)]
pub(crate) struct Clip {
    pub id: String,
    pub kind: ClipKind,
    /// The app in front when it was copied.
    pub app: Option<String>,
    pub at_ms: i64,
    /// The clipboard's change count it was seen at: its content is read only
    /// while the clipboard still holds it.
    pub change: i64,
    pub content: Option<Content>,
}

/// Something the person added to the draft.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub id: String,
    pub app: Option<String>,
    pub content: Content,
    pub comment: String,
}

#[derive(Debug, Default)]
pub(crate) struct Draft {
    pub clip: Option<Clip>,
    pub items: Vec<Item>,
    pub note: String,
}

impl Draft {
    /// A new copy takes the latest copy's place; `None` empties it (a copy
    /// the bubble does not show also took the clipboard).
    pub(crate) fn copied(&mut self, clip: Option<Clip>) {
        self.clip = clip;
    }

    /// Adds the latest copy, once its content is read. False when there is
    /// nothing to add.
    pub(crate) fn add_clip(&mut self) -> bool {
        let Some(clip) = &self.clip else {
            return false;
        };
        if self.items.iter().any(|item| item.id == clip.id) {
            return true;
        }
        let Some(content) = clip.content.clone() else {
            return false;
        };
        self.items.push(Item {
            id: clip.id.clone(),
            app: clip.app.clone(),
            content,
            comment: String::new(),
        });
        true
    }

    /// Writes `comment` on item `id`. Writing on the latest copy adds it:
    /// nobody comments on what they do not mean to send.
    pub(crate) fn comment(&mut self, id: &str, comment: &str) {
        if self.clip.as_ref().is_some_and(|clip| clip.id == id) {
            self.add_clip();
        }
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.comment = comment.to_owned();
        }
    }

    pub(crate) fn remove(&mut self, id: &str) {
        self.items.retain(|item| item.id != id);
    }

    /// Lets go of everything: the draft, its note and the latest copy.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// What a send carries: the draft's items, or with none, the latest copy.
    pub(crate) fn outgoing(&self) -> Vec<Item> {
        if !self.items.is_empty() {
            return self.items.clone();
        }
        self.clip
            .as_ref()
            .and_then(|clip| {
                Some(Item {
                    id: clip.id.clone(),
                    app: clip.app.clone(),
                    content: clip.content.clone()?,
                    comment: String::new(),
                })
            })
            .into_iter()
            .collect()
    }

    /// What every HUD window draws.
    pub(crate) fn view(&self) -> Value {
        let clip = self.clip.as_ref().map(|clip| {
            json!({
                "id": clip.id,
                "kind": clip.kind,
                "app": clip.app,
                "at": clip.at_ms,
                "added": self.items.iter().any(|item| item.id == clip.id),
                "content": clip.content.as_ref().map(Content::view),
            })
        });
        let items = self
            .items
            .iter()
            .map(|item| {
                json!({
                    "id": item.id,
                    "kind": item.content.kind(),
                    "app": item.app,
                    "comment": item.comment,
                    "content": item.content.view(),
                })
            })
            .collect::<Vec<_>>();
        json!({"clip": clip, "items": items, "note": self.note})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(id: &str, text: Option<&str>) -> Clip {
        Clip {
            id: id.into(),
            kind: ClipKind::Text,
            app: Some("Terminal".into()),
            at_ms: 1,
            change: 1,
            content: text.map(|text| Content::Text(text.into())),
        }
    }

    #[test]
    fn a_copy_takes_the_last_ones_place_and_joins_only_when_added() {
        let mut draft = Draft::default();
        draft.copied(Some(clip("a", Some("first"))));
        draft.copied(Some(clip("b", Some("second"))));
        assert!(draft.items.is_empty());
        assert_eq!(draft.outgoing()[0].content, Content::Text("second".into()));
        assert!(draft.add_clip());
        assert!(draft.add_clip());
        assert_eq!(draft.items.len(), 1);
        draft.copied(Some(clip("c", Some("third"))));
        // With something added, only what was added goes.
        let out = draft.outgoing();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "b");
    }

    #[test]
    fn a_comment_on_the_latest_copy_adds_it() {
        let mut draft = Draft::default();
        draft.copied(Some(clip("a", Some("words"))));
        draft.comment("a", "this one");
        assert_eq!(draft.items.len(), 1);
        assert_eq!(draft.items[0].comment, "this one");
        assert_eq!(draft.view()["clip"]["added"], true);
    }

    #[test]
    fn an_unread_copy_cannot_be_added_and_clear_lets_go_of_all() {
        let mut draft = Draft::default();
        draft.copied(Some(clip("a", None)));
        assert!(!draft.add_clip());
        assert!(draft.outgoing().is_empty());
        draft.note = "note".into();
        draft.clear();
        assert!(draft.clip.is_none() && draft.items.is_empty() && draft.note.is_empty());
    }
}
