//! Global shortcuts through Carbon's RegisterEventHotKey: ⌘⌥H, and the one
//! that calls the clipboard window up (⌥Z unless Settings names another).
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock, PoisonError};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
        num_types: u32,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut *mut c_void,
    ) -> i32;
    fn RegisterEventHotKey(
        hot_key_code: u32,
        hot_key_modifiers: u32,
        hot_key_id: EventHotKeyID,
        target: *mut c_void,
        options: u32,
        out_ref: *mut *mut c_void,
    ) -> i32;
    fn UnregisterEventHotKey(hot_key: *mut c_void) -> i32;
    fn GetEventParameter(
        event: *mut c_void,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> i32;
}

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
const SIGNATURE: u32 = 0x4148_5544; // 'AHUD'
const KEY_H: u32 = 4; // kVK_ANSI_H
const CMD_OPTION: u32 = (1 << 8) | (1 << 11); // cmdKey | optionKey
const K_EVENT_CLASS_KEYBOARD: u32 = 0x6B65_7962; // 'keyb'
const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
const K_EVENT_PARAM_DIRECT_OBJECT: u32 = 0x2D2D_2D2D; // '----'
const TYPE_EVENT_HOT_KEY_ID: u32 = 0x686B_6964; // 'hkid'
const GLOW: u32 = 1;
const CLIP: u32 = 2;
/// The clipboard window's hot key while it is held, as an address.
static CLIP_KEY: Mutex<usize> = Mutex::new(0);

unsafe extern "C" fn hotkey_callback(
    _call_ref: *mut c_void,
    event: *mut c_void,
    _user_data: *mut c_void,
) -> i32 {
    let mut hk_id = EventHotKeyID::default();
    let status = unsafe {
        GetEventParameter(
            event,
            K_EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyID>(),
            std::ptr::null_mut(),
            (&mut hk_id as *mut EventHotKeyID).cast::<c_void>(),
        )
    };
    if status == 0 {
        if let Some(app) = APP_HANDLE.get() {
            match hk_id.id {
                GLOW => crate::coordinator::toggle_glow(app),
                CLIP => crate::inspect::summon(app),
                _ => {}
            }
        }
    }
    0
}

pub fn install(app: &tauri::AppHandle) {
    if APP_HANDLE.set(app.clone()).is_err() {
        return;
    }
    let spec = EventTypeSpec {
        event_class: K_EVENT_CLASS_KEYBOARD,
        event_kind: K_EVENT_HOT_KEY_PRESSED,
    };
    let mut handler_ref: *mut c_void = std::ptr::null_mut();
    let mut hotkey_ref: *mut c_void = std::ptr::null_mut();
    let hk_id = EventHotKeyID {
        signature: SIGNATURE,
        id: GLOW,
    };
    unsafe {
        let target = GetApplicationEventTarget();
        let _ = InstallEventHandler(
            target,
            hotkey_callback,
            1,
            &spec,
            std::ptr::null_mut(),
            &mut handler_ref,
        );
        let _ = RegisterEventHotKey(KEY_H, CMD_OPTION, hk_id, target, 0, &mut hotkey_ref);
    }
}

/// Holds the clipboard window's shortcut in place of the last one, or lets
/// it go. A key it cannot name holds nothing.
pub(super) fn set_clip(app: &tauri::AppHandle, shortcut: Option<crate::inspect::Shortcut>) {
    let wanted = shortcut.and_then(|shortcut| {
        let mut modifiers = 0;
        for (on, bit) in [
            (shortcut.command, 1 << 8),
            (shortcut.shift, 1 << 9),
            (shortcut.option, 1 << 11),
            (shortcut.control, 1 << 12),
        ] {
            if on {
                modifiers |= bit;
            }
        }
        Some((key_code(&shortcut.code)?, modifiers))
    });
    let _ = app.run_on_main_thread(move || {
        let mut held = CLIP_KEY.lock().unwrap_or_else(PoisonError::into_inner);
        if *held != 0 {
            unsafe { UnregisterEventHotKey(*held as *mut c_void) };
            *held = 0;
        }
        let Some((code, modifiers)) = wanted else {
            return;
        };
        let id = EventHotKeyID {
            signature: SIGNATURE,
            id: CLIP,
        };
        let mut key: *mut c_void = std::ptr::null_mut();
        let status = unsafe {
            RegisterEventHotKey(
                code,
                modifiers,
                id,
                GetApplicationEventTarget(),
                0,
                &mut key,
            )
        };
        if status == 0 {
            *held = key as usize;
        }
    });
}

/// The virtual key code (Events.h, ANSI layout) of a key as the browser's
/// `KeyboardEvent.code` names it.
fn key_code(code: &str) -> Option<u32> {
    const LETTERS: [u32; 26] = [
        0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F,
        0x23, 0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
    ];
    const DIGITS: [u32; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
    const FUNCTIONS: [u32; 12] = [
        0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F,
    ];
    if let Some(letter) = code.strip_prefix("Key") {
        let [byte] = letter.as_bytes() else {
            return None;
        };
        return byte
            .is_ascii_uppercase()
            .then(|| LETTERS[usize::from(byte - b'A')]);
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        let [byte] = digit.as_bytes() else {
            return None;
        };
        return byte
            .is_ascii_digit()
            .then(|| DIGITS[usize::from(byte - b'0')]);
    }
    if let Some(number) = code.strip_prefix('F') {
        let number: usize = number.parse().ok()?;
        return (1..=12).contains(&number).then(|| FUNCTIONS[number - 1]);
    }
    Some(match code {
        "Space" => 0x31,
        "Minus" => 0x1B,
        "Equal" => 0x18,
        "BracketLeft" => 0x21,
        "BracketRight" => 0x1E,
        "Backslash" => 0x2A,
        "Semicolon" => 0x29,
        "Quote" => 0x27,
        "Comma" => 0x2B,
        "Period" => 0x2F,
        "Slash" => 0x2C,
        "Backquote" => 0x32,
        _ => return None,
    })
}
