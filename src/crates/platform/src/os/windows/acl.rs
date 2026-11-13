//! Per-user access control on Windows: the current user's SID, a protected
//! DACL for that user and SYSTEM, and the directories and files made with it.
use std::{
    ffi::{c_void, OsStr},
    io,
    path::Path,
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
pub(super) struct Local(pub(super) *mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0) };
        }
    }
}
pub(super) fn sid() -> Result<String, String> {
    token_sid(TokenUser)
}
/// The owner Windows gives what this process creates: the user, or for an
/// elevated administrator the Administrators group.
fn default_owner() -> Option<&'static str> {
    static OWNER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    OWNER.get_or_init(|| token_sid(TokenOwner).ok()).as_deref()
}
/// One SID from this process's token (`TokenUser` or `TokenOwner`), as text.
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
pub(super) fn security(inherit: bool) -> Result<Local, String> {
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
    path_owner(path).is_some_and(|owner| sid().is_ok_and(|sid| sid == owner))
}
/// Owned by this process's default owner. Only transcript reads accept this
/// (`files::user_transcript`); the app's own folders and inbox never do.
pub(crate) fn owned_by_default_owner(path: &Path) -> bool {
    path_owner(path).is_some_and(|owner| default_owner() == Some(owner.as_str()))
}
/// A file's owner SID, as text.
fn path_owner(path: &Path) -> Option<String> {
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
        return None;
    }
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(owner, &mut text) } == 0 {
        return None;
    }
    let text = Local(text.cast());
    let mut count = 0;
    unsafe {
        while *text.0.cast::<u16>().add(count) != 0 {
            count += 1;
        }
    }
    Some(unsafe {
        String::from_utf16_lossy(std::slice::from_raw_parts(text.0.cast::<u16>(), count))
    })
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
