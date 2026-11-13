//! Owner-only files on POSIX hosts: mode bits and the effective user ID.
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

/// A directory only this user can open (0700), created when absent. A symlink
/// or another user's directory is refused.
pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "The private directory is unavailable")?;
    }
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("The private directory is unavailable".into()),
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "The private directory is unavailable")?;
    if !metadata.is_dir() || !owned_by_user(path, &metadata) {
        return Err("The private directory must be a directory of the current user".into());
    }
    if metadata.permissions().mode() & 0o777 != 0o700 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "The private directory could not be protected")?;
    }
    Ok(())
}

pub(crate) fn owned_by_user(_path: &Path, metadata: &fs::Metadata) -> bool {
    metadata.uid() == unsafe { libc::geteuid() }
}

/// A client's transcript this user may read: owned by this user.
pub(crate) fn user_transcript(path: &Path, metadata: &fs::Metadata) -> bool {
    owned_by_user(path, metadata)
}

/// Owned by this user and closed to group and others.
pub(crate) fn owner_only(path: &Path, metadata: &fs::Metadata) -> bool {
    owned_by_user(path, metadata) && metadata.permissions().mode() & 0o077 == 0
}

/// Opens a private read/write file without following a symlink. Existing files
/// are preserved; callers that need exclusive creation pass `create_new`.
pub(crate) fn private_file(path: &Path, create_new: bool) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    options.open(path)
}

/// A file only this user can read or write, opened to append: each write
/// lands whole at the end, so two writers never overwrite each other.
pub(crate) fn private_append(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

/// A hook payload from stdin, or Err after five seconds or past `limit` bytes.
pub(crate) fn read_stdin(bytes: &mut Vec<u8>, limit: usize) -> Result<(), ()> {
    use std::os::fd::AsRawFd;
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let deadline = Instant::now() + Duration::from_secs(5);
    while bytes.len() <= limit {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(());
        }
        let mut descriptor = libc::pollfd {
            fd: stdin.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                remaining.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if ready < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if ready <= 0 {
            return Err(());
        }
        let mut chunk = [0; 8192];
        match stdin.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(()),
        }
    }
    Err(())
}

/// The client's hook command runs the executable through the user's POSIX shell.
pub(crate) fn hook_configuration(
    source: &str,
    flag: &str,
    current: &serde_json::Value,
    executable: Option<&Path>,
) -> Result<serde_json::Value, String> {
    let command = executable.map(|executable| hook_command(executable, flag, source));
    agent_hud_agents::patch_configuration(source, flag, current, command.as_deref())
}

/// The same command, in a client's settings that are text, not JSON.
pub(crate) fn hook_text(
    source: &str,
    flag: &str,
    original: &str,
    executable: Option<&Path>,
) -> Result<String, String> {
    let command = executable.map(|executable| hook_command(executable, flag, source));
    agent_hud_agents::patch_text(source, flag, original, command.as_deref())
}

/// `'<executable>' <flag> <client>`, the executable single-quoted for a
/// POSIX shell (and for `shlex.split`, which reads it the same).
fn hook_command(executable: &Path, flag: &str, source: &str) -> String {
    format!(
        "'{}' {} {}",
        executable.to_string_lossy().replace('\'', "'\\''"),
        flag,
        source
    )
}
