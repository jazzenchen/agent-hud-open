//! JSON-lines development transport; the desktop embeds the same Runtime directly.
use agent_hud_runtime::Runtime;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::PoisonError;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(status) = agent_hud_runtime::hooks::entry(&std::env::args().collect::<Vec<_>>()) {
        std::process::exit(status);
    }
    let app_channel = std::env::args().any(|arg| arg == "--app-channel");
    let directory = if app_channel {
        Some(agent_hud_runtime::local_tools_directory()?)
    } else {
        None
    };
    let executable = std::env::current_exe().ok();
    let _ownership = if app_channel {
        None
    } else {
        Some(agent_hud_runtime::claim_collector(
            false,
            executable.as_deref(),
        )?)
    };
    // App-channel mode is a transport only. No Runtime, watcher, database or
    // provider job is created, even when the application is absent or tools off.
    let mut runtime = if app_channel {
        None
    } else {
        Some(Runtime::new(None)?)
    };
    let collection = runtime.as_ref().map(Runtime::collection_gate);
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let response = match serde_json::from_str::<Value>(&line?) {
            Ok(request) if app_channel => {
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                match agent_hud_runtime::local_tools::request(directory.as_ref().unwrap(), &request)
                {
                    Ok(response) => response,
                    Err(error) => {
                        json!({"id":id,"error":{"code":"tools_unavailable","message":error}})
                    }
                }
            }
            Ok(request) => {
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                match request["method"].as_str() {
                    Some(method) => {
                        // A collection pass waits for an account reading in progress.
                        let _turn = collection
                            .as_ref()
                            .filter(|_| {
                                matches!(method, "snapshot" | "refresh" | "request_snapshot")
                            })
                            .map(|gate| gate.lock().unwrap_or_else(PoisonError::into_inner));
                        match runtime
                            .as_mut()
                            .unwrap()
                            .request(method, request.get("params").cloned().unwrap_or(json!({})))
                        {
                            Ok(result) => json!({"id":id,"result":result}),
                            Err(error) => {
                                json!({"id":id,"error":{"code":"operation_failed","message":error}})
                            }
                        }
                    }
                    None => {
                        json!({"id":id,"error":{"code":"invalid_request","message":"method must be a string"}})
                    }
                }
            }
            Err(_) => {
                json!({"id":null,"error":{"code":"parse_error","message":"Expected one JSON request per line"}})
            }
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
