use super::*;

/// Numbers within 1e-9; everything else equal.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 1e-9
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same(a, b)))
        }
        _ => a == b,
    }
}

/// director-cases.json holds steps the TypeScript MarkDirector took, with
/// what it published and when it would next change after each.
#[test]
fn directs_as_the_typescript_director_did() {
    let cases: Value = serde_json::from_str(include_str!("director-cases.json")).unwrap();
    let mut steps = 0;
    for case in cases["cases"].as_array().unwrap() {
        let mut director = Director::new(case["startedAt"].as_f64().unwrap());
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let now = step["now"].as_f64().unwrap();
            if let Some(snapshot) = step.get("read") {
                director.read(snapshot, now);
            } else if let Some(keys) = step.get("looked") {
                let keys: Option<Vec<String>> = serde_json::from_value(keys.clone()).unwrap();
                director.looked(keys.as_deref(), now);
            } else {
                director.advance(now);
            }
            let place = format!("{} step {index}", case["name"]);
            let wire = director.wire();
            assert!(
                same(&wire, &step["wire"]),
                "{place}: published\n{wire}\nnot\n{}",
                step["wire"]
            );
            let next = director.next_change(now);
            assert!(
                same(&json!(next), &step["nextChange"]),
                "{place}: next change {next:?}, not {}",
                step["nextChange"]
            );
            steps += 1;
        }
    }
    assert!(steps >= 150);
}
