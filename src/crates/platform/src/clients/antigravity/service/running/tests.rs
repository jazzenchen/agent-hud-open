use super::*;

/// The servers found with no saved token files.
fn parse_servers(processes: Vec<crate::os::ProcessInfo>) -> Vec<Server> {
    super::parse_servers(processes, &HashMap::new())
}

fn process(pid: u32, executable: &str, command: &str) -> crate::os::ProcessInfo {
    crate::os::ProcessInfo {
        pid,
        executable: executable.into(),
        command: command.into(),
    }
}

#[test]
fn discovers_all_authenticated_native_and_cli_servers() {
    let found = parse_servers(vec![
        process(
            1,
            "/Applications/Antigravity.app/language_server",
            "server --csrf_token fixture-1",
        ),
        process(
            2,
            "/u/.gemini/antigravity-cli/language_server",
            "server --csrf_token fixture-2",
        ),
        process(3, "/bin/agy", "agy"),
        process(
            4,
            "/unrelated/language_server",
            "server --csrf_token fixture-4",
        ),
        process(
            5,
            "/Applications/Antigravity IDE.app/language-server",
            "server --csrf_token fixture-5",
        ),
    ]);
    assert_eq!(
        found.iter().map(|server| server.pid).collect::<Vec<_>>(),
        [1, 2, 5]
    );
}

#[test]
fn discovers_windows_language_server_and_cli_with_native_paths() {
    let found = parse_servers(vec![
        process(
            1,
            r"C:\Program Files\Antigravity\language_server.exe",
            "server --app_data_dir antigravity --csrf_token fixture",
        ),
        process(2, r"C:\Tools\agy.exe", "agy.exe --csrf_token fixture"),
        process(
            3,
            r"C:\Other\language_server.exe",
            "server --app_data_dir unrelated --csrf_token fixture",
        ),
        process(
            4,
            r"C:\Program Files\Antigravity IDE\resources\bin\language_server_windows_x64.exe",
            "server --csrf_token fixture-ide",
        ),
    ]);
    assert_eq!(
        found
            .iter()
            .map(|server| (server.pid, server.client))
            .collect::<Vec<_>>(),
        [
            (1, "antigravity"),
            (4, "antigravity-ide"),
            (2, "antigravity-cli")
        ]
    );
}

#[test]
fn tokenless_cli_fallback_uses_the_executable_not_command_arguments() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("installed/agy");
    let executable = executable.to_str().unwrap();
    let found = cli_from(vec![
        process(1, "/bin/sh", "sh /bin/agy"),
        process(2, executable, "agy"),
    ]);
    assert_eq!(found.as_deref(), Some(executable));
    assert!(cli_from(vec![process(1, "agy", "agy")]).is_none());
}

#[test]
fn each_server_names_the_client_it_serves() {
    let found = parse_servers(vec![
        process(
            1,
            "/Applications/Antigravity.app/Contents/Resources/bin/language_server",
            "server --csrf_token fixture-1",
        ),
        process(
            2,
            "/Applications/Antigravity IDE.app/Contents/Resources/bin/language_server",
            "server --csrf_token fixture-2",
        ),
        process(
            3,
            "/u/.gemini/antigravity-cli/language_server",
            "server --csrf_token fixture-3",
        ),
        process(
            4,
            "/opt/language_server",
            "server --app_data_dir antigravity-ide --csrf_token fixture-4",
        ),
    ]);
    let clients: Vec<_> = found
        .iter()
        .map(|server| (server.pid, server.client))
        .collect();
    assert_eq!(
        clients,
        [
            (1, "antigravity"),
            (2, "antigravity-ide"),
            (3, "antigravity-cli"),
            (4, "antigravity-ide"),
        ]
    );
}

#[test]
fn a_server_with_no_token_on_its_command_line_takes_the_one_it_saved() {
    let root = tempfile::tempdir().unwrap();
    let daemon = root.path().join("antigravity/daemon");
    std::fs::create_dir_all(&daemon).unwrap();
    std::fs::write(
        daemon.join("hub_57399.json"),
        r#"{"pid":1,"httpsPort":57398,"httpPort":57399,"lsVersion":"2.34.0","csrfToken":"saved-1"}"#,
    )
    .unwrap();
    // Not a server here, a broken file and one too large: all passed over.
    std::fs::write(
        daemon.join("hub_1.json"),
        r#"{"pid":9,"csrfToken":"saved-9"}"#,
    )
    .unwrap();
    std::fs::write(daemon.join("hub_2.json"), "{").unwrap();
    std::fs::write(
        daemon.join("hub_3.json"),
        format!(r#"{{"pid":3,"csrfToken":"{}"}}"#, "x".repeat(5000)),
    )
    .unwrap();
    let saved = saved_tokens(root.path());
    assert_eq!(saved.len(), 2);
    let server = "/Applications/Antigravity.app/Contents/Resources/bin/language_server";
    let found = super::parse_servers(
        vec![
            process(1, server, "server --app_data_dir antigravity"),
            process(
                2,
                server,
                "server --app_data_dir antigravity --csrf_token typed-2",
            ),
            process(3, server, "server --app_data_dir antigravity"),
        ],
        &saved,
    );
    assert_eq!(
        found
            .iter()
            .map(|server| (server.pid, server.token.as_str()))
            .collect::<Vec<_>>(),
        [(1, "saved-1"), (2, "typed-2")]
    );
}
