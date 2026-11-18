//! Whether a directory's DACL is already private: protected from what its
//! parent passes on, open only to the current user and SYSTEM, and giving the
//! user full access to the directory and to what is made in it.
use super::acl::{sid_text, wide, Local};
use std::path::Path;
use windows_sys::Win32::{
    Foundation::GENERIC_ALL,
    Security::{
        Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
        GetAce, GetSecurityDescriptorControl, ACCESS_ALLOWED_ACE, ACL, CONTAINER_INHERIT_ACE,
        DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE, OBJECT_INHERIT_ACE, SE_DACL_PROTECTED,
    },
    Storage::FileSystem::FILE_ALL_ACCESS,
};

/// `ACCESS_ALLOWED_ACE_TYPE` from winnt.h; windows-sys keeps it in a module
/// this crate does not otherwise need.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const SYSTEM: &str = "S-1-5-18";

/// True when `path`'s DACL already gives what `acl::security(true)` sets.
/// Windows may store that one entry per user as two, one for the directory
/// and one passed on to what is made in it; both shapes count.
pub(super) fn private(path: &Path, user: &str) -> bool {
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let path = wide(path.as_os_str());
    let status = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    let _descriptor = Local(descriptor);
    // No DACL at all lets everyone in.
    if status != 0 || dacl.is_null() {
        return false;
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED == 0
    {
        return false;
    }
    let (mut opens, mut passes_on) = (false, false);
    for index in 0..unsafe { (*dacl).AceCount } {
        let mut ace = std::ptr::null_mut();
        if unsafe { GetAce(dacl, u32::from(index), &mut ace) } == 0 {
            return false;
        }
        let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
        let (header, mask) = unsafe { ((*ace).Header, (*ace).Mask) };
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE {
            return false;
        }
        let Some(trustee) = sid_text(unsafe { std::ptr::addr_of_mut!((*ace).SidStart) }.cast())
        else {
            return false;
        };
        if trustee == SYSTEM {
            continue;
        }
        if trustee != user {
            return false;
        }
        let full = mask & GENERIC_ALL != 0 || mask & FILE_ALL_ACCESS == FILE_ALL_ACCESS;
        let flags = u32::from(header.AceFlags);
        let inherits = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
        opens |= full && flags & INHERIT_ONLY_ACE == 0;
        passes_on |= full && flags & inherits == inherits;
    }
    opens && passes_on
}
