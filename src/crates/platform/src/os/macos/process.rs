//! Darwin process inspection: proc_pidinfo, proc_pidpath and KERN_PROCARGS2.
//! Only processes of the current user are read.
use super::super::ProcessAncestor;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// The hook's parent processes, nearest first, up to 16 or the first one that
/// belongs to another user.
pub(crate) fn parent_processes() -> Vec<ProcessAncestor> {
    let mut pid = unsafe { libc::getppid() };
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while pid > 1 && result.len() < 16 && seen.insert(pid) {
        let Some(info) = bsd_info(pid) else {
            break;
        };
        let Some(executable) = executable_path(pid) else {
            break;
        };
        let Some((arguments, _)) = arguments(pid).and_then(|data| parse_procargs2(&data, &[]))
        else {
            break;
        };
        let bundle = bundle(&executable);
        result.push(ProcessAncestor {
            pid: u32::try_from(pid).ok(),
            executable,
            arguments,
            bundle,
        });
        pid = info.pbi_ppid as i32;
    }
    result
}

/// The one `codex` process of this user that listens on loopback `port`: its
/// arguments and the requested environment variables.
pub(crate) fn codex_cli_on_port(
    port: u16,
    wanted_env: &[&str],
) -> Option<(Vec<String>, HashMap<String, String>)> {
    const PROC_UID_ONLY: u32 = 5;
    let uid = unsafe { libc::getuid() };
    let needed = unsafe { libc::proc_listpids(PROC_UID_ONLY, uid, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return None;
    }
    let capacity = (needed as usize / std::mem::size_of::<i32>()) + 32;
    let mut pids = vec![0i32; capacity];
    let written = unsafe {
        libc::proc_listpids(
            PROC_UID_ONLY,
            uid,
            pids.as_mut_ptr().cast(),
            (capacity * std::mem::size_of::<i32>()) as i32,
        )
    };
    if written <= 0 {
        return None;
    }
    let count = written as usize / std::mem::size_of::<i32>();
    let mut matches = Vec::new();
    for &pid in &pids[..count] {
        if pid <= 1 {
            continue;
        }
        let Some(executable) = executable_path(pid) else {
            continue;
        };
        if executable.file_name().and_then(|s| s.to_str()) != Some("codex") {
            continue;
        }
        let Some(identity) = identity(pid) else {
            continue;
        };
        if super::listening_ports(pid as u32).is_ok_and(|ports| ports.contains(&port)) {
            matches.push((pid, identity));
        }
    }
    if matches.len() != 1 {
        return None;
    }
    let (pid, before) = matches[0];
    let data = arguments(pid)?;
    // The pid must still name the same process after its arguments were read.
    if identity(pid) != Some(before) {
        return None;
    }
    parse_procargs2(&data, wanted_env)
}

fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of_val(&info) as i32;
        (libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        ) == size
            && info.pbi_uid == libc::getuid())
        .then_some(info)
    }
}

/// Start time, which tells a reused pid from the process first seen.
fn identity(pid: i32) -> Option<(u64, u64)> {
    bsd_info(pid).map(|info| (info.pbi_start_tvsec, info.pbi_start_tvusec))
}

fn executable_path(pid: i32) -> Option<PathBuf> {
    let mut path = vec![0u8; 4096];
    let length = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    if length <= 0 {
        return None;
    }
    let end = path.iter().position(|b| *b == 0).unwrap_or(length as usize);
    Some(PathBuf::from(
        String::from_utf8_lossy(&path[..end]).into_owned(),
    ))
}

/// The raw KERN_PROCARGS2 record, at most 1 MiB.
fn arguments(pid: i32) -> Option<Vec<u8>> {
    unsafe {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let mut size = 0;
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
            || !(5..=1024 * 1024).contains(&size)
        {
            return None;
        }
        let mut data = vec![0u8; size];
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            data.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        data.truncate(size);
        Some(data)
    }
}

/// KERN_PROCARGS2: argc, the executable path, padding, argv, then environ.
fn parse_procargs2(
    data: &[u8],
    wanted_env: &[&str],
) -> Option<(Vec<String>, HashMap<String, String>)> {
    let count = i32::from_ne_bytes(data.get(..4)?.try_into().ok()?);
    if !(1..=4096).contains(&count) {
        return None;
    }
    let mut index = 4 + data[4..].iter().position(|b| *b == 0)?;
    while data.get(index) == Some(&0) {
        index += 1;
    }
    let mut args = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let length = data.get(index..)?.iter().position(|b| *b == 0)?;
        args.push(String::from_utf8_lossy(&data[index..index + length]).into_owned());
        index += length + 1;
    }
    let mut env = HashMap::new();
    while index < data.len() {
        while data.get(index) == Some(&0) {
            index += 1;
        }
        if index >= data.len() {
            break;
        }
        let end = data
            .get(index..)?
            .iter()
            .position(|b| *b == 0)
            .map(|len| index + len)
            .unwrap_or(data.len());
        let entry = &data[index..end];
        if let Some(eq) = entry.iter().position(|b| *b == b'=') {
            if let Ok(key) = std::str::from_utf8(&entry[..eq]) {
                if wanted_env.contains(&key) {
                    env.insert(
                        key.to_owned(),
                        String::from_utf8_lossy(&entry[eq + 1..]).into_owned(),
                    );
                }
            }
        }
        index = end + 1;
    }
    Some((args, env))
}

/// The bundle identifier of an executable inside `X.app/Contents/MacOS/`.
fn bundle(executable: &Path) -> Option<String> {
    let directory = executable.parent()?;
    if directory.file_name()? != "MacOS" || directory.parent()?.file_name()? != "Contents" {
        return None;
    }
    let info = directory.parent()?.join("Info.plist");
    if std::fs::metadata(&info).ok()?.len() > 1024 * 1024 {
        return None;
    }
    let value = plist::Value::from_file(info).ok()?;
    let object = value.as_dictionary()?;
    if object.get("CFBundleExecutable")?.as_string()? != executable.file_name()?.to_str()? {
        return None;
    }
    object
        .get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_owned)
}

/// Whether a process runs on `pid` that started when `started` says, for
/// Claude Code's process registry: signal 0, and the start proc_pidinfo reads.
pub(crate) fn process_runs(pid: i32, started: Option<i64>) -> bool {
    super::super::unix::pid_runs(pid, started, start_of)
}

/// When the process on `pid` started, in Unix seconds, for a process of
/// this user.
pub(crate) fn process_start(pid: i32) -> Option<i64> {
    start_of(pid)
}

/// Whether the stdin and stdout of the process on `pid` are terminals, as
/// that process's own `isatty` would answer: both are open on a `/dev/tty…`
/// character device. False for a process of another user.
pub(crate) fn terminal_io(pid: i32) -> bool {
    [0, 1].into_iter().all(|fd| on_terminal(pid, fd))
}

/// `struct proc_fileinfo` and `struct vnode_fdinfowithpath` of
/// `<sys/proc_info.h>`, which the libc crate leaves out.
#[repr(C)]
struct FileInfo {
    open_flags: u32,
    status: u32,
    offset: libc::off_t,
    kind: i32,
    guard_flags: u32,
}
#[repr(C)]
struct VnodeFdInfoWithPath {
    file: FileInfo,
    vnode: libc::vnode_info_path,
}
const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;

fn on_terminal(pid: i32, fd: i32) -> bool {
    // SAFETY: proc_pidfdinfo fills at most `size` bytes of the zeroed
    // struct, and the path it writes is NUL-terminated within it.
    unsafe {
        let mut info: VnodeFdInfoWithPath = std::mem::zeroed();
        let size = std::mem::size_of_val(&info) as i32;
        if libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDVNODEPATHINFO,
            (&mut info as *mut VnodeFdInfoWithPath).cast(),
            size,
        ) != size
        {
            return false;
        }
        if info.vnode.vip_vi.vi_stat.vst_mode & libc::S_IFMT != libc::S_IFCHR {
            return false;
        }
        std::ffi::CStr::from_ptr(info.vnode.vip_path.as_ptr().cast())
            .to_bytes()
            .starts_with(b"/dev/tty")
    }
}

/// When a process of this user started, in Unix seconds.
fn start_of(pid: i32) -> Option<i64> {
    // SAFETY: proc_pidinfo fills at most `size` bytes of the zeroed struct.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of_val(&info) as i32;
        (libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        ) == size
            && info.pbi_uid == libc::getuid())
        .then_some(info.pbi_start_tvsec as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pid another program now runs on is not the process a file named.
    #[test]
    fn a_pid_whose_process_started_at_another_time_is_another_program() {
        let own = std::process::id() as i32;
        let began = start_of(own).unwrap();
        assert!(process_runs(own, Some(began)), "the same process");
        assert!(!process_runs(own, Some(began - 86_400)), "a reused pid");
        let mut finished = std::process::Command::new("true").spawn().unwrap();
        finished.wait().unwrap();
        assert!(!process_runs(finished.id() as i32, None));
    }

    #[test]
    fn a_process_reads_a_terminal_only_when_its_stdin_and_stdout_are_one() {
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::process::{Command, Stdio};
        let (mut leader, mut follower) = (0, 0);
        // SAFETY: openpty writes the two descriptors it opens.
        let opened = unsafe {
            libc::openpty(
                &mut leader,
                &mut follower,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(opened, 0);
        // SAFETY: both are open descriptors this test alone owns.
        let (leader, follower) =
            unsafe { (OwnedFd::from_raw_fd(leader), OwnedFd::from_raw_fd(follower)) };
        let sleep = |stdin: Stdio, stdout: Stdio| {
            Command::new("/bin/sleep")
                .arg("30")
                .stdin(stdin)
                .stdout(stdout)
                .spawn()
                .unwrap()
        };
        let mut children = vec![
            sleep(
                follower.try_clone().unwrap().into(),
                follower.try_clone().unwrap().into(),
            ),
            sleep(Stdio::null(), follower.try_clone().unwrap().into()),
            sleep(Stdio::piped(), Stdio::piped()),
        ];
        let answers = children
            .iter()
            .map(|child| terminal_io(child.id() as i32))
            .collect::<Vec<_>>();
        assert_eq!(answers, [true, false, false]);
        for child in &mut children {
            let _ = child.kill();
            let _ = child.wait();
        }
        drop((leader, follower));
    }

    #[test]
    fn procargs_environment_keeps_only_named_variables() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&2i32.to_ne_bytes());
        buf.extend_from_slice(b"/usr/local/bin/codex\0\0");
        buf.extend_from_slice(b"codex\0--full-auto\0");
        buf.extend_from_slice(b"TERM_PROGRAM=iTerm.app\0ITERM_SESSION_ID=w0t0p0:ABC\0IGNORED=1\0");
        let (args, env) =
            parse_procargs2(&buf, &["TERM_PROGRAM", "ITERM_SESSION_ID", "TMUX"]).unwrap();
        assert_eq!(args, vec!["codex", "--full-auto"]);
        assert_eq!(
            env.get("TERM_PROGRAM").map(String::as_str),
            Some("iTerm.app")
        );
        assert_eq!(
            env.get("ITERM_SESSION_ID").map(String::as_str),
            Some("w0t0p0:ABC")
        );
        assert!(!env.contains_key("IGNORED"));
    }
}
