//! The public AntigravityNavigation contract: an already-running desktop's
//! loopback CDP inventory and /c/<UUID>. Never launches an application/server.
use agent_hud_core::l10n;
use reqwest::{blocking::Client, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
const LIMIT: usize = 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Page {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    web_socket_debugger_url: Option<String>,
}
pub(super) struct Destination {
    id: String,
    url: Url,
    socket: Url,
}
fn eligible(id: &str, page: &Page, port: u16) -> Option<Destination> {
    if !super::uuid(id) || page.kind != "page" {
        return None;
    }
    let mut url = Url::parse(&page.url).ok()?;
    let socket = Url::parse(page.web_socket_debugger_url.as_deref()?).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("127.0.0.1")
        || socket.scheme() != "ws"
        || socket.host_str() != Some("127.0.0.1")
        || socket.port() != Some(port)
        || socket.path() != format!("/devtools/page/{}", page.id)
    {
        return None;
    }
    // User info must not turn a loopback navigation into a credential-bearing request.
    if !url.username().is_empty()
        || url.password().is_some()
        || !socket.username().is_empty()
        || socket.password().is_some()
    {
        return None;
    }
    url.set_path(&format!("/c/{id}"));
    url.set_query(None);
    url.set_fragment(None);
    Some(Destination {
        id: page.id.clone(),
        url,
        socket,
    })
}
pub(super) fn destination(id: &str, pages: &[Page], port: u16) -> Option<Destination> {
    let mut all = pages
        .iter()
        .filter_map(|page| eligible(id, page, port))
        .collect::<Vec<_>>();
    let existing = all
        .iter()
        .enumerate()
        .filter(|(_, target)| {
            pages
                .iter()
                .find(|p| p.id == target.id)
                .and_then(|p| Url::parse(&p.url).ok())
                .is_some_and(|u| u.path() == target.url.path())
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if existing.len() == 1 {
        return Some(all.swap_remove(existing[0]));
    }
    if existing.is_empty() && all.len() == 1 {
        all.pop()
    } else {
        None
    }
}
fn remaining(deadline: Instant) -> Result<Duration, String> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(l10n::text(
            "打开 Antigravity 会话超时",
            "Antigravity navigation timed out",
        ))
    } else {
        Ok(left)
    }
}
fn inventory(client: &Client, port: u16, deadline: Instant) -> Result<Vec<Page>, String> {
    let response = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .timeout(remaining(deadline)?.min(Duration::from_secs(2)))
        .send()
        .map_err(|_| "Antigravity page inventory is unavailable")?;
    if response.status() != 200 {
        return Err("Antigravity page inventory is unavailable".into());
    }
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err("Antigravity page inventory exceeds its size limit".into());
    }
    let mut bytes = Vec::new();
    response
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Antigravity page inventory is unavailable")?;
    if bytes.len() > LIMIT {
        return Err("Antigravity page inventory exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Antigravity page inventory is invalid".into())
}
pub(super) fn navigate(id: &str, port: u16) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| "Antigravity navigation client is unavailable")?;
    let pages = inventory(&client, port, deadline)?;
    let Some(target) = destination(id, &pages, port) else {
        return Ok(false);
    };
    if pages
        .iter()
        .find(|p| p.id == target.id)
        .and_then(|p| Url::parse(&p.url).ok())
        .is_some_and(|u| u.path() == target.url.path())
    {
        return Ok(true);
    }
    let stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        remaining(deadline)?.min(Duration::from_secs(2)),
    )
    .map_err(|_| "Antigravity navigation channel is unavailable")?;
    stream
        .set_read_timeout(Some(remaining(deadline)?))
        .map_err(|_| "Antigravity navigation channel is unavailable")?;
    stream
        .set_write_timeout(Some(remaining(deadline)?))
        .map_err(|_| "Antigravity navigation channel is unavailable")?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(LIMIT))
        .max_frame_size(Some(LIMIT))
        .read_buffer_size(8192)
        .write_buffer_size(8192);
    let (mut socket, _) =
        tungstenite::client::client_with_config(target.socket.as_str(), stream, Some(config))
            .map_err(|_| "Antigravity navigation handshake failed")?;
    socket
        .send(tungstenite::Message::Text(
            json!({"id":1,"method":"Page.navigate","params":{"url":target.url.as_str()}})
                .to_string()
                .into(),
        ))
        .map_err(|_| "Antigravity navigation request failed")?;
    loop {
        socket
            .get_mut()
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| "Antigravity navigation channel is unavailable")?;
        let message = socket
            .read()
            .map_err(|_| "Antigravity navigation response is unavailable")?;
        if !matches!(
            message,
            tungstenite::Message::Text(_) | tungstenite::Message::Binary(_)
        ) {
            continue;
        }
        let Ok(response) = serde_json::from_slice::<Value>(&message.into_data()) else {
            continue;
        };
        if response["id"] != 1 {
            continue;
        }
        if !response["result"]["frameId"].is_string()
            || response["result"].get("errorText").is_some()
        {
            return Ok(false);
        }
        break;
    }
    drop(socket);
    loop {
        let current = inventory(&client, port, deadline)?;
        if current
            .iter()
            .find(|page| page.id == target.id)
            .and_then(|page| Url::parse(&page.url).ok())
            .is_some_and(|url| {
                url.scheme() == target.url.scheme()
                    && url.host_str() == target.url.host_str()
                    && url.port() == target.url.port()
                    && url.path() == target.url.path()
            })
        {
            return Ok(true);
        }
        std::thread::sleep(remaining(deadline)?.min(Duration::from_millis(100)));
    }
}
pub(super) fn devtools_port(home: &Path) -> Option<u16> {
    let path = crate::os::app_support(home, &std::env::vars().collect())
        .join("Antigravity/DevToolsActivePort");
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    std::str::from_utf8(&bytes)
        .ok()?
        .lines()
        .next()?
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "12345678-1234-1234-1234-123456789abc";
    fn page(url: &str, socket: &str) -> Page {
        serde_json::from_value(json!({
            "id": "page1", "type": "page", "url": url,
            "webSocketDebuggerUrl": socket
        }))
        .unwrap()
    }

    #[test]
    fn desktop_destination_accepts_only_the_matching_loopback_page() {
        let trusted = page(
            "https://127.0.0.1:4567/",
            "ws://127.0.0.1:1234/devtools/page/page1",
        );
        let target = destination(ID, &[trusted], 1234).unwrap();
        assert_eq!(target.url.path(), format!("/c/{ID}"));
        for (url, socket) in [
            (
                "https://example.com/",
                "ws://127.0.0.1:1234/devtools/page/page1",
            ),
            (
                "https://127.0.0.1/",
                "ws://127.0.0.1:1235/devtools/page/page1",
            ),
            (
                "https://user@127.0.0.1/",
                "ws://127.0.0.1:1234/devtools/page/page1",
            ),
            (
                "https://127.0.0.1/",
                "ws://127.0.0.1:1234/devtools/page/other",
            ),
        ] {
            assert!(destination(ID, &[page(url, socket)], 1234).is_none());
        }
        assert!(destination(
            "not-a-uuid",
            &[page(
                "https://127.0.0.1/",
                "ws://127.0.0.1:1234/devtools/page/page1"
            )],
            1234
        )
        .is_none());
    }

    #[test]
    fn desktop_destination_refuses_ambiguous_pages() {
        let first = page(
            "https://127.0.0.1/",
            "ws://127.0.0.1:1234/devtools/page/page1",
        );
        let second: Page = serde_json::from_value(json!({
            "id": "page2", "type": "page", "url": "https://127.0.0.1/",
            "webSocketDebuggerUrl": "ws://127.0.0.1:1234/devtools/page/page2"
        }))
        .unwrap();
        assert!(destination(ID, &[first, second], 1234).is_none());
    }
}
