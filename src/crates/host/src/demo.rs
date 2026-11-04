//! `--demo` shows agent-hud-native-open's sample through the real Core
//! (`Runtime::demo`); `--demo-script <name|file>` plays a timed scenario in
//! it, a built-in one by name (`showcase`) or a JSON file, and implies
//! `--demo`. The player wakes for each step, plays it under the collection
//! gate, republishes, and tells the HUD when a step opens or closes the panel.
use crate::service::{wake_collector, Service, REQUEST};
use agent_hud_runtime::DemoScript;
use std::{
    sync::{Arc, PoisonError},
    time::Duration,
};
use tauri::Emitter;

/// Whether this launch shows the demo.
pub(crate) fn wanted(arguments: &[String]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == "--demo" || argument == "--demo-script")
}

/// The script `--demo-script` names, if any.
pub(crate) fn script(arguments: &[String]) -> Result<Option<DemoScript>, String> {
    let Some(at) = arguments
        .iter()
        .position(|argument| argument == "--demo-script")
    else {
        return Ok(None);
    };
    let name = arguments
        .get(at + 1)
        .ok_or("--demo-script needs a name or a file")?;
    DemoScript::named(name).map(Some)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Plays the demo's script for as long as the app runs.
pub(crate) fn play(app: tauri::AppHandle, service: &Service) {
    let backend = Arc::downgrade(&service.backend);
    let wake = Arc::clone(&service.wake);
    let collection = Arc::clone(&service.collection);
    let _ = std::thread::Builder::new()
        .name("agenthud-demo".into())
        .spawn(move || loop {
            let next = {
                let Some(backend) = backend.upgrade() else {
                    return;
                };
                let backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
                backend.runtime.demo_next_at()
            };
            let Some(next) = next else { return };
            std::thread::sleep(Duration::from_millis((next - now()).max(0) as u64));
            let panels = {
                let _turn = collection.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(backend) = backend.upgrade() else {
                    return;
                };
                let mut backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
                match backend.runtime.demo_play(now()) {
                    Ok(panels) => panels,
                    Err(error) => {
                        eprintln!("Demo step failed: {error}");
                        Vec::new()
                    }
                }
            };
            wake_collector(&wake, REQUEST);
            for open in panels {
                let _ = app.emit("demo:panel", open);
            }
        });
}
