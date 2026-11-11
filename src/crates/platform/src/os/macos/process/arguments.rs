//! A process's arguments and chosen environment variables, from its
//! KERN_PROCARGS2 record.
use std::collections::HashMap;

/// The raw KERN_PROCARGS2 record, at most 1 MiB.
pub(super) fn arguments(pid: i32) -> Option<Vec<u8>> {
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
pub(super) fn parse_procargs2(
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
