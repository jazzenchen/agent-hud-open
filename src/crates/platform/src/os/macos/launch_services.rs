//! What Launch Services knows: the applications registered under a bundle
//! identifier, whether any application opens a URL, and which browser is the
//! system's.
use std::path::PathBuf;

pub(crate) fn registered_bundle(id: &str) -> bool {
    !registered_applications(id).is_empty()
}

/// Whether Launch Services has an application that can open this URL.
pub(crate) fn registered_url(url: &str) -> bool {
    default_application(url, |_| ()).is_some()
}

/// The bundle identifier of the system's web browser, the application that
/// opens an `https` address.
pub(crate) fn default_browser() -> Option<String> {
    use std::{
        ffi::{c_void, CStr},
        ptr,
    };
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFBundleCreate(allocator: *const c_void, url: *const c_void) -> *const c_void;
        fn CFBundleGetIdentifier(bundle: *const c_void) -> *const c_void;
        fn CFStringGetCString(
            text: *const c_void,
            buffer: *mut i8,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }
    default_application("https://example.com/", |app| unsafe {
        let bundle = CFBundleCreate(ptr::null(), app);
        if bundle.is_null() {
            return None;
        }
        // Owned by the bundle: read before it is released.
        let id = CFBundleGetIdentifier(bundle);
        let mut buffer = [0i8; 256];
        let read = !id.is_null()
            && CFStringGetCString(id, buffer.as_mut_ptr(), buffer.len() as isize, 0x08000100) != 0;
        CFRelease(bundle);
        read.then(|| {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        })
    })
    .flatten()
}

/// What `read` makes of the application Launch Services opens `url` with;
/// none when there is no such application.
fn default_application<T>(url: &str, read: impl FnOnce(*const std::ffi::c_void) -> T) -> Option<T> {
    use std::{ffi::c_void, ptr};
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultApplicationURLForURL(
            url: *const c_void,
            roles: u32,
            error: *mut *const c_void,
        ) -> *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            base: *const c_void,
        ) -> *const c_void;
        fn CFRelease(value: *const c_void);
    }
    unsafe {
        let url = CFURLCreateWithBytes(
            ptr::null(),
            url.as_ptr(),
            url.len() as isize,
            0x08000100,
            ptr::null(),
        );
        if url.is_null() {
            return None;
        }
        let app = LSCopyDefaultApplicationURLForURL(url, u32::MAX, ptr::null_mut());
        CFRelease(url);
        if app.is_null() {
            return None;
        }
        let read = read(app);
        CFRelease(app);
        Some(read)
    }
}

/// The applications Launch Services knows by bundle identifier, wherever
/// they are installed (agent-hud-native-open VendorCatalog.applications).
pub fn registered_applications(id: &str) -> Vec<PathBuf> {
    use std::{
        ffi::{c_void, CStr, CString},
        os::unix::ffi::OsStrExt,
        ptr,
    };
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyApplicationURLsForBundleIdentifier(
            id: *const c_void,
            error: *mut *const c_void,
        ) -> *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            text: *const i8,
            encoding: u32,
        ) -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFURLGetFileSystemRepresentation(
            url: *const c_void,
            resolve: u8,
            buffer: *mut u8,
            length: isize,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }
    let id = CString::new(id).unwrap();
    let mut found = Vec::new();
    unsafe {
        let string = CFStringCreateWithCString(ptr::null(), id.as_ptr(), 0x08000100);
        if string.is_null() {
            return found;
        }
        let apps = LSCopyApplicationURLsForBundleIdentifier(string, ptr::null_mut());
        CFRelease(string);
        if apps.is_null() {
            return found;
        }
        for index in 0..CFArrayGetCount(apps) {
            let url = CFArrayGetValueAtIndex(apps, index);
            let mut buffer = [0u8; 4096];
            if !url.is_null()
                && CFURLGetFileSystemRepresentation(
                    url,
                    1,
                    buffer.as_mut_ptr(),
                    buffer.len() as isize,
                ) != 0
            {
                if let Ok(path) = CStr::from_bytes_until_nul(&buffer) {
                    found.push(PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes())));
                }
            }
        }
        CFRelease(apps);
    }
    found
}
