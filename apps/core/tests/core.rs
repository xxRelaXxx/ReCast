//! Tests for the parts of the core crate that can be checked without a
//! running desktop application.

use recast_core::paths;
use recast_core::user_path;

#[test]
fn install_dir_is_under_local_appdata() {
    let dir = paths::install_dir().expect("LOCALAPPDATA is set in the test environment");
    assert!(dir.ends_with("ReCast"));
    assert!(dir.to_string_lossy().contains("ReCast"));
}

#[test]
fn the_cli_lives_in_the_bin_folder() {
    let cli = paths::cli_path().expect("LOCALAPPDATA is set in the test environment");
    assert!(cli.ends_with("recast.exe"));
    assert!(
        cli.parent().and_then(|p| p.file_name()) == Some(std::ffi::OsStr::new("bin")),
        "the CLI must live in the bin folder, got {}",
        cli.display()
    );
}

#[test]
fn payload_files_are_named_consistently() {
    assert_eq!(paths::APP_FILE, "recast-app.exe");
    assert_eq!(paths::FFMPEG_FILE, "ffmpeg.exe");
    assert_eq!(paths::APK_FILE, "ReCastPhone.apk");
}

#[test]
fn the_path_helper_ignores_case_and_trailing_slashes() {
    let path = "C:\\Tools\\Bin;C:\\Program Files\\recast;C:\\Windows";
    assert!(user_path::path_contains(path, "c:\\tools\\bin"));
    assert!(user_path::path_contains(path, "C:\\Tools\\Bin\\"));
    assert!(user_path::path_contains(path, "C:\\PROGRAM FILES\\RECAST"));
    assert!(!user_path::path_contains(path, "C:\\Tools"));
    assert!(!user_path::path_contains(path, "D:\\Other"));
}

#[test]
fn the_user_path_can_be_read() {
    // Reading must never fail, even on a machine without a PATH entry.
    let path = user_path::read_user_path().expect("reading the PATH must not fail");
    assert!(!path.contains('\0'));
}
