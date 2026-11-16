//! Management of the user's PATH entry.
//!
//! The `recast` command lives in `%LOCALAPPDATA%\ReCast\bin`. Adding that
//! folder to the user's PATH makes the command available in every new
//! terminal, without administrator rights.

use std::path::Path;

use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

/// Environment key that holds the user's PATH.
const ENVIRONMENT_KEY: &str = "Environment";
const PATH_VALUE: &str = "Path";

/// Adds `dir` to the user's PATH if it is not there yet.
///
/// Returns true when the PATH was changed.
pub fn add_to_user_path(dir: &Path) -> Result<bool, String> {
    let target = dir.to_string_lossy().to_string();
    if target.is_empty() {
        return Err("No folder to add to the PATH.".to_string());
    }

    let current = read_user_path()?;
    if path_contains(&current, &target) {
        return Ok(false);
    }

    let updated = if current.trim().is_empty() {
        target
    } else {
        let mut value = current.trim_end().to_string();
        if !value.ends_with(';') {
            value.push(';');
        }
        value.push_str(&target);
        value
    };

    write_user_path(&updated)?;
    broadcast_environment_change();
    Ok(true)
}

/// Removes `dir` from the user's PATH.
///
/// Returns true when the PATH was changed.
pub fn remove_from_user_path(dir: &Path) -> Result<bool, String> {
    let target = dir.to_string_lossy().to_string();
    let current = read_user_path()?;
    if !path_contains(&current, &target) {
        return Ok(false);
    }

    let kept: Vec<&str> = current
        .split(';')
        .filter(|entry| !entry.trim().eq_ignore_ascii_case(target.trim()))
        .collect();
    let updated = kept.join(";");

    write_user_path(&updated)?;
    broadcast_environment_change();
    Ok(true)
}

/// Reads the user's PATH from the registry.
pub fn read_user_path() -> Result<String, String> {
    let mut key: HKEY = std::ptr::null_mut();
    let wide_key = super::registry::to_wide(ENVIRONMENT_KEY);
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide_key.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if result != 0 {
        return Ok(String::new());
    }

    let wide_name = super::registry::to_wide(PATH_VALUE);
    let mut kind: u32 = 0;
    let mut size: u32 = 0;
    let query = unsafe {
        RegQueryValueExW(
            key,
            wide_name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if query != 0 || size == 0 {
        unsafe {
            RegCloseKey(key);
        }
        return Ok(String::new());
    }

    let mut buffer = vec![0u8; size as usize];
    let query = unsafe {
        RegQueryValueExW(
            key,
            wide_name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    if query != 0 {
        return Ok(String::new());
    }

    // The value is REG_SZ or REG_EXPAND_SZ, so it is UTF-16 and may contain
    // an embedded NUL. Cut it off there.
    let mut wide: Vec<u16> = Vec::with_capacity(buffer.len() / 2);
    for chunk in buffer.as_chunks::<2>().0 {
        let value = u16::from_le_bytes(*chunk);
        if value == 0 {
            break;
        }
        wide.push(value);
    }
    Ok(String::from_utf16_lossy(&wide))
}

fn write_user_path(value: &str) -> Result<(), String> {
    let mut key: HKEY = std::ptr::null_mut();
    let wide_key = super::registry::to_wide(ENVIRONMENT_KEY);
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide_key.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(format!(
            "Cannot open HKCU\\{ENVIRONMENT_KEY} to update the PATH (error {result})."
        ));
    }

    let wide_name = super::registry::to_wide(PATH_VALUE);
    let mut wide_value: Vec<u16> = value.encode_utf16().collect();
    wide_value.push(0);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            wide_value.as_ptr() as *const u8,
            wide_value.len() * std::mem::size_of::<u16>(),
        )
    };
    let result = unsafe {
        RegSetValueExW(
            key,
            wide_name.as_ptr(),
            0,
            REG_EXPAND_SZ,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    if result == 0 {
        Ok(())
    } else {
        Err(format!("Cannot write the user's PATH (error {result})."))
    }
}

/// Tells the running programs that the environment changed, so that new
/// terminals pick the PATH up without a sign-out.
fn broadcast_environment_change() {
    let message = super::registry::to_wide("Environment");
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0usize,
            message.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            3000,
            std::ptr::null_mut(),
        );
    }
}

/// Case-insensitive check whether a PATH list contains an entry.
pub fn path_contains(path: &str, entry: &str) -> bool {
    let wanted = entry.trim().trim_end_matches('\\');
    path.split(';')
        .filter(|part| !part.trim().is_empty())
        .any(|part| {
            let part = part.trim().trim_end_matches('\\');
            part.eq_ignore_ascii_case(wanted)
        })
}
