//! Native window chrome for the borderless ReCast window.
//!
//! egui draws the title bar and the window buttons. This module applies the
//! Windows 11 rounded corners and the dark border, and implements resizing
//! from the window edges, which a borderless window does not get for free.

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

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

/// Starts a native resize when the pointer presses an edge or a corner.
pub fn resize_edges(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let Some(pos) = ctx.pointer_hover_pos() else {
        return;
    };
    let rect = ctx.screen_rect();
    if !rect.contains(pos) {
        return;
    }

    let border = 5.0;
    let left = pos.x <= rect.left() + border;
    let right = pos.x >= rect.right() - border;
    let top = pos.y <= rect.top() + border;
    let bottom = pos.y >= rect.bottom() - border;

    use egui::ResizeDirection;
    use egui::ViewportCommand;

    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => Some((
            ResizeDirection::NorthWest,
            egui::CursorIcon::ResizeNorthWest,
        )),
        (_, true, true, _) => Some((
            ResizeDirection::NorthEast,
            egui::CursorIcon::ResizeNorthEast,
        )),
        (true, _, _, true) => Some((
            ResizeDirection::SouthWest,
            egui::CursorIcon::ResizeSouthWest,
        )),
        (_, true, _, true) => Some((
            ResizeDirection::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        )),
        (true, _, _, _) => Some((ResizeDirection::West, egui::CursorIcon::ResizeHorizontal)),
        (_, true, _, _) => Some((ResizeDirection::East, egui::CursorIcon::ResizeHorizontal)),
        (_, _, true, _) => Some((ResizeDirection::North, egui::CursorIcon::ResizeVertical)),
        (_, _, _, true) => Some((ResizeDirection::South, egui::CursorIcon::ResizeVertical)),
        _ => None,
    };

    let Some((direction, cursor)) = direction else {
        return;
    };
    ctx.set_cursor_icon(cursor);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}
