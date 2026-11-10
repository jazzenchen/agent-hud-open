use super::*;
use super::{
    addresses::endpoints,
    events::{next_frame, Told},
    peers::peer_version,
};
use crate::{credentials::SecretStore, http::Parse};
use std::{
    io::{Read, Write},
    time::Instant,
};

#[derive(Clone, PartialEq, Serialize)]
struct Item(String);
impl Record for Item {
    fn id(&self) -> String {
        self.0.clone()
    }
    fn device_id(&self) -> &str {
        "fixture"
    }
    fn is_visible(&self, _: i64) -> bool {
        true
    }
    fn modified_at_ms(&self) -> i64 {
        0
    }
    fn same_content(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Debug)]
struct NoCertificate;
impl rustls::server::ResolvesServerCert for NoCertificate {
    fn resolve(
        &self,
        _: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        panic!("address projection must not open TLS")
    }
}
#[derive(Default)]
struct Secrets(Mutex<BTreeMap<String, Vec<u8>>>);
impl SecretStore for Secrets {
    fn read(&self, _: &str, account: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    fn write(&self, _: &str, account: &str, bytes: &[u8]) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_owned(), bytes.to_vec());
        Ok(())
    }
}

#[test]
fn connection_projection_follows_address_and_account_without_stale_cache() {
    let directory = std::env::temp_dir().join(format!("lan-address-{}", uuid::Uuid::new_v4()));
    let credentials =
        Credentials::new("fixture", directory.clone(), Arc::new(Secrets::default())).unwrap();
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_cert_resolver(Arc::new(NoCertificate));
    let identity = Identity {
        certificate: vec![],
        fingerprint: "fixture-certificate".into(),
        tls: Arc::new(tls),
    };
    let server = LanServer::new(
        "fixture".into(),
        "Fixture".into(),
        credentials,
        Arc::new(Mutex::new(
            SnapshotCache::<Item>::new(
                "fixture".into(),
                "Fixture".into(),
                directory.join("cursor.json"),
            )
            .unwrap(),
        )),
        identity,
        Arc::new(|_, _| Ok(None)),
    );
    assert!(server.set_account(Some("first-account")).unwrap().is_none());
    assert!(server.set_endpoints(Some("https://192.0.2.1:1234".into()), vec![]));
    let first = server.local_connection().unwrap();
    assert!(first.get("alternateEndpoints").is_none());
    assert!(!server.set_endpoints(Some("https://192.0.2.1:1234".into()), vec![]));
    assert!(server.set_endpoints(Some("https://192.0.2.2:1234".into()), vec![]));
    let connection = server.local_connection().unwrap();
    assert_eq!(connection["endpoint"], "https://192.0.2.2:1234");
    assert_eq!(connection["token"], first["token"]);
    // An overlay address coming or going is a change of its own.
    let tailnet = vec!["https://100.69.88.16:1234".to_string()];
    assert!(server.set_endpoints(Some("https://192.0.2.2:1234".into()), tailnet.clone()));
    assert!(!server.set_endpoints(Some("https://192.0.2.2:1234".into()), tailnet.clone()));
    assert_eq!(
        server.local_connection().unwrap()["alternateEndpoints"],
        json!(["https://100.69.88.16:1234"])
    );
    assert_eq!(
        server.status()["alternateEndpoints"],
        json!(["https://100.69.88.16:1234"])
    );
    assert!(server.set_endpoints(Some("https://192.0.2.2:1234".into()), vec![]));
    assert!(server
        .local_connection()
        .unwrap()
        .get("alternateEndpoints")
        .is_none());
    assert!(server.set_endpoints(None, vec![]));
    assert!(server.local_connection().is_none());
    assert!(server.set_endpoints(Some("https://192.0.2.3:1234".into()), vec![]));
    assert_eq!(server.local_connection().unwrap()["token"], first["token"]);
    server.set_account(None).unwrap();
    assert!(server.local_connection().is_none());
    let other = server.set_account(Some("second-account")).unwrap().unwrap();
    assert_ne!(other["token"], first["token"]);
    assert_eq!(other["endpoint"], "https://192.0.2.3:1234");
}

#[test]
fn the_lan_address_leads_and_overlay_addresses_follow_once() {
    let lan = vec!["192.168.1.19".to_string(), "192.168.2.4".to_string()];
    let overlay = vec!["100.69.88.16".to_string()];
    assert_eq!(
        endpoints(&lan, &overlay, 52724),
        (
            Some("https://192.168.1.19:52724".into()),
            vec!["https://100.69.88.16:52724".into()]
        )
    );
    // Without a LAN address the overlay address leads; Windows lists it in both.
    assert_eq!(
        endpoints(&[], &overlay, 52724),
        (Some("https://100.69.88.16:52724".into()), vec![])
    );
    assert_eq!(
        endpoints(&overlay, &overlay, 52724),
        (Some("https://100.69.88.16:52724".into()), vec![])
    );
    assert_eq!(endpoints(&lan, &[], 52724).1, Vec::<String>::new());
    assert_eq!(endpoints(&[], &[], 52724), (None, vec![]));
}

#[test]
fn an_event_stream_carries_changes_from_where_it_stands() {
    let folder = std::env::temp_dir().join(format!("lan-stream-{}", std::process::id()));
    crate::fs::private_dir(&folder).unwrap();
    let mut cache =
        SnapshotCache::<Item>::new("fixture".into(), "Mac".into(), folder.join("cursor.json"))
            .unwrap();
    let text = |frame: Option<Vec<u8>>| frame.map(|f| String::from_utf8(f).unwrap());
    let signal = cache.signal();
    let before = signal.current();
    cache.update(&[Item("a".into())], 1_000).unwrap();
    assert!(signal.current() > before, "a commit wakes the streams");
    let start = cache.head().unwrap().cursor.clone();

    // A stream told this cursor gets nothing until the next commit, and then its change set.
    let mut told = Told {
        revision: String::new(),
        cursor: Some(start.clone()),
    };
    let first = text(next_frame(&cache, true, &mut told)).unwrap();
    assert!(first.starts_with("event: revision\n"), "{first}");
    assert!(next_frame(&cache, true, &mut told).is_none());
    cache
        .update(&[Item("a".into()), Item("b".into())], 2_000)
        .unwrap();
    let head = cache.head().unwrap().cursor.clone();
    let frame = text(next_frame(&cache, true, &mut told)).unwrap();
    let (header, data) = frame.trim_end().split_once("\ndata: ").unwrap();
    assert_eq!(
        header,
        format!("id: {}\nevent: changes", head.query_value())
    );
    let range: Value = serde_json::from_str(data).unwrap();
    assert_eq!(range["fromCursor"], json!(start));
    assert_eq!(range["toCursor"], json!(head));
    assert_eq!(range["records"], json!(["b"]));
    assert_eq!(told.cursor.as_ref(), Some(&head));

    // A stream that does not take changes keeps getting the revision notice.
    let mut old = Told {
        revision: String::new(),
        cursor: Some(start.clone()),
    };
    assert!(text(next_frame(&cache, false, &mut old))
        .unwrap()
        .starts_with("event: revision\n"));

    // A cursor the cache cannot go on from asks the phone for a whole reading.
    let mut lost = Told {
        revision: String::new(),
        cursor: crate::Cursor::new(500, 3),
    };
    assert!(text(next_frame(&cache, true, &mut lost))
        .unwrap()
        .starts_with("event: reset\n"));
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn an_event_stream_may_ask_from_a_cursor() {
    let ask = |head: &str| {
        let Parse::Request(request) = Request::parse(head.as_bytes()) else {
            panic!("{head}")
        };
        request
    };
    let request = ask("GET /v1/events?since=2026-10-25T00:00:00.000Z,4 HTTP/1.1\r\nX-AgentHUD-Stream: changes\r\n\r\n");
    assert!(request.streams_changes());
    assert_eq!(request.cursor().unwrap().change_sequence, 4);
    assert!(!ask("GET /v1/events HTTP/1.1\r\n\r\n").streams_changes());
    assert!(matches!(
        Request::parse(b"GET /v1/snapshot?since=x HTTP/1.1\r\n\r\n"),
        Parse::Invalid
    ));
}

/// Accepts the fixture's throwaway certificate: the test is about the stream, not the pin.
#[derive(Debug)]
struct AnyCertificate;
impl rustls::client::danger::ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ECDSA_NISTP256_SHA256]
    }
}

#[test]
fn a_commit_reaches_an_open_stream_at_once() {
    use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
    let directory = std::env::temp_dir().join(format!("lan-live-{}", uuid::Uuid::new_v4()));
    let credentials =
        Credentials::new("fixture", directory.clone(), Arc::new(Secrets::default())).unwrap();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(
        &ECDSA_P256_SHA256_ASN1_SIGNING,
        &ring::rand::SystemRandom::new(),
    )
    .unwrap();
    let key = rustls::crypto::ring::sign::any_ecdsa_type(&rustls::pki_types::PrivateKeyDer::Pkcs8(
        pkcs8.as_ref().to_vec().into(),
    ))
    .unwrap();
    let identity = Identity::from_signer(vec![0x30, 0x00], key).unwrap();
    let cache = Arc::new(Mutex::new(
        SnapshotCache::<Item>::new(
            "fixture".into(),
            "Fixture".into(),
            directory.join("cursor.json"),
        )
        .unwrap(),
    ));
    cache
        .lock()
        .unwrap()
        .update(&[Item("a".into())], 1_000)
        .unwrap();
    let since = cache.lock().unwrap().head().unwrap().cursor.clone();
    let mut server = LanServer::new(
        "fixture".into(),
        "Fixture".into(),
        credentials,
        cache.clone(),
        identity,
        Arc::new(|_, _| Ok(None)),
    );
    let port = server
        .start_at(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
        .unwrap();
    server.set_account(Some("account")).unwrap();
    let token = server
        .shared
        .credentials
        .lock()
        .unwrap()
        .automatic_credential()
        .unwrap()
        .to_owned();

    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(AnyCertificate))
    .with_no_client_auth();
    let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let session =
        rustls::ClientConnection::new(Arc::new(client), "localhost".try_into().unwrap()).unwrap();
    let mut stream = rustls::StreamOwned::new(session, socket);
    write!(
        stream,
        "GET /v1/events?since={} HTTP/1.1\r\nHost: fixture\r\nAuthorization: Bearer {token}\r\nX-AgentHUD-Stream: changes\r\n\r\n",
        since.query_value()
    )
    .unwrap();
    stream.flush().unwrap();
    let mut received = String::new();
    let mut next_event = |stream: &mut rustls::StreamOwned<_, _>| -> String {
        loop {
            if let Some(end) = received.find("\n\n") {
                let event: String = received.drain(..end + 2).collect();
                if event.starts_with("HTTP/1.1") {
                    continue;
                }
                return event;
            }
            let mut chunk = [0u8; 4096];
            let size = stream.read(&mut chunk).expect("a frame within 3 s");
            received.push_str(std::str::from_utf8(&chunk[..size]).unwrap());
            if let Some(start) = received.find("\r\n\r\n") {
                received.drain(..start + 4);
            }
        }
    };
    // Connected at the head: the revision, as a heartbeat would say.
    assert!(next_event(&mut stream).starts_with("event: revision\n"));

    let committed = Instant::now();
    cache
        .lock()
        .unwrap()
        .update(&[Item("a".into()), Item("b".into())], 2_000)
        .unwrap();
    let event = next_event(&mut stream);
    assert!(
        committed.elapsed() < Duration::from_millis(500),
        "waited {:?}",
        committed.elapsed()
    );
    assert!(event.contains("\nevent: changes\n"), "{event}");
    assert!(event.contains("\"records\":[\"b\"]"), "{event}");
    server.stop(false).unwrap();
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn peer_version_is_trimmed_and_bounded() {
    let request = |value: &str| Request {
        method: "GET".into(),
        path: "/".into(),
        query: None,
        headers: BTreeMap::from([("x-agenthud-client-version".into(), value.into())]),
        body: Vec::new(),
    };
    assert_eq!(
        peer_version(&request(" 1.7.4 (96) ")).as_deref(),
        Some("1.7.4 (96)")
    );
    assert_eq!(peer_version(&request("   ")), None);
    assert_eq!(peer_version(&request("1.7\n(96)")), None);
    assert_eq!(peer_version(&request(&"9".repeat(41))), None);
    assert_eq!(
        peer_version(&request(&"9".repeat(40))).map(|v| v.len()),
        Some(40)
    );
}

#[test]
fn a_host_route_may_answer_with_json() {
    use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
    let directory = std::env::temp_dir().join(format!("lan-route-{}", uuid::Uuid::new_v4()));
    let credentials =
        Credentials::new("fixture", directory.clone(), Arc::new(Secrets::default())).unwrap();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(
        &ECDSA_P256_SHA256_ASN1_SIGNING,
        &ring::rand::SystemRandom::new(),
    )
    .unwrap();
    let key = rustls::crypto::ring::sign::any_ecdsa_type(&rustls::pki_types::PrivateKeyDer::Pkcs8(
        pkcs8.as_ref().to_vec().into(),
    ))
    .unwrap();
    let identity = Identity::from_signer(vec![0x30, 0x00], key).unwrap();
    let cache = Arc::new(Mutex::new(
        SnapshotCache::<Item>::new(
            "fixture".into(),
            "Fixture".into(),
            directory.join("cursor.json"),
        )
        .unwrap(),
    ));
    let mut server = LanServer::new(
        "fixture".into(),
        "Fixture".into(),
        credentials,
        cache,
        identity,
        Arc::new(|request, _| {
            Ok((request.path == "/v1/echo").then(|| Handled {
                status: 200,
                remember: false,
                body: Some(serde_json::json!({"echo": String::from_utf8_lossy(&request.body)})),
            }))
        }),
    );
    let port = server
        .start_at(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
        .unwrap();
    server.set_account(Some("account")).unwrap();
    let token = server
        .shared
        .credentials
        .lock()
        .unwrap()
        .automatic_credential()
        .unwrap()
        .to_owned();
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(AnyCertificate))
    .with_no_client_auth();
    let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let session =
        rustls::ClientConnection::new(Arc::new(client), "localhost".try_into().unwrap()).unwrap();
    let mut stream = rustls::StreamOwned::new(session, socket);
    write!(
        stream,
        "POST /v1/echo HTTP/1.1\r\nHost: fixture\r\nAuthorization: Bearer {token}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi"
    )
    .unwrap();
    stream.flush().unwrap();
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let response = String::from_utf8_lossy(&response);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains("content-type: application/json"),
        "{response}"
    );
    assert!(response.ends_with("{\"echo\":\"hi\"}"), "{response}");
    server.stop(false).unwrap();
    let _ = std::fs::remove_dir_all(&directory);
}
