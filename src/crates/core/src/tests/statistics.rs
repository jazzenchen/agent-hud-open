//! Statistics drawn from the ledger, asked the way the Usage page asks.
use super::*;

#[test]
fn every_query_at_a_revision_answers_as_it_would_alone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/p");
    fs::create_dir_all(&root).unwrap();
    let now = Utc::now().timestamp_millis();
    let (hour, day) = (3_600_000i64, 86_400_000i64);
    let iso = |at: i64| {
        DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339()
    };
    // Sessions of different ages, the oldest beyond a year.
    let ages = [hour, 2 * day, 20 * day, 45 * day, 200 * day, 380 * day];
    for (index, age) in ages.iter().enumerate() {
        let id = format!("3111111{index}-2222-3333-4444-555555555555");
        let model = ["claude-opus-4-5", "claude-sonnet-4-5"][index % 2];
        let prompt = json!({"type":"user","sessionId":id,"uuid":"p","timestamp":iso(now - age - 1_000),
            "message":{"role":"user","content":"Go on."}});
        // The second call sends the whole prompt again: the cache lapsed.
        let call = |message: &str, at: i64, read: u64| {
            json!({"type":"assistant","sessionId":id,"timestamp":iso(at),
                "message":{"id":message,"model":model,"role":"assistant","content":[{"type":"text","text":"Done."}],
                "usage":{"input_tokens":1_000,"output_tokens":500,"cache_creation_input_tokens":20_000,"cache_read_input_tokens":read}}})
        };
        fs::write(
            root.join(format!("{id}.jsonl")),
            format!(
                "{prompt}\n{}\n{}\n",
                call("m1", now - age, 5_000),
                call("m2", now - age + 60_000, 0)
            ),
        )
        .unwrap();
    }
    let mut core = fixture(temp.path());
    core.request("snapshot", json!({})).unwrap();
    let range = |from: i64, to: i64, bucket: i64, extra: Value| {
        let mut query = json!({"from":iso(from),"to":iso(to),"bucketMinutes":bucket,
            "tokenKinds":["input","output","cacheWrite"],"costKinds":"all","timeZone":"UTC"});
        query
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        query
    };
    let queries = [
        range(now - 30 * day, now, 1440, json!({})),
        range(now - 370 * day, now, 1440, json!({})),
        range(now - 30 * day, now, 1440, json!({"providers":["claude"]})),
        range(
            now - 30 * day,
            now,
            1440,
            json!({"models":["claude-model:claude-opus-4-5"]}),
        ),
        range(now - 90 * day, now, 1440, json!({})),
        range(now - 7 * day, now, 60, json!({})),
        range(
            now - 200 * day - 12 * hour,
            now - 199 * day - 12 * hour,
            60,
            json!({}),
        ),
        range(now - 365 * day, now, 1440, json!({})),
    ];
    let ask = |core: &mut AgentHudCore, query: &Value| {
        let mut value = core.request("statistics", query.clone()).unwrap();
        value.as_object_mut().unwrap().remove("generatedAt");
        value
    };
    let first = queries
        .iter()
        .map(|query| ask(&mut core, query))
        .collect::<Vec<_>>();
    // Asked again, the answers come from the cache.
    let again = queries
        .iter()
        .map(|query| ask(&mut core, query))
        .collect::<Vec<_>>();
    let revision = &first[0]["dataRevision"];
    for (index, query) in queries.iter().enumerate() {
        assert_eq!(&first[index]["dataRevision"], revision, "one revision");
        core.statistics.cache = Default::default();
        let alone = ask(&mut core, query);
        assert_eq!(first[index], alone, "{query}");
        assert_eq!(again[index], alone, "{query}");
    }
    assert!(
        first[0]["totals"]["selectedTokens"].as_u64().unwrap() > 0
            && first[1]["providers"][0]["recachedCost"].is_number(),
        "{}",
        first[1]
    );
    assert!(first[0]["previous"].is_object(), "the month before");
    assert!(first[4]["previous"].is_object(), "the quarter before");
    assert!(first[1]["previous"].is_null(), "no year before");
    assert!(
        first[6]["totals"]["selectedTokens"].as_u64().unwrap() > 0,
        "the day picked"
    );
}
