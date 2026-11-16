//! The installation itself: unpacking the runtime, registering the virtual
//! camera and creating the shortcuts.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};

pub const APP_EXE: &[u8] = include_bytes!("../payload/recast-app.exe");
pub const CLI_EXE: &[u8] = include_bytes!("../payload/recast-cli.exe");
pub const FFMPEG_EXE: &[u8] = include_bytes!("../payload/ffmpeg.exe");
pub const PHONE_APK: &[u8] = include_bytes!("../payload/ReCastPhone.apk");
pub const SOFTCAM_X64: &[u8] = include_bytes!("../payload/vcam/x64/softcam.dll");
pub const SOFTCAM_X86: &[u8] = include_bytes!("../payload/vcam/x86/softcam.dll");

/// The choices the user makes in the installer.
#[derive(Clone)]
pub struct Options {
    pub install_dir: PathBuf,
    pub desktop_icon: bool,
    pub start_menu: bool,
    pub autostart: bool,
}

/// Reports the running step and how far the installation is.
pub type Progress<'a> = &'a mut dyn FnMut(&str, f32);

/// Writes the runtime into the chosen folder and sets everything up.
pub fn install(options: &Options, progress: Progress<'_>) -> Result<(), String> {
    let dir = &options.install_dir;

    progress("Preparing the folder", 0.04);
    fs::create_dir_all(dir.join("vcam").join("x64"))
        .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    fs::create_dir_all(dir.join("vcam").join("x86"))
        .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    fs::create_dir_all(dir.join("bin"))
        .map_err(|e| format!("Cannot create {}: {e}", dir.join("bin").display()))?;

    progress("Copying the desktop app", 0.16);
    write_if_changed(&dir.join("recast-app.exe"), APP_EXE)?;

    progress("Copying the camera filter", 0.34);
    write_if_changed(&dir.join("vcam").join("x64").join("softcam.dll"), SOFTCAM_X64)?;
    write_if_changed(&dir.join("vcam").join("x86").join("softcam.dll"), SOFTCAM_X86)?;

    progress("Copying the phone app and the decoder", 0.52);
    write_if_changed(&dir.join("ReCastPhone.apk"), PHONE_APK)?;
    write_if_changed(&dir.join("ffmpeg.exe"), FFMPEG_EXE)?;

    progress("Copying the command line tool", 0.64);
    write_if_changed(&dir.join("bin").join("recast.exe"), CLI_EXE)?;

    progress("Registering the virtual camera", 0.76);
    recast_core::registry::register_virtual_camera(dir)?;

    progress("Creating the shortcuts", 0.88);
    if options.desktop_icon {
        let desktop = desktop_dir().ok_or("The desktop folder cannot be found.")?;
        create_shortcut(&desktop.join("ReCast.lnk"), &dir.join("recast-app.exe"))?;
    }
    progress("Creating the start menu entry", 0.9);
    if options.start_menu {
        let programs = start_menu_dir().ok_or("The start menu folder cannot be found.")?;
        create_shortcut(&programs.join("ReCast.lnk"), &dir.join("recast-app.exe"))?;
    }

    if options.autostart {
        progress("Setting up the automatic start", 0.94);
        set_autostart(&dir.join("recast-app.exe"))?;
    }

    progress("Finishing", 1.0);
    Ok(())
}

/// Starts the installed app.
pub fn launch(dir: &Path) -> Result<(), String> {
    let exe = dir.join("recast-app.exe");
    std::process::Command::new(exe)
        .current_dir(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot start ReCast: {e}"))
}

/// Removes the installer and the files it left behind, so that only the
/// installed program stays.
pub fn cleanup(installer: &Path, install_dir: &Path) {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(current) = std::env::current_exe() {
        files.push(current);
    }
    files.push(installer.to_path_buf());

    // The installer cannot delete itself while it runs, so a small helper
    // does it after this process is gone.
    let command = format!(
        "ping -n 3 127.0.0.1 > nul & del /f /q \"{}\"",
        files[0].display()
    );
    let _ = Command::new("cmd")
        .args(["/c", &command])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    // Everything that is not part of the installed program.
    let keep = [
        "recast-app.exe",
        "ffmpeg.exe",
        "ReCastPhone.apk",
        "vcam",
        "bin",
    ];
    if let Ok(entries) = fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if keep.contains(&name.as_ref()) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn write_if_changed(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Ok(existing) = fs::read(path) {
        if existing == data {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, data).map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

/// Creates a Windows shortcut with the built-in script host.
fn create_shortcut(link: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    let script = format!(
        "$s = New-Object -ComObject WScript.Shell; \
         $l = $s.CreateShortcut('{}'); \
         $l.TargetPath = '{}'; \
         $l.WorkingDirectory = '{}'; \
         $l.IconLocation = '{}'; \
         $l.Description = 'ReCast - use your phone as a webcam'; \
         $l.Save()",
        link.display(),
        target.display(),
        target.parent().unwrap_or(target).display(),
        target.display(),
    );
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|e| format!("Cannot create the shortcut: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Cannot create the shortcut {}: {}",
            link.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// Adds ReCast to the programs that Windows starts automatically.
fn set_autostart(exe: &Path) -> Result<(), String> {
    let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let key = wide("ReCast");
    let value = wide(exe.to_str().unwrap_or_default());
    unsafe {
        let mut handle = std::ptr::null_mut();
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        );
        if result != 0 {
            return Err(format!("Cannot open the autostart entry (error {result})."));
        }
        let result = RegSetValueExW(
            handle,
            key.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        );
        RegCloseKey(handle);
        if result != 0 {
            return Err(format!("Cannot write the autostart entry (error {result})."));
        }
    }
    Ok(())
}

fn desktop_dir() -> Option<PathBuf> {
    known_folder("Desktop")
}

fn start_menu_dir() -> Option<PathBuf> {
    known_folder("Programs")
}

fn known_folder(name: &str) -> Option<PathBuf> {
    let user_profile = std::env::var_os("USERPROFILE")?;
    let folder = match name {
        "Desktop" => "Desktop",
        _ => r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs",
    };
    Some(PathBuf::from(user_profile).join(folder))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
