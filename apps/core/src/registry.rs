//! Registration of the virtual camera filter in the per-user registry.
//!
//! Everything is written under `HKCU\Software\Classes`, so no administrator
//! rights are needed and the camera disappears with the user profile.

use std::path::Path;

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_WRITE, REG_BINARY, REG_OPTION_NON_VOLATILE, REG_SZ,
};

/// CLSID of the current filter.
pub const NEW_CLSID: &str = "{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}";
/// CLSID of the pre-rebrand filter, kept so that old selections keep working.
pub const LEGACY_CLSID: &str = "{AEF3B972-5FA5-4647-9571-358EB472BC9E}";
/// The DirectShow video input device category.
pub const CAMERA_CATEGORY: &str = "{860BB310-5D01-11D0-BD3B-00A0C911CE86}";
/// Name the camera shows up with.
pub const FILTER_NAME: &str = "ReCast Camera";
/// Name of the filter that shipped before the rebrand.
pub const OLD_FILTER_NAME: &str = "DirectShow Softcam";
/// Name of the filter that shipped under the previous product name.
pub const PREVIOUS_FILTER_NAME: &str = "Rewebcam Virtual Camera";
/// Name of the filter that shipped with the word "virtual" in it. Some
/// platforms refuse a camera with such a name, so it is removed as well.
pub const VIRTUAL_FILTER_NAME: &str = "ReCast Virtual Camera";

/// REGFILTER2 blob describing the single video output pin. The same for both
/// DLL architectures.
pub const FILTER_DATA: [u8; 88] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x70, 0x69, 0x33, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x74, 0x79, 0x33, 0x00, 0x00, 0x00, 0x00,
    0x38, 0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x76, 0x69, 0x64, 0x73, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Registers the virtual camera for the current user.
///
/// `install_dir` is the folder that holds `vcam\x64\softcam.dll` and
/// `vcam\x86\softcam.dll`.
pub fn register_virtual_camera(install_dir: &Path) -> Result<(), String> {
    let x64 = install_dir
        .join("vcam")
        .join("x64")
        .join("softcam.dll")
        .to_string_lossy()
        .to_string();
    let x86 = install_dir
        .join("vcam")
        .join("x86")
        .join("softcam.dll")
        .to_string_lossy()
        .to_string();

    for (classes_root, dll) in [
        ("Software\\Classes", x64.as_str()),
        ("Software\\Classes\\WOW6432Node", x86.as_str()),
    ] {
        // "ReCast Virtual Camera" is refused by some apps because of the word
        // "virtual". The older names stay registered as aliases: Discord keeps
        // them selected from a previous install and would otherwise open a
        // filter that does not receive the current frames.
        delete_registry_tree(&format!(
            "{classes_root}\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{VIRTUAL_FILTER_NAME}"
        ));
        // The previous installer used the friendly name as the instance key.
        // The CLSID key below replaces it, so the name key must not remain
        // and list a second ReCast Camera.
        delete_registry_tree(&format!(
            "{classes_root}\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{FILTER_NAME}"
        ));

        for (clsid, name) in [(NEW_CLSID, FILTER_NAME), (LEGACY_CLSID, OLD_FILTER_NAME)] {
            let base = format!("{classes_root}\\CLSID\\{clsid}");
            reg_set_string(&base, None, name)?;
            reg_set_string(&format!("{base}\\InprocServer32"), None, dll)?;
            reg_set_string(
                &format!("{base}\\InprocServer32"),
                Some("ThreadingModel"),
                "Both",
            )?;
        }

        // The instance key is the CLSID, the same shape OBS uses. A key that
        // is only the friendly name is dropped by Discord's device list.
        register_instance(classes_root, NEW_CLSID, FILTER_NAME, NEW_CLSID)?;
        register_instance(classes_root, OLD_FILTER_NAME, OLD_FILTER_NAME, NEW_CLSID)?;
        register_instance(
            classes_root,
            PREVIOUS_FILTER_NAME,
            PREVIOUS_FILTER_NAME,
            NEW_CLSID,
        )?;
    }

    Ok(())
}

/// Removes the per-user registration of the virtual camera.
pub fn unregister_virtual_camera() -> Result<(), String> {
    for classes_root in ["Software\\Classes", "Software\\Classes\\WOW6432Node"] {
        for clsid in [NEW_CLSID, LEGACY_CLSID] {
            delete_registry_tree(&format!("{classes_root}\\CLSID\\{clsid}"));
        }
        for name in [
            NEW_CLSID,
            FILTER_NAME,
            OLD_FILTER_NAME,
            PREVIOUS_FILTER_NAME,
            VIRTUAL_FILTER_NAME,
        ] {
            delete_registry_tree(&format!(
                "{classes_root}\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{name}"
            ));
        }
    }
    Ok(())
}

/// Returns true when the camera is registered for the current user.
pub fn is_registered() -> bool {
    let path = format!("Software\\Classes\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{NEW_CLSID}");
    let wide_path = to_wide(&path);
    let mut key: HKEY = std::ptr::null_mut();
    let result = unsafe {
        windows_sys::Win32::System::Registry::RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide_path.as_ptr(),
            0,
            windows_sys::Win32::System::Registry::KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if result == 0 {
        unsafe {
            RegCloseKey(key);
        }
        true
    } else {
        false
    }
}

fn register_instance(
    classes_root: &str,
    key_name: &str,
    friendly: &str,
    clsid: &str,
) -> Result<(), String> {
    let instance = format!("{classes_root}\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{key_name}");
    reg_set_string(&instance, Some("FriendlyName"), friendly)?;
    reg_set_string(&instance, Some("CLSID"), clsid)?;
    reg_set_binary(&instance, Some("FilterData"), &FILTER_DATA)?;
    Ok(())
}

fn reg_set_string(path: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe {
        std::slice::from_raw_parts(
            wide.as_ptr() as *const u8,
            wide.len() * std::mem::size_of::<u16>(),
        )
    };
    reg_set(path, name, REG_SZ, bytes)
}

fn reg_set_binary(path: &str, name: Option<&str>, value: &[u8]) -> Result<(), String> {
    reg_set(path, name, REG_BINARY, value)
}

fn reg_set(path: &str, name: Option<&str>, kind: u32, data: &[u8]) -> Result<(), String> {
    let key = open_user_key(path)?;
    let wide_name = name.map(to_wide);
    let name_ptr = wide_name
        .as_ref()
        .map(|value| value.as_ptr())
        .unwrap_or(std::ptr::null());
    let result =
        unsafe { RegSetValueExW(key, name_ptr, 0, kind, data.as_ptr(), data.len() as u32) };
    unsafe {
        RegCloseKey(key);
    }
    if result == 0 {
        Ok(())
    } else {
        Err(format!(
            "Cannot write the virtual camera setting in HKCU\\{path} (error {result})."
        ))
    }
}

fn open_user_key(path: &str) -> Result<HKEY, String> {
    let wide_path = to_wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide_path.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Ok(key)
    } else {
        Err(format!(
            "Cannot open HKCU\\{path} for the virtual camera (error {result})."
        ))
    }
}

fn delete_registry_tree(path: &str) {
    let wide_path = to_wide(path);
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, wide_path.as_ptr());
    }
}

pub(crate) fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
