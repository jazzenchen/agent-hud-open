use super::*;
use crate::database::metering::{write, Event, View};
use crate::{pricing, Usage};

/// The desktop app on the sign-in, the command line on a relay and a local
/// server, an older call on another relay left out: each client shows its
/// own, and a log of no known client none.
#[test]
fn each_client_shows_what_its_own_calls_went_through() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = |key: &str, model: &str, at: i64| Event {
        key: key.into(),
        model: model.into(),
        at,
        usage: Usage {
            input: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    let logs = [
        (
            "/logs/desktop.jsonl",
            vec![call("d1", "gpt-5.4", now - 3_600_000)],
        ),
        (
            "/logs/cli.jsonl",
            vec![
                call("c1", "gpt-5.4#pool", now - 600_000),
                call("c2", "qwen3-coder:30b#ollama", now - 600_000),
                call("c3", "gpt-5.4#old", now - 40 * 86_400_000),
            ],
        ),
        (
            "/logs/other.jsonl",
            vec![call("o1", "gpt-5.4", now - 600_000)],
        ),
    ];
    for (path, calls) in &logs {
        write(&db, path, "codex", calls).unwrap();
    }
    let paths: Vec<String> = logs.iter().map(|(path, _)| path.to_string()).collect();
    let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
    let client_of = |path: &str| match path {
        "/logs/desktop.jsonl" => Some("codex-desktop".to_string()),
        "/logs/cli.jsonl" => Some("codex".to_string()),
        _ => None,
    };
    let relays = HashMap::from([("pool", "pool.example.net"), ("old", "old.example.net")]);
    let spends = tally(&view, now - WINDOW_MS, client_of, &relays);
    assert_eq!(spends.len(), 2);
    assert_eq!(
        spends["codex-desktop"],
        Spends {
            own: true,
            ..Default::default()
        }
    );
    assert_eq!(
        spends["codex"],
        Spends {
            own: false,
            relays: BTreeSet::from(["pool.example.net".to_string()]),
            routes: BTreeSet::from(["ollama".to_string()]),
        }
    );
    assert_eq!(
        spends["codex"].json(),
        json!({"own":false,"relays":["pool.example.net"],"routes":["ollama"]})
    );
}
