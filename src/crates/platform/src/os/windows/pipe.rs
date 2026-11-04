//! Per-user local-only Windows IPC. Async I/O owns cancellation/deadlines; all
//! provider decisions stay in the same portable Requests/coordinator.
use super::acl::{private_directory, security, sid};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::watch,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

mod client;

pub use client::request;

pub(crate) fn name(data: &Path, channel: &str) -> Result<String, String> {
    let mut hash = Sha256::new();
    hash.update(sid()?.as_bytes());
    hash.update([0]);
    let path = std::fs::canonicalize(data).unwrap_or_else(|_| data.to_owned());
    hash.update(path.as_os_str().to_string_lossy().as_bytes());
    Ok(format!(
        "\\\\.\\pipe\\agenthud-{channel}-{:x}",
        hash.finalize()
    ))
}
fn pipe(name: &str, first: bool) -> Result<NamedPipeServer, String> {
    let descriptor = security(false)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(64)
            .create_with_security_attributes_raw(
                name,
                (&attributes as *const SECURITY_ATTRIBUTES)
                    .cast_mut()
                    .cast(),
            )
    }
    .map_err(|_| "The local named-pipe endpoint is occupied or unavailable".into())
}
pub type Handler = Arc<dyn Fn(Value, &AtomicBool) -> Option<Value> + Send + Sync>;
pub struct Server {
    running: Arc<AtomicBool>,
    stop: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub fn start(
        data: &Path,
        channel: &str,
        request_limit: usize,
        response_limit: usize,
        handler: Handler,
    ) -> Result<Self, String> {
        private_directory(data)?;
        let name = name(data, channel)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(64)
            .build()
            .map_err(|_| "The named-pipe runtime is unavailable")?;
        let initial = {
            let _entered = runtime.enter();
            pipe(&name, true)?
        };
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        let (stop, mut stopped) = watch::channel(false);
        let thread = std::thread::Builder::new()
            .name(format!("agenthud-{channel}-pipe"))
            .spawn(move || {
                runtime.block_on(async move {
                    let mut listener = initial;
                    let mut jobs = tokio::task::JoinSet::new();
                    loop {
                        let connected = tokio::select! {
                            _ = stopped.changed() => break,
                            result = listener.connect() => result,
                        };
                        if connected.is_err() || !active.load(Ordering::Acquire) {
                            break;
                        }
                        while jobs.try_join_next().is_some() {}
                        if jobs.len() >= 63 {
                            // Shutdown must also cancel an accept waiting for a
                            // worker slot, including long-lived approval calls.
                            tokio::select! {
                                _ = stopped.changed() => break,
                                _ = jobs.join_next() => {},
                            }
                            while jobs.try_join_next().is_some() {}
                        }
                        let next = match pipe(&name, false) {
                            Ok(next) => next,
                            Err(_) => break,
                        };
                        let stream = std::mem::replace(&mut listener, next);
                        jobs.spawn(serve(
                            stream,
                            request_limit,
                            response_limit,
                            handler.clone(),
                            active.clone(),
                            stopped.clone(),
                        ));
                    }
                    active.store(false, Ordering::Release);
                    jobs.abort_all();
                    while jobs.join_next().await.is_some() {}
                });
                runtime.shutdown_timeout(Duration::from_secs(2));
            })
            .map_err(|_| "The named-pipe worker could not start")?;
        Ok(Self {
            running,
            stop,
            thread: Some(thread),
        })
    }
    pub fn is_listening(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.stop.send(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct ConnectionAlive(Arc<AtomicBool>);
impl Drop for ConnectionAlive {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
async fn serve(
    mut stream: NamedPipeServer,
    request_limit: usize,
    response_limit: usize,
    handler: Handler,
    running: Arc<AtomicBool>,
    mut stopped: watch::Receiver<bool>,
) {
    let read = async {
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0u8; 8192];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            bytes.extend_from_slice(&chunk[..n]);
            if bytes.len() > request_limit {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            if let Some(at) = bytes.iter().position(|b| *b == b'\n') {
                if at + 1 != bytes.len() {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                return Ok(bytes);
            }
        }
    };
    let bytes = tokio::select! {_ = stopped.changed()=>return,read=tokio::time::timeout(Duration::from_secs(2),read)=>match read{Ok(Ok(bytes))=>bytes,_=>return}};
    let Ok(request) = serde_json::from_slice(&bytes) else {
        return;
    };
    let alive = Arc::new(AtomicBool::new(true));
    let _lifetime = ConnectionAlive(alive.clone());
    let callback_alive = alive.clone();
    let callback_running = running.clone();
    let mut result = tokio::task::spawn_blocking(move || {
        if !callback_running.load(Ordering::Acquire) {
            None
        } else {
            handler(request, &callback_alive)
        }
    });
    let mut extra = [0u8; 1];
    let response = tokio::select! {_ = stopped.changed()=>None,_ = stream.read(&mut extra)=>None,response=&mut result=>response.ok().flatten()};
    alive.store(false, Ordering::Release);
    if !running.load(Ordering::Acquire) {
        return;
    }
    if let Some(response) = response {
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        if bytes.len() <= response_limit {
            let _ = tokio::time::timeout(Duration::from_secs(5), stream.write_all(&bytes)).await;
        }
    }
}

#[cfg(test)]
mod tests;
