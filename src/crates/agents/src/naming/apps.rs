//! The app a prompt was typed in, by the bundle id of its process or the
//! terminal program that ran it, as a surface's source.
use super::surfaces::AGENT_HUD;

/// The app a bundle identifier names, its helpers' identifiers too
/// (`com.microsoft.VSCode.helper`).
pub fn bundle_app(bundle: &str) -> Option<&'static str> {
    const APPS: &[(&str, &str)] = &[
        ("com.apple.Terminal", "terminal"),
        ("com.googlecode.iterm2", "iterm"),
        ("com.mitchellh.ghostty", "ghostty"),
        ("dev.warp.Warp-Stable", "warp"),
        ("dev.warp.Warp-Preview", "warp"),
        ("dev.warp.Warp", "warp"),
        ("com.github.wez.wezterm", "wezterm"),
        ("net.kovidgoyal.kitty", "kitty"),
        ("org.alacritty", "alacritty"),
        ("io.alacritty", "alacritty"),
        ("co.zeit.hyper", "hyper"),
        ("org.tabby", "tabby"),
        ("com.raphaelamorim.rio", "rio"),
        ("com.microsoft.VSCodeInsiders", "vscode"),
        ("com.microsoft.VSCode", "vscode"),
        ("com.vscodium", "vscodium"),
        ("com.todesktop.230313mzl4w4u92", "cursor"),
        // Windsurf renamed in place; the id is Devin Desktop's now.
        (crate::devin::process::DESKTOP_BUNDLE, "devin"),
        ("dev.zed.Zed-Preview", "zed"),
        ("dev.zed.Zed", "zed"),
        ("com.apple.dt.Xcode", "xcode"),
        ("com.anthropic.claudefordesktop", "claude"),
        ("com.openai.codex", "codex"),
        ("com.google.antigravity", "antigravity"),
        (crate::trae::process::BUNDLES[0], "trae"),
        (crate::trae::process::BUNDLES[1], "trae"),
        ("app.agenthud", AGENT_HUD),
        ("com.agenthud", AGENT_HUD),
    ];
    let within = |id: &str| {
        bundle == id
            || bundle
                .strip_prefix(id)
                .is_some_and(|rest| rest.starts_with('.'))
    };
    if within("com.jetbrains") || within("com.google.android.studio") {
        return Some("jetbrains");
    }
    APPS.iter().find(|(id, _)| within(id)).map(|(_, app)| *app)
}

/// The terminal a `TERM_PROGRAM` value names.
pub fn terminal_app(program: &str) -> Option<&'static str> {
    Some(match program {
        "Apple_Terminal" => "terminal",
        "iTerm.app" => "iterm",
        "ghostty" => "ghostty",
        "WarpTerminal" => "warp",
        "WezTerm" => "wezterm",
        "vscode" => "vscode",
        "Hyper" => "hyper",
        "Tabby" => "tabby",
        "zed" => "zed",
        "kitty" => "kitty",
        "rio" => "rio",
        "tmux" => "tmux",
        _ => return None,
    })
}
