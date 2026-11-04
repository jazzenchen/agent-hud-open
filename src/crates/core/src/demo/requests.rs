//! The demo's permission requests, as native requests.
use crate::{iso, l10n};
use serde_json::{json, Value};

/// The demo's permission requests and how many seconds ago each was asked:
/// the test run, one edit worth reading, one build, one ticket and one file.
pub const DEMO_REQUESTS: [(&str, i64); 5] = [
    ("demo-tests", 40),
    ("demo-edit", 38),
    ("demo-build", 124),
    ("demo-ticket", 71),
    ("demo-read", 9),
];

/// One of the demo's requests, as asked at `at`, in the shape a request card
/// reads. No client waits behind it.
pub fn demo_request(id: &str, at: i64) -> Option<Value> {
    let created = iso(at);
    let base = |source: &str,
                session: &str,
                tool: &str,
                summary: String,
                detail: &str,
                cwd: &str| {
        json!({"title":summary,"summary":summary,"source":source,"toolName":tool,"input":detail,"detail":detail,"sessionId":session,"createdAt":created,"cwd":cwd,"allowsAlways":true,"kind":"permission"})
    };
    Some(match id {
        "demo-tests" => base(
            "claude",
            "s1",
            "Bash",
            l10n::text("运行测试", "Run the test suite"),
            "swift test --filter SyncRecordTests",
            "~/work/orbit",
        ),
        "demo-edit" => {
            let mut view = base(
                "claude",
                "demo-1",
                "Edit",
                "Avatar.tsx".into(),
                "~/Projects/acme-web/src/components/Avatar.tsx",
                "~/Projects/acme-web",
            );
            view["path"] = json!("~/Projects/acme-web/src/components/Avatar.tsx");
            view["removed"] = json!("  const initials = user.name.slice(0, 2)");
            view["added"] = json!("  const initials = user?.name?.slice(0, 2) ?? '?'");
            view["suggestions"] = json!([{"type":"addRules","behavior":"allow","destination":"localSettings","rules":[{"toolName":"Edit","ruleContent":"src/**"}]}]);
            view
        }
        "demo-build" => base(
            "codex",
            "demo-2",
            "Bash",
            l10n::text("构建生产版本", "Build for production"),
            "npm run build",
            "~/Projects/acme-api",
        ),
        "demo-ticket" => base(
            "codebuddy",
            "demo-3",
            "mcp__linear__create_issue",
            "linear · create_issue".into(),
            "team: Mobile\ntitle: Settings screen crashes when offline",
            "~/Projects/acme-mobile",
        ),
        "demo-read" => {
            let mut view = base(
                "claude",
                "demo-4",
                "Read",
                "tsconfig.base.json".into(),
                "~/Projects/acme-shared/tsconfig.base.json",
                "~/Projects/acme-web",
            );
            view["path"] = json!("~/Projects/acme-shared/tsconfig.base.json");
            view
        }
        _ => return None,
    })
}
