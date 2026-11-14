//! Agent HUD's hooks an agent skips until the user allows them in the agent
//! itself (Codex keeps a hook whose command changed `modified`), told on the
//! island with a way to the agent, at most once a day while they wait.
use super::*;
use crate::l10n::tr;

/// Told at most once a day while the hooks wait.
const REVIEW_TOLD_EVERY_MS: i64 = 86_400_000;
/// Agent HUD rewrites the hooks as it starts, and the island takes what the
/// first snapshot holds as already seen: what waits is told only once the
/// app has run this long.
const AFTER_LAUNCH_MS: i64 = 20_000;

impl Tracker {
    /// An agent whose last account reading says it skips some of Agent
    /// HUD's hooks (`unreviewedHooks`) is told on the island: soon after
    /// launch, since a launch is when the hooks change, then at most once a
    /// day, and at once again after they were allowed and changed again.
    pub(super) fn hook_reviews(&mut self, providers: &[Value], now: i64) {
        for provider in providers {
            let Some(id) = provider["id"].as_str() else {
                continue;
            };
            let events: Vec<&str> = provider["accountStatus"]["unreviewedHooks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if events.is_empty() || provider["enabled"] == false {
                self.reviews.remove(id);
                continue;
            }
            if now - self.started < AFTER_LAUNCH_MS
                || self
                    .reviews
                    .get(id)
                    .is_some_and(|at| now - at < REVIEW_TOLD_EVERY_MS)
            {
                continue;
            }
            self.reviews.insert(id.into(), now);
            let name = provider["name"].as_str().unwrap_or(id);
            // Without the permission hook its approvals stay in the agent;
            // the other hooks only report how its sessions are doing.
            let message = if events.contains(&"permissionRequest") {
                tr!(
                    "{{name}} skips Agent HUD's hooks until you allow them, so its approvals won't reach the HUD.",
                    name = name
                )
            } else {
                tr!(
                    "{{name}} skips Agent HUD's hooks until you allow them, so the HUD learns of its sessions late.",
                    name = name
                )
            };
            self.push(
                &format!("{id}:hooks-review:{now}"),
                json!({"kind":"hooks-review-needed","islandAlert":true,"at":iso(now),"source":id,"title":provider["name"],"message":message,"step":agent_hud_agents::hooks::setup_step(id),"hooks":events}),
            );
        }
    }
}
