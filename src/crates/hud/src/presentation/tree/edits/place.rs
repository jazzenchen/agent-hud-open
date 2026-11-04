//! Where an insert or a move puts a node, and putting it there.
use super::*;

/// Where an insert or a move puts a node: before or after a node, or into one.
pub(super) enum Place {
    Before(String),
    After(String),
    Into(String, Option<usize>),
}

pub(super) fn place(step: &Map<String, Value>, index: usize) -> Result<Place, String> {
    let given: Vec<&str> = ["before", "after", "into"]
        .into_iter()
        .filter(|k| step.get(*k).is_some_and(|v| !v.is_null()))
        .collect();
    if given.len() != 1 {
        return Err(format!(
            "edit[{index}] takes one of before, after or into: the id of the node it goes by or into."
        ));
    }
    let id = step
        .get(given[0])
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Ok(match given[0] {
        "before" => Place::Before(id),
        "after" => Place::After(id),
        _ => {
            let at = match step.get("at").filter(|v| !v.is_null()) {
                None => None,
                Some(v) => match whole(v).filter(|n| *n >= 0) {
                    Some(n) => Some(n as usize),
                    None => return Err(format!("edit[{index}].at is a place from 0.")),
                },
            };
            Place::Into(id, at)
        }
    })
}

/// Puts `node` at `place`, or says why it cannot go there.
pub(super) fn put(
    tree: &mut Value,
    node: Value,
    place: &Place,
    index: usize,
) -> Result<(), String> {
    let (parent, position) = match place {
        Place::Into(id, at) => {
            let Some(path) = locate(tree, id) else {
                return Err(no_node(index, id, tree));
            };
            let len = at_ref(tree, &path)["children"]
                .as_array()
                .map_or(0, Vec::len);
            (path, at.map_or(len, |a| a.min(len)))
        }
        Place::Before(id) | Place::After(id) => {
            let Some(mut path) = locate(tree, id) else {
                return Err(no_node(index, id, tree));
            };
            let Some(last) = path.pop() else {
                return Err(format!(
                    "edit[{index}]: nothing goes before or after the panel itself; put it into \"panel\"."
                ));
            };
            let after = matches!(place, Place::After(_));
            (path, last + usize::from(after))
        }
    };
    let parent_node = at(tree, &parent);
    let parent_kind = kind_of(parent_node).to_owned();
    let kind = kind_of(&node).to_owned();
    let own_block = matches!(kind.as_str(), "quota" | "balance");
    let own_section = matches!(kind.as_str(), "blocks" | "tokens" | "sessions");
    let fits = match parent_kind.as_str() {
        "panel" => !own_block,
        "blocks" => !own_section,
        p if HOLDERS.contains(&p) => !own_block,
        p if PARENTS.contains(&p) => !own_block && !own_section,
        _ => false,
    };
    if !fits {
        let into = parent_node["id"]
            .as_str()
            .unwrap_or(&parent_kind)
            .to_owned();
        return Err(if own_block || own_section {
            format!(
                "edit[{index}]: \"{}\" stays {}.",
                node["id"].as_str().unwrap_or(&kind),
                if own_block {
                    "inside blocks"
                } else {
                    "in the panel, or in a row, column or grid of yours placed there"
                }
            )
        } else {
            format!(
                "edit[{index}]: nothing goes into \"{into}\", a {parent_kind}. Controls go into panel, blocks, or a row, column, grid or repeat."
            )
        });
    }
    // A section's width is its column's in a row of yours; at the panel's top it has none.
    let mut node = node;
    if own_section && parent_kind == "panel" {
        if let Some(fields) = node.as_object_mut() {
            fields.remove("width");
        }
    }
    let children = parent_node
        .as_object_mut()
        .map(|o| o.entry("children").or_insert(json!([])));
    match children.and_then(Value::as_array_mut) {
        Some(list) => {
            let position = position.min(list.len());
            list.insert(position, node);
            Ok(())
        }
        None => Err(format!("edit[{index}]: that node holds no children.")),
    }
}
