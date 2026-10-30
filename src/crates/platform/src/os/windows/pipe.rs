//! Per-user local-only Windows IPC. Async I/O owns cancellation/deadlines; all
//! provider decisions stay in the same portable Requests/coordinator.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_void, OsStr},
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
    sync::watch,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, GENERIC_READ, GENERIC_WRITE,
        HANDLE, INVALID_HANDLE_VALUE,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SE_FILE_OBJECT,
        },
        GetTokenInformation, SetFileSecurityW, TokenOwner, TokenUser, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
        TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_ALWAYS,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
fn wide(value: &OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(Some(0)).collect()
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0) };
        }
    }
}
/// One SID from this process's token, as text.
fn token_sid(class: i32) -> Result<String, String> {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err("Current-user token unavailable".into());
        }
        let mut bytes = 0;
        GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut bytes);
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        let ok = GetTokenInformation(token, class, buffer.as_mut_ptr().cast(), bytes, &mut bytes);
        CloseHandle(token);
        if ok == 0 {
            return Err("Current-user identity unavailable".into());
        }
        // TOKEN_USER and TOKEN_OWNER both begin with the SID pointer.
        let sid = if class == TokenUser {
            (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid
        } else {
            (*buffer.as_ptr().cast::<TOKEN_OWNER>()).Owner
        };
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err("Current-user identity unavailable".into());
        }
        let text = Local(text.cast());
        let mut count = 0;
        while *text.0.cast::<u16>().add(count) != 0 {
            count += 1;
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            text.0.cast::<u16>(),
            count,
        )))
    }
}
fn sid() -> Result<String, String> {
    token_sid(TokenUser)
}
/// The owner Windows gives what this process creates. For an elevated
/// administrator that is the Administrators group, not the user, so files the
/// user's own elevated programs made (Claude Code's transcripts, this app's
/// earlier data) carry it.
fn default_owner() -> Option<&'static str> {
    static OWNER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    OWNER.get_or_init(|| token_sid(TokenOwner).ok()).as_deref()
}
fn security(inherit: bool) -> Result<Local, String> {
    let sid = sid()?;
    let flags = if inherit { "OICI" } else { "" };
    let sddl = wide(OsStr::new(&format!(
        "O:{sid}D:P(A;{flags};GA;;;{sid})(A;{flags};GA;;;SY)"
    )));
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("Per-user IPC access control unavailable".into());
    }
    Ok(Local(descriptor))
}
pub(crate) fn owned_path(path: &Path) -> bool {
    let mut owner = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let path = wide(path.as_os_str());
    let status = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    let _descriptor = Local(descriptor);
    if status != 0 || owner.is_null() {
        return false;
    }
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(owner, &mut text) } == 0 {
        return false;
    }
    let text = Local(text.cast());
    let mut count = 0;
    unsafe {
        while *text.0.cast::<u16>().add(count) != 0 {
            count += 1;
        }
    }
    let owner = unsafe {
        String::from_utf16_lossy(std::slice::from_raw_parts(text.0.cast::<u16>(), count))
    };
    sid().is_ok_and(|sid| sid == owner) || default_owner() == Some(owner.as_str())
}
pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        // A later store may protect a missing ancestor too. Give every newly
        // created directory the user's ownership, leaving existing parents alone.
        if !parent
            .try_exists()
            .map_err(|_| "The private IPC directory is unavailable")?
        {
            private_directory(parent)?;
        }
    }
    let descriptor = security(true)?;
    let wide_path = wide(path.as_os_str());
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    // The default owner of an elevated process may be Administrators. Set this
    // new leaf's owner explicitly at creation; never take ownership of an
    // existing directory, including one owned by another administrator.
    if unsafe { CreateDirectoryW(wide_path.as_ptr(), &attributes) } == 0
        && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
    {
        return Err("The private IPC directory is unavailable".into());
    }
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "The private IPC directory is unavailable")?;
    use std::os::windows::fs::MetadataExt;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err("The IPC directory cannot be a reparse point".into());
    }
    if !owned_path(path) {
        return Err("The IPC directory must belong to the current user".into());
    }
    if unsafe {
        SetFileSecurityW(
            wide_path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor.0,
        )
    } == 0
    {
        return Err("The private IPC directory ACL could not be set".into());
    }
    Ok(())
}

/// A protected file's DACL is inherited, but its owner is not: an elevated
/// process otherwise creates Administrators-owned files that our reader must
/// reject. Supply both at creation without taking ownership of existing files.
pub(crate) fn private_file(path: &Path, create_new: bool) -> io::Result<std::fs::File> {
    use std::os::windows::{fs::MetadataExt, io::FromRawHandle};
    let descriptor = security(false).map_err(io::Error::other)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let handle = unsafe {
        CreateFileW(
            wide(path.as_os_str()).as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            if create_new { CREATE_NEW } else { OPEN_ALWAYS },
            FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { std::fs::File::from_raw_handle(handle) };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || !owned_path(path)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The private file must be a regular file of the current user",
        ));
    }
    Ok(file)
}
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
        if let Ok(mut bytes) = serde_json::to_vec(&response) {
            bytes.push(b'\n');
            if bytes.len() <= response_limit {
                let _ =
                    tokio::time::timeout(Duration::from_secs(5), stream.write_all(&bytes)).await;
            }
        }
    }
}
pub fn request(
    data: &Path,
    channel: &str,
    message: &Value,
    request_limit: usize,
    response_limit: usize,
    timeout: Duration,
    mut settled: impl FnMut() -> bool,
) -> Result<Option<Value>, String> {
    let mut bytes = serde_json::to_vec(message).map_err(|_| "Invalid local IPC request")?;
    bytes.push(b'\n');
    if bytes.len() > request_limit {
        return Err("The local IPC request is too large".into());
    }
    let name = name(data, channel)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "The local IPC runtime is unavailable")?;
    runtime.block_on(async {
        let mut stream = connect(&name, timeout.min(Duration::from_secs(2))).await?;
        tokio::time::timeout(Duration::from_secs(2), stream.write_all(&bytes))
            .await
            .map_err(|_| "The local IPC request timed out")?
            .map_err(|_| "The local IPC request failed")?;
        let deadline = Instant::now() + timeout;
        let mut response = Vec::new();
        loop {
            if settled() || Instant::now() >= deadline {
                return Ok(None);
            }
            let mut chunk = [0; 8192];
            match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut chunk)).await {
                Err(_) => continue,
                Ok(Err(_)) => return Err("The local IPC response failed".into()),
                Ok(Ok(0)) => {
                    return if response.is_empty() {
                        Ok(None)
                    } else {
                        serde_json::from_slice(&response)
                            .map(Some)
                            .map_err(|_| "The local IPC response is invalid".into())
                    }
                }
                Ok(Ok(n)) => {
                    response.extend_from_slice(&chunk[..n]);
                    if response.len() > response_limit {
                        return Err("The local IPC response is too large".into());
                    }
                    if response.last() == Some(&b'\n') {
                        return serde_json::from_slice(&response)
                            .map(Some)
                            .map_err(|_| "The local IPC response is invalid".into());
                    }
                }
            }
        }
    })
}

/// A connected instance may be busy for the brief interval before the listener
/// creates its next instance. Retry only that Windows error, within the caller's
/// deadline; a missing endpoint still fails immediately.
async fn connect(
    name: &str,
    timeout: Duration,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match ClientOptions::new().open(name) {
            Ok(stream) => return Ok(stream),
            Err(error)
                if error.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                    && Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(_) => {
                return Err(
                    "Agent HUD is not listening; enable the requested local service in Settings"
                        .into(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn missing_private_directory_ancestors_keep_user_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path().join("outer");
        let inner = outer.join("inner");
        let data = inner.join("private");
        private_directory(&data).unwrap();
        for directory in [&outer, &inner, &data] {
            assert!(owned_path(directory));
            private_directory(directory).unwrap();
        }
    }

    #[test]
    fn private_files_keep_user_ownership_and_reopening_preserves_contents() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("private");
        private_directory(&directory).unwrap();
        let path = directory.join("account.lock");
        private_file(&path, true)
            .unwrap()
            .write_all(b"preserved")
            .unwrap();
        assert!(owned_path(&path));
        assert!(private_file(&path, false).is_ok());
        assert_eq!(std::fs::read(&path).unwrap(), b"preserved");
        assert_eq!(
            private_file(&path, true).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );

        // Hosted runners are elevated: files made with default security can
        // belong to Administrators. Never repair those by taking ownership.
        let other = temp.path().join("default-owner");
        std::fs::write(&other, b"untouched").unwrap();
        if !owned_path(&other) {
            assert_eq!(
                private_file(&other, false).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            );
            assert!(!owned_path(&other));
            assert_eq!(std::fs::read(&other).unwrap(), b"untouched");
        }
    }

    #[test]
    fn private_pipe_round_trip_and_exclusive_owner() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("private");
        let handler: Handler = Arc::new(|message, _| Some(message));
        let server = Server::start(&data, "test", 1024, 1024, handler.clone()).unwrap();
        assert!(server.is_listening());
        assert!(owned_path(&data));
        assert!(Server::start(&data, "test", 1024, 1024, handler).is_err());
        let message = json!({"text": "请求 C:\\my project", "empty": ""});
        assert_eq!(
            request(
                &data,
                "test",
                &message,
                1024,
                1024,
                Duration::from_secs(2),
                || false
            )
            .unwrap(),
            Some(message)
        );
    }

    #[test]
    fn saturated_pipe_shutdown_cancels_waiting_approvals() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("private");
        let started = Arc::new(AtomicUsize::new(0));
        let count = started.clone();
        let handler: Handler = Arc::new(move |_, active| {
            count.fetch_add(1, Ordering::Release);
            while active.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            None
        });
        let server = Server::start(&data, "saturated", 1024, 1024, handler).unwrap();
        let name = name(&data, "saturated").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _clients = runtime.block_on(async {
            let mut clients = Vec::new();
            for _ in 0..64 {
                let mut client = connect(&name, Duration::from_secs(5)).await.unwrap();
                client.write_all(b"{}\n").await.unwrap();
                clients.push(client);
            }
            clients
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while started.load(Ordering::Acquire) < 63 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(started.load(Ordering::Acquire), 63);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            drop(server);
            let _ = sender.send(());
        });
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("saturated server must stop");
    }
}
