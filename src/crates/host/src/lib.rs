use agent_hud_runtime::Runtime;
use std::sync::Arc;
use tauri::Manager;

use agent_hud_hud::{bots, presentation, vendors};

mod activity;
mod auxiliary;
mod backend;
mod capture;
mod claude_plugin;
mod cli;
mod client_cli;
mod coordinator;
mod deepseek_plugin;
mod demo;
pub mod extension;
mod hud;
mod instance;
mod mcp_clients;
mod messages;
mod phones;
mod picture;
mod requests;
mod screens;
mod service;
mod shell;
mod system;
mod tools;
mod tray;
mod web_sign_in;

use service::Service;

/// Immutable process-wide startup mode, shared by Settings, Statistics and every HUD.
#[derive(Clone, Copy, serde::Serialize)]
struct AppState {
    demo: bool,
}

#[tauri::command]
fn startup_mode(state: tauri::State<'_, AppState>) -> AppState {
    *state.inner()
}

#[derive(Clone, Default)]
pub struct HostOptions {
    pub canonical_executable: Option<std::path::PathBuf>,
}
pub fn run_with_context(
    context: tauri::Context<tauri::Wry>,
    extension: Option<Arc<dyn extension::ProductExtension>>,
) {
    run_with_options(context, extension, HostOptions::default())
}
pub fn run_with_options(
    context: tauri::Context<tauri::Wry>,
    extension: Option<Arc<dyn extension::ProductExtension>>,
    options: HostOptions,
) {
    let arguments = std::env::args().collect::<Vec<_>>();
    if let Some(status) = cli::entry(&arguments) {
        std::process::exit(status);
    }
    if let Some(status) = agent_hud_runtime::hooks::entry(&arguments) {
        std::process::exit(status);
    }
    let demo = demo::wanted(&arguments);
    let mut demo_script = match demo::script(&arguments) {
        Ok(script) => script,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    agent_hud_runtime::l10n::set_system_chinese(shell::system_prefers_chinese());
    let instance = if demo {
        None
    } else {
        match instance::claim(options.canonical_executable.is_some()) {
            instance::Launch::Run(locks) => Some(locks),
            instance::Launch::HandOver(lock) => {
                if !agent_hud_runtime::instance::hand_over(&lock) {
                    instance::refuse(None);
                }
                std::process::exit(0);
            }
            instance::Launch::Refused(holder) => {
                instance::refuse(holder.as_deref());
                std::process::exit(0);
            }
            instance::Launch::Unavailable(error) => {
                instance::unavailable(&error);
                std::process::exit(2);
            }
        }
    };
    tauri::Builder::default()
        .manage(options.clone())
        .manage(AppState { demo })
        .invoke_handler({
            let commands: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                requests::native_request,
                backend::cached_snapshot,
                startup_mode,
                requests::presentation_request,
                mcp_clients::native_mcp_clients,
                claude_plugin::native_claude_plugin,
                deepseek_plugin::native_deepseek_plugin,
                picture::picture_payload,
                picture::picture_complete,
                picture::picture_resize,
                capture::native_capture_webview,
                system::native_system_status,
                activity::window_activity,
                hud::hud_monitors,
                hud::hud_geometry,
                hud::hud_report_regions,
                hud::hud_drag_start,
                hud::hud_text_focus,
                auxiliary::hud_show_main,
                auxiliary::quit_app,
                auxiliary::open_statistics,
                auxiliary::take_statistics_target,
                auxiliary::take_settings_page,
                coordinator::hud_bot_directions,
                coordinator::hud_bot_looked,
                coordinator::hud_update_notice,
                coordinator::open_update,
                web_sign_in::web_sign_in,
            ];
            // A sign-in window shows another site's page: none of Agent
            // HUD's commands answer it, whatever its capabilities say.
            move |invoke| {
                if web_sign_in::foreign(invoke.message.webview_ref().label()) {
                    invoke
                        .resolver
                        .reject("Agent HUD's commands are not open to this page");
                    return true;
                }
                commands(invoke)
            }
        })
        .setup(move |app| {
            // 先接收系統關機通知，再啟動任何背景偵測工作。
            shell::prepare(app.handle())?;
            app.manage(system::System::new(app.config().identifier.clone()));
            app.manage(mcp_clients::Clients::default());
            app.manage(claude_plugin::ClaudePlugin::new(
                extension
                    .as_ref()
                    .and_then(|product| product.claude_plugin_paths()),
            )?);
            app.manage(deepseek_plugin::DeepSeekPlugin::new()?);
            claude_plugin::repair_on_launch(app.handle());
            deepseek_plugin::repair_on_launch(app.handle());
            mcp_clients::repair_on_launch(app.handle());
            app.manage(picture::Pictures::default());
            let directory = agent_hud_runtime::local_tools_directory()?;
            let demo_data = directory.join("demo-run");
            let legacy = if app.state::<AppState>().demo {
                None
            } else {
                agent_hud_runtime::legacy_preferences()
            };
            let presentation = presentation::Presentation::new(if app.state::<AppState>().demo {
                directory.join("demo")
            } else {
                directory
            })?;
            if let Some(value) = legacy.as_ref().and_then(|v| v.get("presentation")) {
                presentation.seed_if_absent(value)?;
            }
            app.manage(presentation);
            coordinator::prepare(app.handle());
            if let Some(product) = &extension {
                product.setup_native(app.handle())?;
            }
            app.manage(activity::WindowActivity::default());
            activity::notify(app.handle(), "main");
            if app.state::<AppState>().demo {
                // The demo goes through the real Core, in a folder of its own.
                let runtime =
                    agent_hud_runtime::Runtime::demo(demo_data.clone(), demo_script.take())?;
                let service = Service::start(
                    app.handle().clone(),
                    runtime,
                    extension.clone(),
                    demo_data,
                    None,
                )?;
                demo::play(app.handle().clone(), &service);
                app.manage(service);
            } else {
                let mut runtime = Runtime::new_with_preferences(
                    None,
                    true,
                    legacy.as_ref().and_then(|v| v.get("settings")).cloned(),
                )?;
                // Client hooks add to the app; it starts without them, and Settings says what is missing (Open #16).
                if let Some(path) = &options.canonical_executable {
                    if let Err(error) = runtime.delegate_hook_owner(path.clone()) {
                        eprintln!("Agent HUD client hooks: {error}");
                    }
                }
                let launch_at_login = runtime.launch_at_login_enabled();
                let data = agent_hud_runtime::local_tools_directory()?;
                let service = Service::start(
                    app.handle().clone(),
                    runtime,
                    extension.clone(),
                    data.clone(),
                    Some(data.join("local-network")),
                )?;
                if launch_at_login {
                    app.state::<system::System>().register_if_never_registered();
                }
                app.manage(service);
            }
            app.manage(hud::HudWindows::default());
            app.manage(auxiliary::AuxiliaryWindows::default());
            app.manage(tray::TrayState::default());
            tray::install(app)?;
            screens::install(app)?;
            shell::install(app.handle());
            coordinator::start(app.handle())?;
            // Keep the claimed lock throughout setup, but accept handovers
            // only once Settings and every command owner can answer them.
            if let Some(lock) = instance {
                let handle = app.handle().clone();
                app.manage(lock.listen(Arc::new(move || auxiliary::show(&handle))));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if web_sign_in::foreign(window.label()) {
                    return;
                }
                if matches!(window.label(), "main" | "statistics") {
                    // Closing Settings or Statistics ends its page and web
                    // content process; the next open builds it again.
                    activity::forget(window.app_handle(), window.label());
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                activity::notify(window.app_handle(), window.label());
            } else if let tauri::WindowEvent::Destroyed = event {
                if matches!(window.label(), "main" | "statistics")
                    || web_sign_in::foreign(window.label())
                {
                    shell::windows_changed(window.app_handle());
                }
            } else if let tauri::WindowEvent::ThemeChanged(theme) = event {
                coordinator::appearance_changed(window.app_handle(), *theme == tauri::Theme::Light);
            } else if matches!(
                event,
                tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_)
            ) {
                activity::notify(window.app_handle(), window.label());
            }
        })
        .build(context)
        .expect("Agent HUD could not start")
        .run(|app, event| {
            // The app lives in the menu bar: closing its last window does not quit it.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = &event
            {
                api.prevent_exit();
            }
            if matches!(event, tauri::RunEvent::Exit) {
                coordinator::stop(app);
                screens::shutdown(app);
                if let Some(service) = app.try_state::<Service>() {
                    service.stop();
                }
                if let Some(windows) = app.try_state::<hud::HudWindows>() {
                    windows.stop();
                }
            }
            if shell::reopened(&event) {
                auxiliary::show(app);
            }
        });
}
