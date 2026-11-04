//! Edit steps: update, insert, move, remove or replace one node of a tree, named
//! by its id.
use super::*;

/// Where a node sits in a tree: the indices of `children` from the root.
fn locate(tree: &Value, id: &str) -> Option<Vec<usize>> {
    if tree["id"] == id {
        return Some(vec![]);
    }
    for (index, child) in tree["children"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(mut path) = locate(child, id) {
            path.insert(0, index);
            return Some(path);
        }
    }
    None
}

fn at<'a>(tree: &'a mut Value, path: &[usize]) -> &'a mut Value {
    let mut node = tree;
    for index in path {
        node = &mut node["children"][*index];
    }
    node
}

fn ids(tree: &Value, out: &mut Vec<String>) {
    if let Some(id) = tree["id"].as_str() {
        out.push(id.to_owned());
    }
    for child in tree["children"].as_array().into_iter().flatten() {
        ids(child, out);
    }
}

fn no_node(index: usize, id: &str, tree: &Value) -> String {
    let mut known = vec![];
    ids(tree, &mut known);
    known.truncate(40);
    format!(
        "edit[{index}]: no node has the id \"{id}\". Ids now: {}.",
        quoted(&known)
    )
}

/// Where an insert or a move puts a node: before or after a node, or into one.
enum Place {
    Before(String),
    After(String),
    Into(String, Option<usize>),
}

fn place(step: &Map<String, Value>, index: usize) -> Result<Place, String> {
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
fn put(tree: &mut Value, node: Value, place: &Place, index: usize) -> Result<(), String> {
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
                    "inside panel"
                }
            )
        } else {
            format!(
                "edit[{index}]: nothing goes into \"{into}\", a {parent_kind}. Controls go into panel, blocks, or a row, column, grid or repeat."
            )
        });
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

fn at_ref<'a>(tree: &'a Value, path: &[usize]) -> &'a Value {
    let mut node = tree;
    for index in path {
        node = &node["children"][*index];
    }
    node
}

/// The tree with `edits` applied in order, and a note for each that did something
/// other than asked; every edit that cannot be done is a reason.
pub(in crate::presentation) fn apply(
    tree: &mut Value,
    edits: &Value,
) -> (Vec<String>, Vec<String>) {
    let (mut issues, mut notes) = (vec![], vec![]);
    let Some(steps) = edits.as_array() else {
        return (
            vec!["edit is a list of steps, such as [{\"update\": \"claude\", \"widget\": \"rings\"}].".into()],
            notes,
        );
    };
    for (index, step) in steps.iter().enumerate() {
        let Some(step) = step.as_object() else {
            issues.push(format!("edit[{index}] is an object, such as {{\"update\": \"claude\", \"widget\": \"rings\"}}."));
            continue;
        };
        let verbs: Vec<&str> = EDITS
            .iter()
            .copied()
            .filter(|v| step.get(*v).is_some_and(|x| !x.is_null()))
            .collect();
        if verbs.len() != 1 {
            issues.push(format!("edit[{index}] takes one of {}.", quoted(EDITS)));
            continue;
        }
        let verb = verbs[0];
        let target = step
            .get(verb)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let result = match verb {
            "update" => {
                let Some(path) = locate(tree, &target) else {
                    issues.push(no_node(index, &target, tree));
                    continue;
                };
                let node = at(tree, &path);
                let mut bad = vec![];
                for (key, value) in step {
                    if key == "update" {
                        continue;
                    }
                    if ["id", "type", "children"].contains(&key.as_str()) {
                        bad.push(format!("edit[{index}]: update does not change {key}; use insert, move, remove or replace."));
                        continue;
                    }
                    if let Some(fields) = node.as_object_mut() {
                        if value.is_null() {
                            fields.remove(key);
                        } else {
                            fields.insert(key.clone(), value.clone());
                        }
                    }
                }
                if bad.is_empty() {
                    Ok(())
                } else {
                    Err(bad.join(" "))
                }
            }
            "insert" => {
                let node = step.get("insert").cloned().unwrap_or(Value::Null);
                if !node.is_object() {
                    Err(format!("edit[{index}].insert is the node to add."))
                } else if OWN.contains(&kind_of(&node)) {
                    Err(format!("edit[{index}]: the panel's own parts are there already; move or update them."))
                } else {
                    place(step, index).and_then(|p| put(tree, node, &p, index))
                }
            }
            "move" => match locate(tree, &target) {
                None => Err(no_node(index, &target, tree)),
                Some(path) if path.is_empty() => {
                    Err(format!("edit[{index}]: the panel itself does not move."))
                }
                Some(path) => match place(step, index) {
                    Err(e) => Err(e),
                    Ok(place) => {
                        let snapshot = tree.clone();
                        let (last, parent) = path.split_last().unwrap();
                        let node = at(tree, parent)["children"]
                            .as_array_mut()
                            .unwrap()
                            .remove(*last);
                        let moved = put(tree, node, &place, index);
                        if moved.is_err() {
                            *tree = snapshot;
                        }
                        moved
                    }
                },
            },
            "remove" => match locate(tree, &target) {
                None => Err(no_node(index, &target, tree)),
                Some(path) if path.is_empty() => {
                    Err(format!("edit[{index}]: the panel itself stays."))
                }
                Some(path) => {
                    let node = at(tree, &path);
                    if OWN.contains(&kind_of(node)) {
                        node["hidden"] = json!(true);
                        notes.push(format!(
                            "\"{target}\" is one of the panel's own parts, so it is hidden, not deleted; {{\"update\": \"{target}\", \"hidden\": false}} shows it again."
                        ));
                    } else {
                        let (last, parent) = path.split_last().unwrap();
                        at(tree, parent)["children"]
                            .as_array_mut()
                            .unwrap()
                            .remove(*last);
                    }
                    Ok(())
                }
            },
            _ => match locate(tree, &target) {
                None => Err(no_node(index, &target, tree)),
                Some(path) => {
                    let node = at(tree, &path);
                    let with = step.get("with").cloned().unwrap_or(Value::Null);
                    if OWN.contains(&kind_of(node)) {
                        Err(format!("edit[{index}]: \"{target}\" is one of the panel's own parts; change it with update."))
                    } else if !with.is_object() || OWN.contains(&kind_of(&with)) {
                        Err(format!(
                            "edit[{index}].with is the control to put in its place."
                        ))
                    } else {
                        *node = with;
                        Ok(())
                    }
                }
            },
        };
        if let Err(reason) = result {
            issues.push(reason);
        }
    }
    (issues, notes)
}
