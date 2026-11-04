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

#[cfg(test)]
mod tests {
    use super::*;

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
