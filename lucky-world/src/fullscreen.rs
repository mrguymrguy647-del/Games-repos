//! Fullscreen, asked of the window system directly: Duck Forge opens the
//! window and keeps it to itself, with no fullscreen call (ISSUES.md), so
//! the game finds its own window by title ("LUCKY WORLD") and makes it a
//! borderless window over the whole screen, or back.
//!
//! Windows: `SetWindowLongPtrW` + `SetWindowPos` on the monitor's rectangle.
//! Linux (X11, as Duck Forge's windows are): the `_NET_WM_STATE_FULLSCREEN`
//! message to the window manager. Without a window (headless), nothing.

use std::sync::Mutex;

/// The wanted state, and the state last applied.
static WANT: Mutex<(bool, Option<bool>)> = Mutex::new((false, None));

/// Asks for fullscreen on or off; applied on the next frame with a window.
pub fn request(on: bool) {
    if let Ok(mut w) = WANT.lock() {
        w.0 = on;
    }
}

/// Called each frame: does what was asked once the window exists.
/// Returns an error once if the window system refused.
pub fn apply(headless: bool) -> Option<String> {
    if headless {
        return None;
    }
    let mut w = WANT.lock().ok()?;
    if w.1 == Some(w.0) {
        return None;
    }
    let on = w.0;
    match platform::set(on) {
        Ok(true) => {
            w.1 = Some(on);
            None
        }
        // The window isn't there yet: try next frame.
        Ok(false) => None,
        Err(e) => {
            w.1 = Some(on);
            Some(e)
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    /// The window's style, extended style, place and whether it was
    /// maximized, before going full screen.
    static SAVED: std::sync::Mutex<Option<(isize, isize, [i32; 4], bool)>> = std::sync::Mutex::new(None);

    fn window() -> HWND {
        let title: Vec<u16> = "LUCKY WORLD\0".encode_utf16().collect();
        unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
    }

    /// Full screen as a borderless window over the whole monitor (the
    /// taskbar included). A maximized window is restored first: Windows
    /// keeps a maximized window inside the work area, above the taskbar.
    pub fn set(on: bool) -> Result<bool, String> {
        let hwnd = window();
        if hwnd.is_null() {
            return Ok(false);
        }
        unsafe {
            if on {
                let zoomed = IsZoomed(hwnd) != 0;
                if zoomed {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut r);
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if let Ok(mut s) = SAVED.lock() {
                    if s.is_none() {
                        *s = Some((style, ex, [r.left, r.top, r.right, r.bottom], zoomed));
                    }
                }
                let mut mi: MONITORINFO = std::mem::zeroed();
                mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                if GetMonitorInfoW(mon, &mut mi) == 0 {
                    return Err("Windows wouldn't say how big the screen is".into());
                }
                let m = mi.rcMonitor;
                let frame = (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_BORDER | WS_DLGFRAME) as isize;
                SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !frame) | (WS_POPUP | WS_VISIBLE) as isize);
                let edges = (WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE) as isize;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !edges);
                SetWindowPos(hwnd, HWND_TOP, m.left, m.top, m.right - m.left, m.bottom - m.top, SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOOWNERZORDER);
                SetForegroundWindow(hwnd);
            } else if let Some((style, ex, [l, t, r, b], zoomed)) = SAVED.lock().ok().and_then(|mut s| s.take()) {
                SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
                SetWindowPos(hwnd, std::ptr::null_mut(), l, t, r - l, b - t, SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOOWNERZORDER);
                if zoomed {
                    ShowWindow(hwnd, SW_MAXIMIZE);
                }
            }
        }
        Ok(true)
    }
}

#[cfg(all(target_os = "linux", feature = "window"))]
mod platform {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_long, c_uchar, c_uint, c_ulong};
    use x11_dl::xlib::{self, Xlib};

    /// Looks through the window tree for the game's window.
    unsafe fn find(x: &Xlib, d: *mut xlib::Display, w: c_ulong, title: &str, depth: u32) -> Option<c_ulong> {
        unsafe {
            let mut name: *mut c_char = std::ptr::null_mut();
            if (x.XFetchName)(d, w, &mut name) != 0 && !name.is_null() {
                let same = std::ffi::CStr::from_ptr(name).to_string_lossy() == title;
                (x.XFree)(name as *mut _);
                if same {
                    return Some(w);
                }
            }
            if depth > 4 {
                return None;
            }
            let (mut root, mut parent): (c_ulong, c_ulong) = (0, 0);
            let mut kids: *mut c_ulong = std::ptr::null_mut();
            let mut n: c_uint = 0;
            if (x.XQueryTree)(d, w, &mut root, &mut parent, &mut kids, &mut n) == 0 || kids.is_null() {
                return None;
            }
            let list = std::slice::from_raw_parts(kids, n as usize).to_vec();
            (x.XFree)(kids as *mut _);
            list.into_iter().find_map(|k| find(x, d, k, title, depth + 1))
        }
    }

    pub fn set(on: bool) -> Result<bool, String> {
        let x = Xlib::open().map_err(|e| format!("no X11 library ({e})"))?;
        unsafe {
            let d = (x.XOpenDisplay)(std::ptr::null());
            if d.is_null() {
                return Err("couldn't reach the display".into());
            }
            let root = (x.XDefaultRootWindow)(d);
            let Some(w) = find(&x, d, root, "LUCKY WORLD", 0) else {
                (x.XCloseDisplay)(d);
                return Ok(false);
            };
            let state = CString::new("_NET_WM_STATE").unwrap();
            let full = CString::new("_NET_WM_STATE_FULLSCREEN").unwrap();
            let a_state = (x.XInternAtom)(d, state.as_ptr(), 0);
            let a_full = (x.XInternAtom)(d, full.as_ptr(), 0);
            let mut ev: xlib::XEvent = std::mem::zeroed();
            ev.client_message.type_ = xlib::ClientMessage;
            ev.client_message.window = w;
            ev.client_message.message_type = a_state;
            ev.client_message.format = 32;
            ev.client_message.data.set_long(0, if on { 1 } else { 0 } as c_long);
            ev.client_message.data.set_long(1, a_full as c_long);
            ev.client_message.data.set_long(2, 0);
            ev.client_message.data.set_long(3, 1);
            (x.XSendEvent)(d, root, 0, xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask, &mut ev);
            (x.XFlush)(d);
            (x.XCloseDisplay)(d);
            let _: c_uchar = 0;
        }
        Ok(true)
    }
}

#[cfg(not(any(windows, all(target_os = "linux", feature = "window"))))]
mod platform {
    pub fn set(_on: bool) -> Result<bool, String> {
        Err("fullscreen isn't supported on this system yet".into())
    }
}
