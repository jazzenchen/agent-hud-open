//! Where Windows shortcuts (`.lnk`) point, read from the files themselves:
//! the Shell Link format's LinkInfo local path ([MS-SHLLINK] 2.1, 2.3). No
//! shell, COM or script runs, so a read takes milliseconds (VirPoe,
//! agent-hud-open#34).
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

const HAS_LINK_TARGET_ID_LIST: u32 = 0x1;
const HAS_LINK_INFO: u32 = 0x2;
const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 0x1;
/// Enough for the Start menu's folders; deeper trees are not app shortcuts.
const MAX_DEPTH: usize = 4;
/// Shortcuts are small; anything larger is not read.
const MAX_SIZE: u64 = 64 * 1024;

/// How long one walk of the Start menu and desktops is reused: an app
/// installed meanwhile is found on the next walk.
const REUSE: Duration = Duration::from_secs(300);

/// The programs the Start menu and desktop shortcuts point at, the user's and
/// all users'. Walked at most once in `REUSE`, not on every lookup.
pub(super) fn programs() -> Vec<PathBuf> {
    static WALKED: Mutex<Option<(Instant, Vec<PathBuf>)>> = Mutex::new(None);
    let mut walked = WALKED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, programs)) = walked.as_ref().filter(|(at, _)| at.elapsed() < REUSE) {
        return programs.clone();
    }
    let env = |key: &str| std::env::var_os(key).map(PathBuf::from);
    let start_menu = Path::new("Microsoft/Windows/Start Menu/Programs");
    let roots = [
        env("APPDATA").map(|dir| dir.join(start_menu)),
        env("ProgramData").map(|dir| dir.join(start_menu)),
        env("USERPROFILE").map(|dir| dir.join("Desktop")),
        env("PUBLIC").map(|dir| dir.join("Desktop")),
    ];
    let mut programs: Vec<PathBuf> = Vec::new();
    for target in roots.into_iter().flatten().flat_map(|root| targets(&root)) {
        let program = target
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
        if program && !programs.contains(&target) {
            programs.push(target);
        }
    }
    *walked = Some((Instant::now(), programs.clone()));
    programs
}

/// The local targets of the shortcuts under `root`, in no particular order.
fn targets(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(root, 0, &mut found);
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() && depth < MAX_DEPTH {
            walk(&path, depth + 1, found);
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
            && entry.metadata().is_ok_and(|meta| meta.len() <= MAX_SIZE)
        {
            if let Some(target) = std::fs::read(&path).ok().and_then(|bytes| target(&bytes)) {
                found.push(target);
            }
        }
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize)
}
fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// A NUL-terminated single-byte string; only ASCII is taken as is, since the
/// file's code page is not recorded.
fn ansi(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let text = &rest[..rest.iter().position(|b| *b == 0)?];
    text.is_ascii()
        .then(|| String::from_utf8_lossy(text).into_owned())
}
fn unicode(bytes: &[u8], at: usize) -> Option<String> {
    let units: Vec<u16> = bytes
        .get(at..)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16(&units).ok()
}

/// The local path a shortcut's bytes point at, when it has one.
fn target(bytes: &[u8]) -> Option<PathBuf> {
    // ShellLinkHeader: size 0x4C and the shell link CLSID.
    if u32_at(bytes, 0)? != 0x4C
        || bytes.get(4..20)?
            != [
                0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x46,
            ]
    {
        return None;
    }
    let flags = u32_at(bytes, 0x14)? as u32;
    let mut at = 0x4C;
    if flags & HAS_LINK_TARGET_ID_LIST != 0 {
        at += 2 + u16_at(bytes, at)?;
    }
    if flags & HAS_LINK_INFO == 0 {
        return None;
    }
    let info = bytes.get(at..at + u32_at(bytes, at)?)?;
    let header = u32_at(info, 4)?;
    if u32_at(info, 8)? as u32 & VOLUME_ID_AND_LOCAL_BASE_PATH == 0 {
        return None;
    }
    let (base, suffix) = if header >= 0x24 {
        (
            unicode(info, u32_at(info, 28)?)?,
            unicode(info, u32_at(info, 32)?)?,
        )
    } else {
        (
            ansi(info, u32_at(info, 16)?)?,
            ansi(info, u32_at(info, 24)?)?,
        )
    };
    let path = PathBuf::from(base + &suffix);
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shortcut with an empty item list and a LinkInfo whose local base
    /// path is `path`, as Explorer writes one for a file on a local drive.
    fn shortcut(path: &str, unicode: bool) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x4C];
        bytes[0] = 0x4C;
        bytes[4..20].copy_from_slice(&[
            0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x46,
        ]);
        bytes[0x14..0x18].copy_from_slice(&(HAS_LINK_TARGET_ID_LIST | HAS_LINK_INFO).to_le_bytes());
        bytes.extend(2u16.to_le_bytes()); // item list: just its terminator
        bytes.extend([0, 0]);
        let header: u32 = if unicode { 0x24 } else { 0x1C };
        let volume = [
            16u32.to_le_bytes(),
            3u32.to_le_bytes(),
            0u32.to_le_bytes(),
            16u32.to_le_bytes(),
        ]
        .concat();
        let mut info = vec![0u8; header as usize];
        let volume_at = info.len();
        info.extend(&volume);
        info.push(0);
        let ansi_at = info.len();
        info.extend(if unicode {
            b"?".as_slice()
        } else {
            path.as_bytes()
        });
        info.push(0);
        let suffix_at = info.len();
        info.push(0);
        let (wide_at, wide_suffix_at) = if unicode {
            let at = info.len();
            info.extend(path.encode_utf16().flat_map(u16::to_le_bytes));
            info.extend([0, 0]);
            let suffix = info.len();
            info.extend([0, 0]);
            (at, suffix)
        } else {
            (0, 0)
        };
        let size = info.len() as u32;
        let mut fields = [
            size,
            header,
            VOLUME_ID_AND_LOCAL_BASE_PATH,
            volume_at as u32,
            ansi_at as u32,
            0,
            suffix_at as u32,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        if unicode {
            fields.extend((wide_at as u32).to_le_bytes());
            fields.extend((wide_suffix_at as u32).to_le_bytes());
        }
        info[..fields.len()].copy_from_slice(&fields);
        bytes.extend(info);
        bytes.extend([0, 0, 0, 0]); // no extra data
        bytes
    }

    #[test]
    fn a_shortcut_names_its_local_target() {
        assert_eq!(
            target(&shortcut(r"D:\ChatGPT\app\ChatGPT.exe", false)),
            Some(PathBuf::from(r"D:\ChatGPT\app\ChatGPT.exe"))
        );
        assert_eq!(
            target(&shortcut(r"D:\应用\Claude\Claude.exe", true)),
            Some(PathBuf::from(r"D:\应用\Claude\Claude.exe"))
        );
    }

    #[test]
    fn other_files_and_relative_or_damaged_shortcuts_name_nothing() {
        assert_eq!(target(b"not a shortcut"), None);
        assert_eq!(target(&shortcut(r"app\ChatGPT.exe", false)), None);
        let whole = shortcut(r"D:\ChatGPT\app\ChatGPT.exe", false);
        for cut in [0x10, 0x4C, 0x60, whole.len() - 12] {
            assert_eq!(target(&whole[..cut]), None, "cut at {cut}");
        }
    }

    #[test]
    fn shortcuts_are_found_in_subfolders() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("OpenAI");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("ChatGPT.lnk"),
            shortcut(r"D:\ChatGPT\ChatGPT.exe", false),
        )
        .unwrap();
        std::fs::write(temp.path().join("readme.txt"), b"x").unwrap();
        assert_eq!(
            targets(temp.path()),
            vec![PathBuf::from(r"D:\ChatGPT\ChatGPT.exe")]
        );
    }
}
