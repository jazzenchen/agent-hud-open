//! Which client drives a session (Open's SessionSource and the vendor
//! catalog's client names): Codex Desktop, CLI or IDE extension; Claude Code
//! CLI, Desktop or IDE extension, or the Agent SDK. A name the catalog does
//! not know is shown as written.

/// The Codex client a rollout's `session_meta` names. A CLI launched from
/// Desktop can inherit its originator, so the rollout's `source` decides;
/// otherwise the originator names the client.
pub fn codex_client(source: Option<&str>, originator: Option<&str>) -> Option<String> {
    match source {
        Some("cli") => return Some("CLI".into()),
        Some("exec") => return Some("CLI · exec".into()),
        _ => {}
    }
    if let Some(origin) = originator.filter(|origin| !origin.is_empty()) {
        let named = match origin {
            "Codex Desktop" | "codex_work_desktop" | "codex_desktop" => "Desktop",
            "codex_vscode" => "IDE",
            "codex-tui" | "codex_cli_rs" | "codex-cli" => "CLI",
            "codex_exec" => "CLI · exec",
            other => other,
        };
        return Some(named.into());
    }
    (source == Some("vscode")).then(|| "IDE".into())
}

/// The name a session's client is shown by, for `source` (the app's provider
/// id) and the client its log names; none where the app has nothing to add to
/// the agent's own name.
pub fn client_name(source: &str, client: Option<&str>) -> Option<String> {
    match source {
        "codex" => Some(
            match client {
                Some("Desktop") => "Codex Desktop",
                Some("CLI") | Some("CLI · exec") => "Codex CLI",
                Some("IDE") => "Codex IDE extension",
                None | Some("Codex") => "Codex",
                Some(other) => other,
            }
            .into(),
        ),
        "claude" => Some(match client {
            Some("cli") => "Claude Code CLI".into(),
            Some("claude-desktop") => "Claude Code Desktop".into(),
            Some("claude-vscode") => "Claude Code IDE extension".into(),
            Some(other) if other.starts_with("sdk-") => "Claude Agent SDK".into(),
            Some(other) if other != "Claude Code" => other.into(),
            _ => "Claude Code".into(),
        }),
        "antigravity" => match client {
            Some("Desktop") => Some("Antigravity Desktop".into()),
            Some("CLI") | Some("CLI · exec") => Some("Antigravity CLI".into()),
            Some("IDE") => Some("Antigravity IDE".into()),
            None | Some("Antigravity") => None,
            Some(other) => Some(other.into()),
        },
        _ => None,
    }
}

/// Whether a Codex session's shown client is another app-server client
/// (a wrapper's own originator, such as `codex_ask`), not Codex Desktop, the
/// CLI or the IDE extension. Codex keeps those threads in its own list, so
/// they open in the Codex app; Codex Desktop does not hold them.
pub fn codex_other_client(client: Option<&str>) -> bool {
    client.is_some_and(|client| {
        !client.is_empty()
            && !matches!(
                client,
                "Codex Desktop" | "Codex CLI" | "Codex IDE extension" | "Codex"
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_are_named_as_open_names_them() {
        assert_eq!(
            codex_client(Some("cli"), Some("Codex Desktop")).as_deref(),
            Some("CLI")
        );
        assert_eq!(
            codex_client(Some("vscode"), Some("codex_work_desktop")).as_deref(),
            Some("Desktop")
        );
        assert_eq!(codex_client(Some("vscode"), None).as_deref(), Some("IDE"));
        assert_eq!(
            codex_client(None, Some("codex_exec")).as_deref(),
            Some("CLI · exec")
        );
        assert_eq!(
            codex_client(None, Some("my_wrapper")).as_deref(),
            Some("my_wrapper")
        );
        assert_eq!(
            client_name("codex", Some("Desktop")).as_deref(),
            Some("Codex Desktop")
        );
        assert_eq!(
            client_name("codex", Some("CLI · exec")).as_deref(),
            Some("Codex CLI")
        );
        assert_eq!(client_name("codex", None).as_deref(), Some("Codex"));
        assert_eq!(
            client_name("claude", Some("claude-vscode")).as_deref(),
            Some("Claude Code IDE extension")
        );
        assert_eq!(
            client_name("claude", Some("sdk-ts")).as_deref(),
            Some("Claude Agent SDK")
        );
        assert_eq!(client_name("claude", None).as_deref(), Some("Claude Code"));
        assert_eq!(client_name("deepseek", None), None);
    }

    #[test]
    fn other_codex_clients_are_the_ones_codex_does_not_name() {
        assert!(codex_other_client(Some("codex_ask")));
        for own in [
            "Codex Desktop",
            "Codex CLI",
            "Codex IDE extension",
            "Codex",
            "",
        ] {
            assert!(!codex_other_client(Some(own)), "{own}");
        }
        assert!(!codex_other_client(None));
    }
}
