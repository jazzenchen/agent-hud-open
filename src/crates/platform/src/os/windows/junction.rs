//! Directory junctions: what a moved data folder leaves at its old place
//! (`home_folder::relocate`). A junction needs no administrator and no
//! developer mode, unlike a symbolic link.
use std::{
    fs, io,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
};
use windows_sys::Win32::{
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    },
    System::IO::DeviceIoControl,
};

const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

pub(crate) fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Makes `link`, which must not exist, a junction to the folder `target`,
/// an absolute path.
pub(crate) fn link_folder(link: &Path, target: &Path) -> io::Result<()> {
    // A junction's NT path takes backslashes only.
    let target = target.to_string_lossy().replace('/', "\\");
    let target = target.strip_prefix(r"\\?\").unwrap_or(&target);
    if !Path::new(target).is_absolute() {
        return Err(io::Error::other("A junction's target must be absolute"));
    }
    let buffer = mount_point(target)?;
    fs::create_dir(link)?;
    let made = (|| {
        let folder = fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(link)?;
        let mut returned = 0u32;
        let done = unsafe {
            DeviceIoControl(
                folder.as_raw_handle(),
                FSCTL_SET_REPARSE_POINT,
                buffer.as_ptr().cast(),
                buffer.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if done == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })();
    if made.is_err() {
        let _ = fs::remove_dir(link);
    }
    made
}

/// A REPARSE_DATA_BUFFER for a mount point: the NT path (`\??\C:\…`) to
/// follow, then the path to show, each ending in a null.
fn mount_point(target: &str) -> io::Result<Vec<u8>> {
    let wide = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
    let substitute = wide(&format!(r"\??\{target}"));
    let print = wide(target);
    let substitute_bytes = substitute.len() * 2;
    let print_bytes = print.len() * 2;
    // Four u16 offsets and lengths, then both names with their nulls.
    let data_length = 8 + substitute_bytes + 2 + print_bytes + 2;
    let field = |value: usize| {
        u16::try_from(value).map_err(|_| io::Error::other("The junction's target is too long"))
    };
    let mut buffer = Vec::with_capacity(8 + data_length);
    buffer.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buffer.extend_from_slice(&field(data_length)?.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&field(substitute_bytes)?.to_le_bytes());
    buffer.extend_from_slice(&field(substitute_bytes + 2)?.to_le_bytes());
    buffer.extend_from_slice(&field(print_bytes)?.to_le_bytes());
    for unit in substitute.iter().chain([&0]).chain(print.iter()).chain([&0]) {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(buffer)
}
