//! Vendors' public status pages (Atlassian Statuspage `summary.json`), read
//! for the providers that name one (`Provider::status`). The same for every
//! provider: one request per page every five minutes, in the account queue,
//! never with a credential. A provider shows a status only while one of the
//! components it depends on is not operational.
use agent_hud_agents::account::StatusPage;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;

/// How often a page is read.
pub(super) const INTERVAL: Duration = Duration::from_secs(300);

/// A page's components and open incidents, as far as they are shown: each
/// component's name and status, each incident's name, impact and the
/// components it names. None when the page could not be read.
pub(super) fn read(http: &Client, url: &str) -> Option<Value> {
    let response = http
        .get(url)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(10))
        .send()
        .ok()?;
    let summary = super::http::read_json(response).ok()?;
    Some(trimmed(&summary))
}

fn trimmed(summary: &Value) -> Value {
    let components = summary["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|component| {
            Some(json!({
                "name": component["name"].as_str()?,
                "status": component["status"].as_str()?,
            }))
        })
        .collect::<Vec<_>>();
    let incidents = summary["incidents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|incident| {
            let names = incident["components"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|component| component["name"].as_str())
                .collect::<Vec<_>>();
            Some(json!({
                "name": incident["name"].as_str()?,
                "impact": incident["impact"],
                "components": names,
            }))
        })
        .collect::<Vec<_>>();
    json!({"components": components, "incidents": incidents})
}

/// How bad a component status is; operational is 0, unknown words are not ranked.
fn rank(status: &str) -> Option<u8> {
    match status {
        "operational" => Some(0),
        "under_maintenance" => Some(1),
        "degraded_performance" => Some(2),
        "partial_outage" => Some(3),
        "major_outage" => Some(4),
        _ => None,
    }
}

/// What `page` says about the components a provider depends on: the worst
/// status among them, the components in that state and the first open
/// incident naming one of them. None while they are all operational or the
/// page names none of them.
pub(super) fn service(page: &StatusPage, summary: &Value, checked_at: &str) -> Option<Value> {
    let mine = |name: &str| page.components.contains(&name);
    let (worst, affected) = summary["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|component| {
            let name = component["name"].as_str().filter(|name| mine(name))?;
            let status = component["status"].as_str()?;
            Some((rank(status)?, status, name))
        })
        .fold(
            (None, Vec::new()),
            |(worst, mut names), (rank, status, name)| match worst {
                Some((top, _)) if top > rank => (worst, names),
                Some((top, _)) if top == rank => {
                    names.push(name);
                    (worst, names)
                }
                _ => (Some((rank, status)), vec![name]),
            },
        );
    let (rank, status) = worst?;
    if rank == 0 {
        return None;
    }
    let incident = summary["incidents"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|incident| {
            incident["components"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|name| name.as_str().is_some_and(mine))
        })
        .and_then(|incident| incident["name"].as_str());
    Some(json!({
        "status": status,
        "components": affected,
        "incident": incident,
        "page": page_address(page.url),
        "checkedAt": checked_at,
    }))
}

/// The page people read: the API address without its path.
fn page_address(url: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).ok()?;
    Some(format!("{}://{}", url.scheme(), url.host_str()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    static CLAUDE: StatusPage = StatusPage {
        url: "https://status.claude.com/api/v2/summary.json",
        components: &["Claude Code", "Claude API (api.anthropic.com)"],
    };

    fn summary(code: &str, api: &str, incidents: Value) -> Value {
        trimmed(&json!({
            "components": [
                {"name": "claude.ai", "status": "major_outage"},
                {"name": "Claude Code", "status": code},
                {"name": "Claude API (api.anthropic.com)", "status": api},
            ],
            "incidents": incidents,
        }))
    }

    #[test]
    fn only_the_providers_own_components_count() {
        // claude.ai is down, but Claude Code and the API are not.
        let calm = summary("operational", "operational", json!([]));
        assert_eq!(service(&CLAUDE, &calm, "t"), None);
    }

    #[test]
    fn the_worst_of_its_components_shows_with_the_incident_naming_one() {
        let incidents = json!([
            {"name": "Login trouble on claude.ai", "impact": "major", "components": [{"name": "claude.ai"}]},
            {"name": "Elevated API errors", "impact": "minor", "components": [{"name": "Claude API (api.anthropic.com)"}]},
        ]);
        let shown = service(
            &CLAUDE,
            &summary("degraded_performance", "partial_outage", incidents),
            "2026-11-03T10:00:00Z",
        )
        .unwrap();
        assert_eq!(shown["status"], "partial_outage");
        assert_eq!(
            shown["components"],
            json!(["Claude API (api.anthropic.com)"])
        );
        assert_eq!(shown["incident"], "Elevated API errors");
        assert_eq!(shown["page"], "https://status.claude.com");
        assert_eq!(shown["checkedAt"], "2026-11-03T10:00:00Z");
    }

    #[test]
    fn an_unknown_status_word_is_not_an_outage() {
        let odd = summary("something_new", "operational", json!([]));
        assert_eq!(service(&CLAUDE, &odd, "t"), None);
    }
}
