// Rewebcam single-file launcher.
//
// The compiled runtime is embedded in this executable. On launch it is
// unpacked into %LOCALAPPDATA%\Rewebcam, the virtual camera is registered for
// the current user, and the desktop app starts. A one-time elevated helper
// removes the old camera entry and adds the Windows Firewall rule.

#![windows_subsystem = "windows"]

use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_WRITE, REG_BINARY, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, INFINITE,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, MessageBoxW, SetForegroundWindow, ShowWindow, IDYES, MB_ICONERROR,
    MB_ICONQUESTION, MB_YESNO, SW_RESTORE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NEW_CLSID: &str = "{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}";
const LEGACY_CLSID: &str = "{AEF3B972-5FA5-4647-9571-358EB472BC9E}";
const CAMERA_CATEGORY: &str = "{860BB310-5D01-11D0-BD3B-00A0C911CE86}";
const FILTER_NAME: &str = "Rewebcam Virtual Camera";
const OLD_FILTER_NAME: &str = "DirectShow Softcam";

// REGFILTER2 blob for the single video output pin.
const FILTER_DATA: [u8; 88] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x70, 0x69, 0x33, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x74, 0x79, 0x33, 0x00, 0x00, 0x00, 0x00,
    0x38, 0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x76, 0x69, 0x64, 0x73, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const REWEBCAM_EXE: &[u8] = include_bytes!("../payload/rewebcam.exe");
const FFMPEG_EXE: &[u8] = include_bytes!("../payload/ffmpeg.exe");
const PHONE_APK: &[u8] = include_bytes!("../payload/RewebcamPhone.apk");
const SOFTCAM_X64: &[u8] = include_bytes!("../payload/vcam/x64/softcam.dll");
const SOFTCAM_X86: &[u8] = include_bytes!("../payload/vcam/x86/softcam.dll");

fn main() {
    std::panic::set_hook(Box::new(|info| {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            log_line(
                &PathBuf::from(local_app_data).join("Rewebcam"),
                &format!("PANIC: {info}"),
            );
        }
    }));

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|arg| arg == "--setup-elevated") {
        let code = elevated_setup(&args);
        std::process::exit(code);
    }

    let skip_setup_prompt = args.iter().any(|arg| arg == "--skip-setup-prompt");
    if let Err(error) = run(skip_setup_prompt) {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            log_line(
                &PathBuf::from(local_app_data).join("Rewebcam"),
                &format!("ERROR: {error}"),
            );
        }
        message_box(
            &format!("Rewebcam could not start.\n\n{error}"),
            MB_ICONERROR,
        );
    }
}

fn run(skip_setup_prompt: bool) -> Result<(), String> {
    let local_app_data =
        env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not available on this PC.")?;
    let install_dir = PathBuf::from(local_app_data).join("Rewebcam");
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Cannot create {}: {e}", install_dir.display()))?;
    let _ = fs::write(
        install_dir.join("launcher.log"),
        format!("Rewebcam launcher {VERSION}\n"),
    );
    log_line(&install_dir, "Starting the launcher.");

    let version_marker = install_dir.join(".payload-version");
    let payload_current = fs::read_to_string(&version_marker)
        .map(|value| value.trim() == VERSION)
        .unwrap_or(false);
    let app_ready = install_dir.join("rewebcam.exe").is_file();

    if !payload_current || !app_ready {
        log_line(&install_dir, "Installing the embedded runtime.");
        if is_app_running(&install_dir) {
            terminate_app(&install_dir);
        }
        extract_payload(&install_dir)?;
    }

    register_virtual_camera(&install_dir)?;
    log_line(&install_dir, "Virtual camera registered for the current user.");

    // Remove the pre-rebrand camera entry and add the firewall rule once.
    let setup_marker = install_dir.join(".setup-complete");
    if !setup_marker.is_file() {
        let old_entry = old_machine_entry_exists();
        let firewall = firewall_rule_exists();
        if !skip_setup_prompt && (old_entry || !firewall) {
            let text = "Rewebcam can finish setup for this PC:\n\n\
                        \u{2022}  remove the old \"DirectShow Softcam\" entry from the camera list\n\
                        \u{2022}  allow Rewebcam through Windows Firewall for phone connections\n\n\
                        This needs administrator approval. Continue?\n\n\
                        Choose No to skip. The virtual camera still works for this user, \
                        and Windows will ask again when the app opens the network port.";
            if message_box(text, MB_YESNO | MB_ICONQUESTION) == IDYES {
                let _ = run_elevated_helper(&install_dir);
            }
            let _ = fs::write(&setup_marker, b"1\r\n");
        } else {
            let _ = fs::write(&setup_marker, b"1\r\n");
        }
    }

    if is_app_running(&install_dir) {
        log_line(&install_dir, "Rewebcam is already running; focusing its window.");
        focus_app_window();
        return Ok(());
    }

    log_line(&install_dir, "Starting rewebcam.exe.");
    Command::new(install_dir.join("rewebcam.exe"))
        .current_dir(&install_dir)
        .spawn()
        .map_err(|e| format!("Cannot start rewebcam.exe: {e}"))?;
    log_line(&install_dir, "Launcher finished.");

    Ok(())
}

fn extract_payload(install_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(install_dir.join("vcam").join("x64"))
        .map_err(|e| format!("Cannot create the camera folder: {e}"))?;
    fs::create_dir_all(install_dir.join("vcam").join("x86"))
        .map_err(|e| format!("Cannot create the camera folder: {e}"))?;

    write_if_changed(&install_dir.join("rewebcam.exe"), REWEBCAM_EXE)?;
    log_line(install_dir, "rewebcam.exe is ready.");
    write_if_changed(&install_dir.join("ffmpeg.exe"), FFMPEG_EXE)?;
    log_line(install_dir, "ffmpeg.exe is ready.");
    write_if_changed(&install_dir.join("RewebcamPhone.apk"), PHONE_APK)?;
    log_line(install_dir, "RewebcamPhone.apk is ready.");
    write_if_changed(
        &install_dir.join("vcam").join("x64").join("softcam.dll"),
        SOFTCAM_X64,
    )?;
    write_if_changed(
        &install_dir.join("vcam").join("x86").join("softcam.dll"),
        SOFTCAM_X86,
    )?;

    write_file(&install_dir.join(".payload-version"), VERSION.as_bytes())?;
    log_line(install_dir, "Runtime installed.");
    Ok(())
}

fn write_if_changed(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Ok(existing) = fs::read(path) {
        if existing == data {
            return Ok(());
        }
    }
    write_file(path, data)
}

fn write_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("new");
    fs::write(&temp, data).map_err(|e| format!("Cannot write {}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!(
            "Cannot replace {}. Close Rewebcam and any app that uses the camera, then try again.\n\n{e}",
            path.display()
        )
    })
}

fn log_line(install_dir: &Path, message: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(install_dir.join("launcher.log"))
    {
        let _ = writeln!(file, "{message}");
    }
}

fn register_virtual_camera(install_dir: &Path) -> Result<(), String> {
    let x64 = install_dir.join("vcam").join("x64").join("softcam.dll");
    let x86 = install_dir.join("vcam").join("x86").join("softcam.dll");
    let x64 = x64.to_string_lossy().to_string();
    let x86 = x86.to_string_lossy().to_string();

    for (classes_root, dll) in [
        ("Software\\Classes", x64.as_str()),
        ("Software\\Classes\\WOW6432Node", x86.as_str()),
    ] {
        delete_registry_tree(&format!(
            "{classes_root}\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{OLD_FILTER_NAME}"
        ));

        for (clsid, name) in [
            (NEW_CLSID, FILTER_NAME),
            (LEGACY_CLSID, OLD_FILTER_NAME),
        ] {
            let base = format!("{classes_root}\\CLSID\\{clsid}");
            reg_set_string(&base, None, name)?;
            reg_set_string(&format!("{base}\\InprocServer32"), None, dll)?;
            reg_set_string(&format!("{base}\\InprocServer32"), Some("ThreadingModel"), "Both")?;
        }

        let instance = format!(
            "{classes_root}\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{FILTER_NAME}"
        );
        reg_set_string(&instance, Some("FriendlyName"), FILTER_NAME)?;
        reg_set_string(&instance, Some("CLSID"), NEW_CLSID)?;
        reg_set_binary(&instance, Some("FilterData"), &FILTER_DATA)?;
    }

    Ok(())
}

fn reg_set_string(path: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe {
        std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * std::mem::size_of::<u16>())
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
    let result = unsafe {
        RegSetValueExW(
            key,
            name_ptr,
            0,
            kind,
            data.as_ptr(),
            data.len() as u32,
        )
    };
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

fn old_machine_entry_exists() -> bool {
    let key = format!(
        "HKLM\\SOFTWARE\\Classes\\CLSID\\{CAMERA_CATEGORY}\\Instance\\{OLD_FILTER_NAME}"
    );
    command_ok("reg", &["query", &key])
}

fn firewall_rule_exists() -> bool {
    command_ok(
        "netsh",
        &["advfirewall", "firewall", "show", "rule", "name=Rewebcam"],
    )
}

fn run_elevated_helper(install_dir: &Path) -> Result<(), String> {
    let current = env::current_exe().map_err(|e| format!("Cannot locate the launcher: {e}"))?;
    let app = install_dir.join("rewebcam.exe");
    let parameters = format!("--setup-elevated \"{}\"", app.display());

    let verb = to_wide("runas");
    let file = to_wide(&current.to_string_lossy());
    let params = to_wide(&parameters);

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.hwnd = std::ptr::null_mut();
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = 0; // SW_HIDE

    let started: BOOL = unsafe { ShellExecuteExW(&mut info) };
    if started == 0 {
        // The user cancelled the UAC prompt.
        return Ok(());
    }

    if !info.hProcess.is_null() {
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

fn elevated_setup(args: &[String]) -> i32 {
    let app = args
        .iter()
        .position(|arg| arg == "--setup-elevated")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|path| path.join("Rewebcam").join("rewebcam.exe"))
        });

    for view in ["", "WOW6432Node\\"] {
        let key = format!(
            "HKLM\\SOFTWARE\\Classes\\{view}CLSID\\{CAMERA_CATEGORY}\\Instance\\{OLD_FILTER_NAME}"
        );
        let _ = command_ok("reg", &["delete", &key, "/f"]);
    }

    if !firewall_rule_exists() {
        let _ = command_ok(
            "netsh",
            &["advfirewall", "firewall", "delete", "rule", "name=Rewebcam"],
        );
        let Some(app) = app else {
            return 1;
        };
        let program = format!("program=\"{}\"", app.display());
        let added = command_ok(
            "netsh",
            &[
                "advfirewall",
                "firewall",
                "add",
                "rule",
                "name=Rewebcam",
                "dir=in",
                "action=allow",
                &program,
                "protocol=TCP",
                "localport=47777",
                "profile=private,public",
                "enable=yes",
            ],
        );
        if !added {
            return 1;
        }
    }

    0
}

fn command_ok(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn is_app_running(install_dir: &Path) -> bool {
    !app_pids(install_dir).is_empty()
}

fn app_pids(install_dir: &Path) -> Vec<u32> {
    // Compare full paths, not image names: "Rewebcam.exe" (this launcher) and
    // "rewebcam.exe" (the app) are the same name to case-insensitive Windows.
    let app_path = install_dir.join("rewebcam.exe").to_string_lossy().to_lowercase();

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut pids = Vec::new();
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut found = unsafe { Process32FirstW(snapshot, &mut entry) };
    while found != 0 {
        if process_path(entry.th32ProcessID)
            .map(|path| path.to_lowercase() == app_path)
            .unwrap_or(false)
        {
            pids.push(entry.th32ProcessID);
        }
        found = unsafe { Process32NextW(snapshot, &mut entry) };
    }

    unsafe {
        CloseHandle(snapshot);
    }
    pids
}

fn process_path(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }

    let mut buffer = [0u16; 32768];
    let mut length = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
    unsafe {
        CloseHandle(process);
    }
    if ok == 0 {
        return None;
    }
    Some(normalize_process_path(&String::from_utf16_lossy(
        &buffer[..length as usize],
    )))
}

fn normalize_process_path(path: &str) -> String {
    let path = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    };
    path.to_lowercase()
}

fn terminate_app(install_dir: &Path) {
    let pids = app_pids(install_dir);
    if pids.is_empty() {
        return;
    }
    for pid in pids {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    std::thread::sleep(std::time::Duration::from_millis(700));
}

fn focus_app_window() {
    let title = to_wide("Rewebcam");
    let window: HWND = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !window.is_null() {
        unsafe {
            ShowWindow(window, SW_RESTORE);
            SetForegroundWindow(window);
        }
    }
}

fn message_box(text: &str, flags: u32) -> i32 {
    let text = to_wide(text);
    let caption = to_wide("Rewebcam");
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), flags) }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
