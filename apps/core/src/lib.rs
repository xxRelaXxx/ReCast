//! Shared pieces of ReCast: install paths, the virtual camera registration,
//! the user PATH entry and the local control channel that the `recast` CLI
//! uses to talk to the running desktop app.

pub mod control;
pub mod paths;
pub mod registry;
pub mod user_path;

pub use paths::{
    bin_dir, cli_path, install_dir, payload_file, APK_FILE, APP_FILE, BIN_DIR, CLI_FILE,
    FFMPEG_FILE,
};
pub use registry::{register_virtual_camera, unregister_virtual_camera};
pub use user_path::{add_to_user_path, remove_from_user_path};

/// Version of the whole project. Kept in sync with the crate versions.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
