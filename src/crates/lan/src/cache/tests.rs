use super::*;
use sha2::Digest;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Item {
    key: String,
    value: f64,
    at: i64,
}
impl Record for Item {
    fn id(&self) -> String {
        self.key.clone()
    }
    fn device_id(&self) -> &str {
        "mac"
    }
    fn modified_at_ms(&self) -> i64 {
        self.at
    }
    fn is_visible(&self, _: i64) -> bool {
        true
    }
    fn same_content(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}
fn item(key: &str, value: f64, at: i64) -> Item {
    Item {
        key: key.into(),
        value,
        at,
    }
}
/// The revision as the phone checks it: its JSON form of the whole snapshot.
fn whole(cache: &SnapshotCache<Item>) -> String {
    let snapshot = cache.snapshot().unwrap();
    let value = json!({"cursor": snapshot.cursor, "records": snapshot.records});
    format!(
        "{:x}",
        sha2::Sha256::digest(crate::canonical::to_vec(&value))
    )
}

#[test]
fn a_version_carries_only_its_changes_and_the_revision_of_the_whole() {
    let folder = std::env::temp_dir().join(format!("lan-cache-{}", std::process::id()));
    crate::fs::private_dir(&folder).unwrap();
    let mut cache =
        SnapshotCache::<Item>::new("mac".into(), "Mac".into(), folder.join("cursor.json")).unwrap();
    let first = [
        item("a", 1.0, 10),
        item("b", 0.00001, 10),
        item("c", 3.5, 10),
    ];
    assert!(cache.update(&first, 1_000).unwrap());
    let start = cache.head().unwrap().cursor.clone();
    assert_eq!(cache.head().unwrap().revision, whole(&cache));

    // The same content with a later time publishes nothing.
    let later = [
        item("a", 1.0, 20),
        item("b", 0.00001, 20),
        item("c", 3.5, 20),
    ];
    assert!(!cache.update(&later, 2_000).unwrap());
    assert_eq!(cache.snapshot().unwrap().records[0].at, 10);

    // One change and one deletion make the next minor version.
    let next = [item("a", 2.0, 30), item("b", 0.00001, 30)];
    assert!(cache.update(&next, 3_000).unwrap());
    let head = cache.head().unwrap().clone();
    assert_eq!(head.cursor.base_version, start.base_version);
    assert_eq!(head.cursor.change_sequence, 1);
    assert_eq!(head.revision, whole(&cache));
    let changes = cache.changes(&start).unwrap();
    assert_eq!(changes.records, vec![item("a", 2.0, 30)]);
    assert_eq!(changes.deleted_ids, vec!["c".to_string()]);

    // A baseline is a new major version over the same records.
    let baseline = cache.baseline(4_000).unwrap().unwrap();
    assert_eq!(baseline.cursor.change_sequence, 0);
    assert_ne!(baseline.cursor.base_version, start.base_version);
    assert_eq!(cache.head().unwrap().revision, whole(&cache));
    let served: Value = serde_json::from_slice(&cache.body().unwrap().unwrap()).unwrap();
    assert_eq!(
        served,
        serde_json::to_value(cache.snapshot().unwrap()).unwrap()
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_version_that_could_not_be_saved_goes_out_on_the_next_update() {
    let folder = std::env::temp_dir().join(format!("lan-unsaved-{}", std::process::id()));
    crate::fs::private_dir(&folder).unwrap();
    let file = folder.join("cursor.json");
    let mut cache = SnapshotCache::<Item>::new("mac".into(), "Mac".into(), file.clone()).unwrap();
    assert!(cache
        .update(&[item("a", 1.0, 10), item("c", 3.0, 10)], 1_000)
        .unwrap());
    let start = cache.head().unwrap();
    // A folder where the cursor goes makes saving it fail.
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(file.join("blocked")).unwrap(); // plain directory
    let next = [item("a", 2.0, 20)];
    assert!(cache.update(&next, 2_000).is_err());
    // What is served is still the saved version, whole.
    let head = cache.head().unwrap();
    assert_eq!(head.cursor, start.cursor);
    assert_eq!(head.revision, start.revision);
    assert_eq!(head.revision, whole(&cache));
    assert_eq!(
        cache.snapshot().unwrap().records,
        vec![item("a", 1.0, 10), item("c", 3.0, 10)]
    );
    // Once saving works, the same update publishes the change.
    std::fs::remove_dir_all(&file).unwrap();
    assert!(cache.update(&next, 3_000).unwrap());
    let changes = cache.changes(&start.cursor).unwrap();
    assert_eq!(changes.records, vec![item("a", 2.0, 20)]);
    assert_eq!(changes.deleted_ids, vec!["c".to_string()]);
    assert_eq!(cache.head().unwrap().revision, whole(&cache));
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_revision_hashed_only_when_asked_is_still_the_whole_ones() {
    // Commits that nobody reads in between are hashed once, when asked,
    // and the answer is the latest version's, never an earlier one's.
    let folder = std::env::temp_dir().join(format!("lan-lazy-{}", std::process::id()));
    crate::fs::private_dir(&folder).unwrap();
    let mut cache =
        SnapshotCache::<Item>::new("mac".into(), "Mac".into(), folder.join("cursor.json")).unwrap();
    assert!(cache.head().is_none());
    assert!(cache.update(&[item("a", 1.0, 10)], 1_000).unwrap());
    let first = cache.head().unwrap().revision;
    assert!(cache.update(&[item("a", 2.0, 20)], 2_000).unwrap());
    assert!(cache
        .update(&[item("a", 2.0, 20), item("b", 1.0, 20)], 3_000)
        .unwrap());
    let latest = cache.head().unwrap();
    assert_ne!(latest.revision, first);
    assert_eq!(latest.revision, whole(&cache));
    assert_eq!(latest.cursor.change_sequence, 2);
    let served: Value = serde_json::from_slice(&cache.body().unwrap().unwrap()).unwrap();
    assert_eq!(served["revision"], latest.revision.as_str());
    // A change set names the same revision as the head.
    let changes = cache.changes(&Cursor {
        change_sequence: 0,
        ..latest.cursor.clone()
    });
    assert_eq!(changes.unwrap().revision, latest.revision);
    cache.clear();
    assert!(cache.head().is_none());
    std::fs::remove_dir_all(&folder).unwrap();
}
