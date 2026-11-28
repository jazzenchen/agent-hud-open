//! Builds shell-escaped terminal launch commands for isolated provider homes.
use super::{homes, AccountContext};
use agent_hud_agents::Os;
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn build(context: &AccountContext, params: &Value) -> Result<Value, String> {
    let provider = params["provider"].as_str().ok_or("Provider is required")?;
    let cli = cli_name(provider)?;
    let home_dir = match params["home"]
        .as_str()
        .or_else(|| params["path"].as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(raw) => {
            let resolved = homes::paths(&context.home, &json!([raw]))?
                .into_iter()
                .next()
                .ok_or("Account folder is required")?;
            if provider == "antigravity" {
                Some(homes::sandbox_home(&context.data_dir, &resolved)?)
            } else {
                Some(resolved)
            }
        }
        None => None,
    };
    let executable = resolve_command(context, cli);
    let os = crate::os::OS;
    let command = format_command(os, provider, home_dir.as_deref(), &executable)?;
    Ok(json!({
        "provider": provider,
        "command": command,
        "shell": if os == Os::Windows { "powershell" } else { "sh" }
    }))
}

fn cli_name(provider: &str) -> Result<&'static str, String> {
    match provider {
        "antigravity" => Ok("agy"),
        "claude" => Ok("claude"),
        "codex" => Ok("codex"),
        _ => Err(format!("Unsupported managed account provider: {provider}")),
    }
}

fn resolve_command(context: &AccountContext, cli: &str) -> String {
    let on_path = crate::os::path_entries(&context.env)
        .into_iter()
        .any(|dir| {
            let direct = dir.join(cli);
            if crate::os::engine_executable(&direct) {
                return true;
            }
            crate::os::OS == Os::Windows
                && ["cmd", "exe", "bat"]
                    .into_iter()
                    .any(|ext| crate::os::engine_executable(&dir.join(format!("{cli}.{ext}"))))
        });
    if on_path {
        return cli.to_owned();
    }
    crate::os::find_engine(&context.home, &context.env, cli)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| cli.to_owned())
}

pub(super) fn format_command(
    os: Os,
    provider: &str,
    home_dir: Option<&Path>,
    executable: &str,
) -> Result<String, String> {
    let _ = cli_name(provider)?;
    if os == Os::Windows {
        return Ok(format_powershell(provider, home_dir, executable));
    }
    Ok(format_posix(provider, home_dir, executable))
}

fn format_posix(provider: &str, home_dir: Option<&Path>, executable: &str) -> String {
    let cmd = quote_posix_token(executable);
    let Some(dir) = home_dir else {
        return cmd;
    };
    let dir_str = quote_posix(&dir.to_string_lossy());
    match provider {
        "antigravity" => format!(
            "HOME={dir_str} AGY_ACP_FORCE_FILE_STORAGE=1 AGY_CLI_FORCE_FILE_TOKEN_STORAGE=1 {cmd}"
        ),
        "claude" => format!("CLAUDE_CONFIG_DIR={dir_str} {cmd}"),
        "codex" => format!("CODEX_HOME={dir_str} {cmd}"),
        _ => cmd,
    }
}

fn format_powershell(provider: &str, home_dir: Option<&Path>, executable: &str) -> String {
    let cmd = if is_bare_command(executable) {
        executable.to_owned()
    } else {
        format!("& {}", quote_powershell(executable))
    };
    let Some(dir) = home_dir else {
        return cmd;
    };
    let dir_str = quote_powershell(&dir.to_string_lossy());
    match provider {
        "antigravity" => format!(
            "$env:HOME={dir_str}; $env:USERPROFILE={dir_str}; $env:AGY_ACP_FORCE_FILE_STORAGE='1'; $env:AGY_CLI_FORCE_FILE_TOKEN_STORAGE='1'; {cmd}"
        ),
        "claude" => format!("$env:CLAUDE_CONFIG_DIR={dir_str}; {cmd}"),
        "codex" => format!("$env:CODEX_HOME={dir_str}; {cmd}"),
        _ => cmd,
    }
}

fn is_bare_command(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn quote_posix_token(value: &str) -> String {
    if is_bare_command(value) {
        value.to_owned()
    } else {
        quote_posix(value)
    }
}

fn quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_and_powershell_commands_escape_paths_and_inject_isolation_env() {
        let path = std::env::temp_dir().join("o'hara/Agent HUD/profile 1");
        let escaped = path.to_string_lossy().replace('\'', "'\\''");
        assert_eq!(
            format_command(Os::MacOs, "antigravity", Some(&path), "agy").unwrap(),
            format!("HOME='{escaped}' AGY_ACP_FORCE_FILE_STORAGE=1 AGY_CLI_FORCE_FILE_TOKEN_STORAGE=1 agy")
        );
        assert_eq!(
            format_command(
                Os::Linux,
                "claude",
                Some(&path),
                "/opt/Claude Code/bin/claude"
            )
            .unwrap(),
            format!("CLAUDE_CONFIG_DIR='{escaped}' '/opt/Claude Code/bin/claude'")
        );
        assert_eq!(
            format_command(Os::MacOs, "codex", None, "codex").unwrap(),
            "codex"
        );

        let win_path = Path::new(r"C:\Users\o'hara\AppData\Roaming\Agent HUD\profile 1");
        assert_eq!(
            format_command(
                Os::Windows,
                "antigravity",
                Some(win_path),
                r"C:\Program Files\Antigravity\agy.cmd"
            )
            .unwrap(),
            r"$env:HOME='C:\Users\o''hara\AppData\Roaming\Agent HUD\profile 1'; $env:USERPROFILE='C:\Users\o''hara\AppData\Roaming\Agent HUD\profile 1'; $env:AGY_ACP_FORCE_FILE_STORAGE='1'; $env:AGY_CLI_FORCE_FILE_TOKEN_STORAGE='1'; & 'C:\Program Files\Antigravity\agy.cmd'"
        );
        assert_eq!(
            format_command(Os::Windows, "codex", Some(win_path), "codex").unwrap(),
            r"$env:CODEX_HOME='C:\Users\o''hara\AppData\Roaming\Agent HUD\profile 1'; codex"
        );
    }
}
