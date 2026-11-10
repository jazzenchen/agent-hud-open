#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The open-source build has no product extension to name the app's own
    // executable, so client hooks (and the DeepSeek Harness plugin's asks
    // channel) would never be delegated a listener. Name it here.
    let mut options = agent_hud_host::HostOptions::default();
    options.canonical_executable = std::env::current_exe().ok();
    agent_hud_host::run_with_options(tauri::generate_context!(), None, options);
}
