//! Native window chrome for the borderless installer window.
//!
//! The window has no title bar, so Windows will not move or resize it unless
//! the hit test says the top band is a caption and the edges are borders.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

/// Height of the custom title band. The close button sits at the right end.
pub const TITLE_BAR_HEIGHT: i32 = 56;
/// Width reserved for the close button, which must stay clickable.
pub const CLOSE_BUTTON_WIDTH: i32 = 52;
/// Invisible resize margin inside the client area.
pub const RESIZE_MARGIN: i32 = 8;

#[allow(non_snake_case)]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        attribute: u32,
        value: *const core::ffi::c_void,
        size: u32,
    ) -> i32;
}

const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

/// HWND of the installer window, used as the owner of the folder dialog.
pub fn native_hwnd(frame: &eframe::Frame) -> Option<isize> {
    let handle = frame.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
        _ => None,
    }
}

/// Opens the Windows folder dialog, rooted at the desktop so every drive is listed.
/// `initial` is the folder the dialog starts in. Cancel returns None.
pub fn pick_folder(owner: isize, initial: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::ffi::c_void;
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_EDITBOX, BIF_NEWDIALOGSTYLE,
        BIF_RETURNONLYFSDIRS, BROWSEINFOW,
    };

    let init = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    // 0 = S_OK, 1 = S_FALSE (already initialized). Anything else still tries the dialog.
    let _ = init;

    let title: Vec<u16> = "Choose the folder where ReCast will be installed\0"
        .encode_utf16()
        .collect();
    let mut initial_wide: Vec<u16> = initial.to_string_lossy().encode_utf16().collect();
    initial_wide.push(0);
    let mut display = [0u16; 260];
    let info = BROWSEINFOW {
        hwndOwner: owner as *mut c_void,
        // A null root is the desktop, which lists every drive (C:, X:, and the rest).
        pidlRoot: std::ptr::null_mut(),
        pszDisplayName: display.as_mut_ptr(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
        lpfn: Some(browse_callback),
        lParam: initial_wide.as_ptr() as isize,
        iImage: 0,
    };
    let pidl = unsafe { SHBrowseForFolderW(&info) };
    if pidl.is_null() {
        return None;
    }
    let mut buffer = [0u16; 520];
    let ok = unsafe { SHGetPathFromIDListW(pidl, buffer.as_mut_ptr()) };
    unsafe { CoTaskMemFree(pidl as *const c_void) };
    if ok == 0 {
        return None;
    }
    let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    let path = String::from_utf16_lossy(&buffer[..len]);
    if path.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(path))
    }
}

unsafe extern "system" fn browse_callback(
    hwnd: *mut core::ffi::c_void,
    msg: u32,
    _lparam: isize,
    data: isize,
) -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    // BFFM_INITIALIZED = 1, BFFM_SETSELECTIONW = WM_USER + 103.
    if msg == 1 && data != 0 {
        unsafe {
            SendMessageW(hwnd, 0x0400 + 103, 1, data);
        }
    }
    0
}

static FRAME_READY: AtomicBool = AtomicBool::new(false);
static PREVIOUS_PROC: AtomicIsize = AtomicIsize::new(0);

/// Lets the borderless window move from the title band and resize from the edges.
pub fn enable_move_and_resize(hwnd: isize) {
    use std::ffi::c_void;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowLongPtrW, SetWindowLongW, SetWindowPos, GetWindowLongW, GWL_STYLE, GWLP_WNDPROC,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_MAXIMIZEBOX,
        WS_THICKFRAME,
    };

    if hwnd == 0 || FRAME_READY.swap(true, Ordering::SeqCst) {
        return;
    }
    let hwnd = hwnd as *mut c_void;
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let style = style | WS_THICKFRAME | WS_MAXIMIZEBOX;
        SetWindowLongW(hwnd, GWL_STYLE, style as i32);
        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, installer_wndproc as *const () as isize);
        PREVIOUS_PROC.store(previous, Ordering::SeqCst);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

unsafe extern "system" fn installer_wndproc(
    hwnd: *mut core::ffi::c_void,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GetClientRect, GetWindowRect, IsZoomed, HTBOTTOM,
        HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT, WM_NCCALCSIZE, WM_NCHITTEST,
    };

    if msg == WM_NCCALCSIZE && wparam != 0 {
        // Keep the client area flush with the window so no system border is drawn.
        // A maximized window uses the work area, which leaves the taskbar visible.
        if unsafe { IsZoomed(hwnd) } != 0 {
            use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
            let mut work = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let ok = unsafe {
                SystemParametersInfoW(
                    0x0030,
                    0,
                    &mut work as *mut RECT as *mut core::ffi::c_void,
                    0,
                )
            };
            if ok != 0 {
                let params = unsafe { &mut *(lparam as *mut [RECT; 3]) };
                params[0] = work;
            }
        }
        return 0;
    }

    if msg == WM_NCHITTEST {
        let point = windows_sys::Win32::Foundation::POINT {
            x: (lparam as i16) as i32,
            y: ((lparam >> 16) as i16) as i32,
        };
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut window = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            GetWindowRect(hwnd, &mut window);
            GetClientRect(hwnd, &mut rect);
        }
        let width = rect.right;
        let height = rect.bottom;
        // NCCALCSIZE makes the client origin the window origin, so this is the client point.
        let x = point.x - window.left;
        let y = point.y - window.top;
        let margin = RESIZE_MARGIN;
        let left = x < margin;
        let right = x >= width - margin;
        let top = y < margin;
        let bottom = y >= height - margin;
        if top && left {
            return HTTOPLEFT as isize;
        }
        if top && right {
            return HTTOPRIGHT as isize;
        }
        if bottom && left {
            return HTBOTTOMLEFT as isize;
        }
        if bottom && right {
            return HTBOTTOMRIGHT as isize;
        }
        if left {
            return HTLEFT as isize;
        }
        if right {
            return HTRIGHT as isize;
        }
        if top {
            return HTTOP as isize;
        }
        if bottom {
            return HTBOTTOM as isize;
        }
        // The close button stays a normal click. The rest of the title band moves the window.
        if y < TITLE_BAR_HEIGHT && x < width - CLOSE_BUTTON_WIDTH {
            return HTCAPTION as isize;
        }
        return HTCLIENT as isize;
    }

    let previous = PREVIOUS_PROC.load(Ordering::Relaxed);
    if previous == 0 {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let previous: unsafe extern "system" fn(*mut core::ffi::c_void, u32, usize, isize) -> isize =
        unsafe { std::mem::transmute(previous) };
    unsafe { CallWindowProcW(Some(previous), hwnd, msg, wparam, lparam) }
}

/// Applies the native window attributes. Returns true once it has run.
pub fn apply_native_chrome(frame: &eframe::Frame) -> bool {
    let Ok(handle) = frame.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return true;
    };
    let hwnd = win32.hwnd.get();

    let dark: u32 = 1;
    let round: u32 = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
    true
}
