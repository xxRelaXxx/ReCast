use libloading::Library;
use std::ffi::{c_void, CString};
use std::path::PathBuf;

type CreateFn = unsafe extern "C" fn(i32, i32, f32) -> *mut c_void;
type SendFn = unsafe extern "C" fn(*mut c_void, *const c_void);
type DeleteFn = unsafe extern "C" fn(*mut c_void);
type IsConnectedFn = unsafe extern "C" fn(*mut c_void) -> bool;

pub struct Softcam {
    _lib: Library,
    camera: *mut c_void,
    send_fn: SendFn,
    delete_fn: DeleteFn,
    is_connected_fn: IsConnectedFn,
    pub width: u32,
    pub height: u32,
}

impl Softcam {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let path = locate_dll().ok_or_else(|| {
            "softcam.dll not found. Start Rewebcam through desktop\\Rewebcam.exe once to install the virtual camera.".to_string()
        })?;
        let lib = unsafe { Library::new(&path) }
            .map_err(|e| format!("cannot load {}: {e}", path.display()))?;

        unsafe {
            let create: CreateFn = *lib
                .get::<CreateFn>(b"scCreateCamera")
                .map_err(|e| format!("scCreateCamera: {e}"))?;
            let send: SendFn = *lib
                .get::<SendFn>(b"scSendFrame")
                .map_err(|e| format!("scSendFrame: {e}"))?;
            let delete: DeleteFn = *lib
                .get::<DeleteFn>(b"scDeleteCamera")
                .map_err(|e| format!("scDeleteCamera: {e}"))?;
            let is_connected: IsConnectedFn = *lib
                .get::<IsConnectedFn>(b"scIsConnected")
                .map_err(|e| format!("scIsConnected: {e}"))?;

            let camera = create(width as i32, height as i32, 0.0);
            if camera.is_null() {
                return Err(
                    "scCreateCamera failed. Another virtual camera sender may already be running."
                        .to_string(),
                );
            }

            Ok(Self {
                _lib: lib,
                camera,
                send_fn: send,
                delete_fn: delete,
                is_connected_fn: is_connected,
                width,
                height,
            })
        }
    }

    pub fn send_rgb(&self, rgb: &[u8]) {
        let expected = self.width as usize * self.height as usize * 3;
        if rgb.len() < expected {
            return;
        }
        unsafe { (self.send_fn)(self.camera, rgb.as_ptr() as *const c_void) }
    }

    pub fn is_connected(&self) -> bool {
        unsafe { (self.is_connected_fn)(self.camera) }
    }
}

impl Drop for Softcam {
    fn drop(&mut self) {
        if !self.camera.is_null() {
            unsafe { (self.delete_fn)(self.camera) }
            self.camera = std::ptr::null_mut();
        }
    }
}

fn locate_dll() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REWEBCAM_SOFTCAM_DLL") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("softcam.dll");
            if p.is_file() {
                return Some(p);
            }
            let p = dir.join("vcam").join("x64").join("softcam.dll");
            if p.is_file() {
                return Some(p);
            }
        }
    }

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local)
            .join("Rewebcam")
            .join("vcam")
            .join("x64")
            .join("softcam.dll");
        if p.is_file() {
            return Some(p);
        }
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
        .join("vcam")
        .join("x64")
        .join("softcam.dll");
    if dev.is_file() {
        return Some(dev);
    }

    None
}

#[allow(dead_code)]
pub fn dll_display_name() -> String {
    locate_dll()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| CString::new("not found").unwrap().to_string_lossy().into_owned())
}
