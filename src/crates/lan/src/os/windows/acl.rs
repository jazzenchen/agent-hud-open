//! Owner-only credential folders: a protected, inheritable ACL for the current
//! user and SYSTEM, which the files written inside inherit.
use crate::Result;
use std::{
    ffi::c_void,
    fs,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
        },
        GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(super) struct Local(pub(super) *mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0) };
        }
    }
}

pub(super) unsafe fn sid_string(sid: *mut c_void) -> Result<String> {
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err("The LAN directory owner is unavailable".into());
    }
    let text = Local(text.cast());
    let mut length = 0;
    while unsafe { *text.0.cast::<u16>().add(length) } != 0 {
        length += 1;
    }
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text.0.cast::<u16>(), length)
    }))
}

pub(super) fn current_user() -> Result<String> {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err("The current Windows user token is unavailable".into());
    }
    let mut size = 0;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size) };
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    unsafe { CloseHandle(token) };
    if ok == 0 {
        return Err("The current Windows user identity is unavailable".into());
    }
    unsafe { sid_string((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid) }
}

fn directory_security(sid: &str) -> Result<Local> {
    // Protected: remove inherited broad grants. Both file and subdirectory
    // inheritance keep every newly written credential private as well.
    // An explicit owner also avoids Administrators becoming a new directory's
    // default owner when this user's process is running elevated.
    let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;GA;;;{sid})(A;OICI;GA;;;SY)")
        .encode_utf16()
        .chain(Some(0))
        .collect();
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
        return Err("The private LAN directory ACL could not be created".into());
    }
    Ok(Local(descriptor))
}

pub(crate) fn create_directory(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        // A later store can protect one of these ancestors too. Give newly
        // created parents the same user ownership even in an elevated process.
        if !parent.try_exists().map_err(|e| e.to_string())? {
            create_directory(parent)?;
        }
    }
    let descriptor = directory_security(&current_user()?)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } == 0
        && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
    {
        return Err("The private LAN directory could not be created".into());
    }
    Ok(())
}

pub(crate) fn protect_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("The LAN credential directory must be a real directory".into());
    }
    let sid = current_user()?;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
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
    let _owner = Local(descriptor);
    if status != ERROR_SUCCESS || owner.is_null() || unsafe { sid_string(owner) }? != sid {
        return Err("The LAN credential directory must belong to the current user".into());
    }
    let descriptor = directory_security(&sid)?;
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = std::ptr::null_mut();
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
        == 0
        || present == 0
        || dacl.is_null()
    {
        return Err("The private LAN directory ACL is unavailable".into());
    }
    if unsafe {
        SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        )
    } != ERROR_SUCCESS
    {
        return Err("The private LAN directory ACL could not be applied".into());
    }
    Ok(())
}

/// New credential files inherit the containing directory's protected user ACL.
pub(crate) fn private_file(_options: &mut fs::OpenOptions) {}
