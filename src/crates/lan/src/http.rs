use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
#[derive(Debug)]
pub enum Parse {
    Incomplete,
    Invalid,
    Request(Request),
}
/// The most a request may hold, headers and body: a phone's message of up to a megabyte, the words of a long recording
/// sent to start a session (a quarter hour of talk is some 12 KB; 64 KB refused an hour of it).
pub const MAX_REQUEST: usize = 1024 * 1024 + 16 * 1024;

impl Request {
    pub fn parse(bytes: &[u8]) -> Parse {
        if bytes.len() > MAX_REQUEST {
            return Parse::Invalid;
        }
        let Some(boundary) = bytes.windows(4).position(|s| s == b"\r\n\r\n") else {
            return Parse::Incomplete;
        };
        let Ok(head) = std::str::from_utf8(&bytes[..boundary]) else {
            return Parse::Invalid;
        };
        let mut lines = head.split("\r\n");
        let first = lines
            .next()
            .unwrap_or("")
            .split_ascii_whitespace()
            .collect::<Vec<_>>();
        if first.len() != 3
            || first[2] != "HTTP/1.1"
            || !first[1].starts_with('/')
            || first[1].starts_with("//")
            || first[1].contains('#')
        {
            return Parse::Invalid;
        }
        let (path, query) = first[1]
            .split_once('?')
            .map(|(p, q)| (p, Some(q)))
            .unwrap_or((first[1], None));
        if path.contains('%') {
            return Parse::Invalid;
        }
        if query.is_some_and(|query| {
            !matches!(path, "/v1/changes" | "/v1/events")
                || !query.starts_with("since=")
                || query.contains('&')
        }) {
            return Parse::Invalid;
        }
        let mut headers = BTreeMap::new();
        for line in lines {
            let Some((key, value)) = line.split_once(':') else {
                return Parse::Invalid;
            };
            let key = key.to_ascii_lowercase();
            if key.is_empty()
                || key.bytes().any(|b| !b.is_ascii_alphanumeric() && b != b'-')
                || headers.insert(key, value.trim().into()).is_some()
            {
                return Parse::Invalid;
            }
        }
        if headers.contains_key("transfer-encoding") {
            return Parse::Invalid;
        }
        let length = match headers
            .get("content-length")
            .map(|v: &String| v.parse::<usize>())
        {
            None => 0,
            Some(Ok(size)) if size <= 1024 * 1024 => size,
            _ => return Parse::Invalid,
        };
        let body = &bytes[boundary + 4..];
        if body.len() < length {
            return Parse::Incomplete;
        }
        if body.len() != length {
            return Parse::Invalid;
        }
        Parse::Request(Self {
            method: first[0].into(),
            path: path.into(),
            query: query.map(str::to_owned),
            headers,
            body: body.into(),
        })
    }
    pub fn bearer(&self) -> Option<&str> {
        self.headers
            .get("authorization")?
            .strip_prefix("Bearer ")
            .filter(|token| credentials::valid_token(token))
    }
    pub fn cursor(&self) -> Option<crate::Cursor> {
        self.query
            .as_deref()?
            .strip_prefix("since=")
            .and_then(decode_query)
            .and_then(|value| crate::Cursor::parse(&value))
    }
    /// Whether an event stream's client takes the changes themselves
    /// (`X-AgentHUD-Stream: changes`). Others get revision notices only.
    pub fn streams_changes(&self) -> bool {
        self.headers
            .get("x-agenthud-stream")
            .is_some_and(|value| value == "changes")
    }
}
/// Bodies under this size gain too little from compression to be worth it.
const COMPRESS_FROM: usize = 1024;

impl Request {
    /// Whether the client accepts a gzip body (`gzip` or `*` without `q=0`).
    pub fn accepts_gzip(&self) -> bool {
        self.headers.get("accept-encoding").is_some_and(|value| {
            value.split(',').any(|item| {
                let mut parts = item.split(';').map(str::trim);
                let coding = parts.next().unwrap_or_default().to_ascii_lowercase();
                let refused = parts.any(|p| {
                    p.strip_prefix("q=")
                        .and_then(|q| q.trim().parse::<f32>().ok())
                        .is_some_and(|q| q <= 0.0)
                });
                (coding == "gzip" || coding == "*") && !refused
            })
        })
    }
}

/// One rule for every LAN reply body, whatever path the client came by (LAN or
/// Tailscale): gzip when the client accepts it and the body is large enough.
/// Returns the body to send and the headers that describe it. The ETag and the
/// revision stay those of the uncompressed content.
pub fn encode_body(request: &Request, body: Vec<u8>) -> (Vec<u8>, Vec<(&'static str, String)>) {
    use std::io::Write;
    if body.len() < COMPRESS_FROM || !request.accepts_gzip() {
        return (body, vec![("Vary", "Accept-Encoding".into())]);
    }
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(body.len() / 4),
        flate2::Compression::default(),
    );
    match encoder.write_all(&body).and_then(|_| encoder.finish()) {
        Ok(compressed) if compressed.len() < body.len() => (
            compressed,
            vec![
                ("Content-Encoding", "gzip".into()),
                ("Vary", "Accept-Encoding".into()),
            ],
        ),
        _ => (body, vec![("Vary", "Accept-Encoding".into())]),
    }
}

pub fn response(status: u16, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        410 => "Gone",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let mut fields = BTreeMap::from([
        ("Content-Length", body.len().to_string()),
        ("Connection", "close".into()),
        ("Cache-Control", "no-store".into()),
    ]);
    for (key, value) in headers {
        fields.insert(key, value.clone());
    }
    let mut output = format!("HTTP/1.1 {status} {reason}\r\n");
    for (key, value) in fields {
        output.push_str(&format!("{key}: {value}\r\n"));
    }
    output.push_str("\r\n");
    let mut bytes = output.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn decode_query(value: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            if at + 2 >= bytes.len() {
                return None;
            }
            let code = std::str::from_utf8(&bytes[at + 1..at + 3]).ok()?;
            out.push(u8::from_str_radix(code, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod encoding_tests {
    use super::*;
    use std::io::Read;

    fn request(accept: Option<&str>) -> Request {
        let mut headers = BTreeMap::new();
        if let Some(value) = accept {
            headers.insert("accept-encoding".to_string(), value.to_string());
        }
        Request {
            method: "GET".into(),
            path: "/v1/snapshot".into(),
            query: None,
            headers,
            body: Vec::new(),
        }
    }

    #[test]
    fn gzip_is_accepted_only_when_offered() {
        assert!(request(Some("gzip, deflate, br")).accepts_gzip());
        assert!(request(Some("br;q=1.0, gzip;q=0.8")).accepts_gzip());
        assert!(request(Some("*")).accepts_gzip());
        assert!(!request(Some("gzip;q=0")).accepts_gzip());
        assert!(!request(Some("br, deflate")).accepts_gzip());
        assert!(!request(None).accepts_gzip());
    }

    #[test]
    fn large_bodies_are_gzipped_for_clients_that_accept_it() {
        let body = serde_json::to_vec(&vec!["session turn text"; 400]).unwrap();
        let (sent, headers) = encode_body(&request(Some("gzip, deflate, br")), body.clone());
        assert!(headers.contains(&("Content-Encoding", "gzip".to_string())));
        assert!(sent.len() < body.len() / 4);
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(&sent[..])
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, body);
        // No offer, or a small body: sent as it is.
        let (plain, headers) = encode_body(&request(None), body.clone());
        assert_eq!(plain, body);
        assert!(!headers.iter().any(|(k, _)| *k == "Content-Encoding"));
        let small = b"{\"records\":[]}".to_vec();
        assert_eq!(encode_body(&request(Some("gzip")), small.clone()).0, small);
    }
}
