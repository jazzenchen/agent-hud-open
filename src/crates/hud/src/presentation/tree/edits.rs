//! Edit steps: update, insert, move, remove or replace one node of a tree, named
//! by its id.
mod place;

use super::*;
use place::{place, put};

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
