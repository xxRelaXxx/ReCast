//! `recast` - the command line interface of ReCast.
//!
//! The command talks to the running desktop application over the local
//! control channel and can also install and remove ReCast on this PC.

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use recast_core::control::{self, Command};

#[derive(Parser)]
#[command(
    name = "recast",
    version,
    about = "ReCast - use your Android phone as a webcam and microphone on Windows",
    long_about = "ReCast turns an Android phone into a webcam and microphone on Windows.\n\n\
                  Run it without arguments to start the desktop application. The other \
                  subcommands control the running application, or install and remove \
                  ReCast on this PC.",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command_>,
}

#[derive(Subcommand)]
enum Command_ {
    /// Start the desktop application (the default).
    Start,

    /// Show the state of the running application.
    Status,

    /// Connect the phone over USB: install the app, open the tunnel, start it.
    Usb,

    /// Change a setting of the running application.
    #[command(subcommand)]
    Set(SetArgs),

    /// Rotate the output of the running application.
    Rotate {
        /// Degrees to add: 90, 180 or 270.
        degrees: u32,
    },

    /// Switch the interface theme.
    Theme {
        /// mono, graphite or light.
        kind: ThemeKind,
    },

    /// Show the address and the PIN to type into the phone.
    Pair,

    /// Check the installation and report what is missing.
    Doctor,

    /// Register the virtual camera and add the command to the PATH.
    Install {
        /// Also add the firewall rule for the phone connection.
        #[arg(long)]
        firewall: bool,
    },

    /// Remove the virtual camera and the PATH entry.
    Uninstall,

    /// Open the folder with the installed files.
    Open {
        /// Which folder to open.
        #[arg(value_enum, default_value = "install")]
        target: OpenTarget,
    },
}

#[derive(Subcommand)]
enum SetArgs {
    /// Choose the camera of the phone.
    Camera {
        /// Camera id or name, for example 0 or "back".
        id: String,
    },
    /// Choose the stream resolution.
    Resolution {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Frames per second.
        #[arg(long, default_value_t = 30)]
        fps: u32,
    },
    /// Mute or unmute the phone microphone.
    Mic {
        /// on or off.
        state: OnOff,
    },
    /// Turn the phone torch on or off.
    Torch {
        /// on or off.
        state: OnOff,
    },
    /// Set the digital zoom of the phone camera.
    Zoom {
        /// Zoom factor, 1.0 to 8.0.
        value: f32,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum ThemeKind {
    Mono,
    Graphite,
    Light,
}

impl ThemeKind {
    fn as_str(self) -> &'static str {
        match self {
            ThemeKind::Mono => "mono",
            ThemeKind::Graphite => "graphite",
            ThemeKind::Light => "light",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OpenTarget {
    /// The folder with the installed files.
    Install,
    /// The folder with the logs.
    Logs,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("recast: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command.unwrap_or(Command_::Start) {
        Command_::Start => start_app(),
        Command_::Status => print_status(),
        Command_::Usb => send_command(Command::UsbSetup, "The USB setup has been started."),
        Command_::Set(args) => set(args),
        Command_::Rotate { degrees } => send_command(
            Command::Rotate { degrees },
            &format!("The output is rotated by {degrees} degrees."),
        ),
        Command_::Theme { kind } => send_command(
            Command::SetTheme {
                kind: kind.as_str().to_string(),
            },
            &format!("The theme is now {}.", kind.as_str()),
        ),
        Command_::Pair => print_pair(),
        Command_::Doctor => doctor(),
        Command_::Install { firewall } => install(firewall),
        Command_::Uninstall => uninstall(),
        Command_::Open { target } => open(target),
    }
}

fn start_app() -> Result<(), String> {
    let launcher = recast_core::paths::install_dir()
        .map(|dir| dir.join("recast.exe"))
        .filter(|path| path.is_file())
        .ok_or("ReCast is not installed yet. Run desktop\\ReCast.exe once.")?;

    std::process::Command::new(launcher)
        .spawn()
        .map_err(|e| format!("Cannot start ReCast: {e}"))?;
    println!("ReCast is starting.");
    Ok(())
}

fn print_status() -> Result<(), String> {
    let reply = control::send(&Command::Status)?;
    if !reply.ok {
        return Err(reply.error);
    }
    let status = &reply.data;

    let state = status
        .get("connected")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    println!("ReCast is running.");
    if state {
        let phone = status
            .get("phone_name")
            .and_then(|v| v.as_str())
            .unwrap_or("phone");
        println!("Phone:   {phone} (connected)");
    } else {
        println!("Phone:   not connected");
    }

    if let Some(address) = status.get("address").and_then(|v| v.as_str()) {
        println!("Address: {address}");
    }
    if let Some(pin) = status.get("pin").and_then(|v| v.as_str()) {
        println!("PIN:     {pin}");
    }
    if let Some(camera) = status.get("camera").and_then(|v| v.as_str()) {
        println!("Camera:  {camera}");
    }
    if let Some(resolution) = status.get("resolution").and_then(|v| v.as_str()) {
        println!("Stream:  {resolution}");
    }
    if let Some(latency) = status.get("latency_ms").and_then(|v| v.as_f64()) {
        println!("Latency: {latency:.0} ms");
    }
    if let Some(vcam) = status.get("virtual_camera").and_then(|v| v.as_bool()) {
        println!("Virtual camera: {}", if vcam { "on" } else { "off" });
    }
    Ok(())
}

fn print_pair() -> Result<(), String> {
    let reply = control::send(&Command::Status)?;
    if !reply.ok {
        return Err(reply.error);
    }
    let address = reply
        .data
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or("The application does not know its address yet.")?;
    let pin = reply
        .data
        .get("pin")
        .and_then(|v| v.as_str())
        .ok_or("The application does not know its PIN yet.")?;
    println!("{address}  PIN {pin}");
    Ok(())
}

fn set(args: SetArgs) -> Result<(), String> {
    match args {
        SetArgs::Camera { id } => {
            send_command(Command::SetCamera { id }, "The camera is switching.")
        }
        SetArgs::Resolution { width, height, fps } => send_command(
            Command::SetResolution { width, height, fps },
            &format!("The stream is now {width}x{height} at {fps} fps."),
        ),
        SetArgs::Mic { state } => send_command(
            Command::SetMic {
                enabled: state == OnOff::On,
            },
            "The phone microphone is updated.",
        ),
        SetArgs::Torch { state } => send_command(
            Command::SetTorch {
                enabled: state == OnOff::On,
            },
            "The torch is updated.",
        ),
        SetArgs::Zoom { value } => {
            if !(1.0..=8.0).contains(&value) {
                return Err("The zoom must be between 1.0 and 8.0.".to_string());
            }
            send_command(Command::SetZoom { value }, "The zoom is updated.")
        }
    }
}

fn send_command(command: Command, message: &str) -> Result<(), String> {
    let reply = control::send(&command)?;
    if !reply.ok {
        return Err(reply.error);
    }
    println!("{message}");
    Ok(())
}

fn doctor() -> Result<(), String> {
    let mut problems = 0;

    match recast_core::paths::install_dir() {
        Some(dir) if dir.is_dir() => {
            println!("[ok]   installed in {}", dir.display());
            for (label, name) in [
                ("desktop app", recast_core::paths::APP_FILE),
                ("video decoder", recast_core::paths::FFMPEG_FILE),
                ("phone app", recast_core::paths::APK_FILE),
            ] {
                let path = dir.join(name);
                if path.is_file() {
                    println!("[ok]   {label}: {}", path.display());
                } else {
                    println!("[miss] {label}: {} is missing", path.display());
                    problems += 1;
                }
            }
            match recast_core::paths::cli_path() {
                Some(path) if path.is_file() => {
                    println!("[ok]   command line: {}", path.display());
                }
                Some(path) => {
                    println!("[miss] command line: {} is missing", path.display());
                    problems += 1;
                }
                None => {
                    println!("[miss] command line: LOCALAPPDATA is not available");
                    problems += 1;
                }
            }
            for arch in ["x64", "x86"] {
                let path = dir.join("vcam").join(arch).join("softcam.dll");
                if path.is_file() {
                    println!("[ok]   virtual camera ({arch}): {}", path.display());
                } else {
                    println!(
                        "[miss] virtual camera ({arch}): {} is missing",
                        path.display()
                    );
                    problems += 1;
                }
            }
        }
        _ => {
            println!("[miss] ReCast is not installed. Run desktop\\ReCast.exe once.");
            problems += 1;
        }
    }

    if recast_core::registry::is_registered() {
        println!("[ok]   the virtual camera is registered for this user");
    } else {
        println!("[miss] the virtual camera is not registered. Run: recast install");
        problems += 1;
    }

    match recast_core::user_path::read_user_path() {
        Ok(path) => {
            let dir = recast_core::paths::install_dir()
                .map(|dir| dir.join("bin"))
                .map(|dir| dir.to_string_lossy().to_string())
                .unwrap_or_default();
            if path
                .split(';')
                .any(|entry| entry.trim().eq_ignore_ascii_case(dir.trim()))
            {
                println!("[ok]   the recast command is on the PATH");
            } else {
                println!("[miss] the recast command is not on the PATH. Run: recast install");
                problems += 1;
            }
        }
        Err(error) => {
            println!("[miss] the PATH cannot be read: {error}");
            problems += 1;
        }
    }

    match control::read_endpoint() {
        Some(_) => println!("[ok]   the desktop application is running"),
        None => println!("[warn] the desktop application is not running"),
    }

    if problems == 0 {
        println!("\nEverything is in place.");
        Ok(())
    } else {
        println!("\n{problems} item(s) need attention.");
        Ok(())
    }
}

fn install(firewall: bool) -> Result<(), String> {
    let dir =
        recast_core::paths::install_dir().ok_or("LOCALAPPDATA is not available on this PC.")?;
    if !dir.is_dir() {
        return Err("ReCast is not installed yet. Run desktop\\ReCast.exe once.".to_string());
    }

    recast_core::registry::register_virtual_camera(&dir)?;
    println!("The virtual camera is registered for this user.");

    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| format!("Cannot create {}: {e}", bin.display()))?;
    if recast_core::user_path::add_to_user_path(&bin)? {
        println!("{} was added to your PATH.", bin.display());
        println!("Open a new terminal to use the recast command.");
    } else {
        println!("{} is already on your PATH.", bin.display());
    }

    if firewall {
        println!("\nTo add the firewall rule, run ReCast.exe once as administrator.");
    }
    Ok(())
}

fn uninstall() -> Result<(), String> {
    recast_core::registry::unregister_virtual_camera()?;
    println!("The virtual camera registration was removed.");

    if let Some(dir) = recast_core::paths::install_dir() {
        let bin = dir.join("bin");
        if recast_core::user_path::remove_from_user_path(&bin)? {
            println!("{} was removed from your PATH.", bin.display());
        }
    }
    println!("\nDelete %LOCALAPPDATA%\\ReCast to remove the files as well.");
    Ok(())
}

fn open(target: OpenTarget) -> Result<(), String> {
    let dir = recast_core::paths::install_dir().ok_or("LOCALAPPDATA is not available.")?;
    let path = match target {
        OpenTarget::Install => dir.clone(),
        OpenTarget::Logs => dir.join("launcher.log"),
    };
    if !path.exists() {
        return Err(format!("{} does not exist.", path.display()));
    }
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(&dir).to_path_buf()
    };
    std::process::Command::new("explorer.exe")
        .arg(&folder)
        .spawn()
        .map_err(|e| format!("Cannot open the folder: {e}"))?;
    println!("{}", folder.display());
    Ok(())
}
