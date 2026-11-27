//! Writing string values under `HKEY_CURRENT_USER`: the one place knobas
//! writes a registry value from. `link_handoff::manifest` still deletes its
//! key (`RegDeleteTreeW`) and reads it back (`RegQueryValueExW`) itself. The
//! writers are the Firefox Native
//! Messaging host key (#2530, `link_handoff::manifest`) and the desktop
//! notification identity under `Software\Classes\AppUserModelId` (#2529,
//! `notify::platform`). Per-user keys only, so nothing here needs elevation.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};
use windows::core::PCWSTR;

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Create `HKCU\<subkey>` if it is missing and set its `name` value (the
/// key's default value when `None`) to `value` as a `REG_SZ`.
///
/// # Errors
///
/// The failing call and its code, as a sentence for the log.
pub fn set_string(subkey: &str, name: Option<&str>, value: &OsStr) -> Result<(), String> {
    let key_path = wide(OsStr::new(subkey));
    let name = name.map(|name| wide(OsStr::new(name)));
    let value = wide(value);
    // SAFETY: a `u16` slice viewed as its bytes, the shape `REG_SZ` takes.
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
    let mut hkey = HKEY::default();
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let created = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    if created != ERROR_SUCCESS {
        return Err(format!("RegCreateKeyExW {subkey} failed: {created:?}"));
    }
    let name_ptr = name
        .as_ref()
        .map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr()));
    // SAFETY: `hkey` was opened above; `name_ptr` is null or NUL-terminated.
    let set = unsafe { RegSetValueExW(hkey, name_ptr, Some(0), REG_SZ, Some(bytes)) };
    // SAFETY: closes the key opened above, once.
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if set != ERROR_SUCCESS {
        return Err(format!("RegSetValueExW {subkey} failed: {set:?}"));
    }
    Ok(())
}
