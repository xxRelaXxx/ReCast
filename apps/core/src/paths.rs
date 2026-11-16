//! Where ReCast keeps its files.

use std::path::PathBuf;

/// Name of the desktop application inside the install folder.
pub const APP_FILE: &str = "recast-app.exe";
/// Name of the video decoder inside the install folder.
pub const FFMPEG_FILE: &str = "ffmpeg.exe";
/// Name of the phone application inside the install folder.
pub const APK_FILE: &str = "ReCastPhone.apk";
/// Folder inside the install folder that holds the command line tool.
pub const BIN_DIR: &str = "bin";
/// Name of the command line tool inside the bin folder.
pub const CLI_FILE: &str = "recast.exe";

/// `%LOCALAPPDATA%\ReCast`, the folder the launcher unpacks the runtime into.
pub fn install_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("ReCast"))
}

/// The folder that holds the command line tool. It is added to the user's
/// PATH so that `recast` works in every terminal.
pub fn bin_dir() -> Option<PathBuf> {
    Some(install_dir()?.join(BIN_DIR))
}

/// Full path of the command line tool.
pub fn cli_path() -> Option<PathBuf> {
    Some(bin_dir()?.join(CLI_FILE))
}

/// Full path of one of the files inside the install folder.
pub fn payload_file(name: &str) -> Option<PathBuf> {
    Some(install_dir()?.join(name))
}
