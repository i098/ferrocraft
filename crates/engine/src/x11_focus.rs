//! X11 without a window manager sends no focus event, so winit drops every key.
//! LWJGL2 reads core key events and needs no focus event. This sets the X input
//! focus by hand when no focus event arrives shortly after the window maps.
use std::time::{Duration, Instant};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

const WAIT: Duration = Duration::from_millis(500);

/// Waits for a focus event, then forces the X input focus once.
pub struct FocusFallback {
    mapped: Option<Instant>,
    focused: bool,
    done: bool,
}

impl FocusFallback {
    pub fn new() -> Self {
        Self {
            mapped: None,
            focused: false,
            done: false,
        }
    }

    pub fn on_focused(&mut self, focused: bool) {
        self.focused |= focused;
    }

    /// True once, when the wait since the first poll is over and no focus event came.
    fn due(&mut self, now: Instant) -> bool {
        if self.done || self.focused {
            return false;
        }
        let mapped = *self.mapped.get_or_insert(now);
        if now.duration_since(mapped) < WAIT {
            return false;
        }
        self.done = true;
        true
    }

    pub fn poll(&mut self, window: &Window) {
        if self.due(Instant::now()) {
            set_input_focus(window);
        }
    }
}

/// X11 only, and only without a window manager: no-op under Wayland, macOS and Windows.
fn set_input_focus(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Xlib(h) = handle.as_raw() else {
        return;
    };
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return;
    };
    // SAFETY: own display connection, closed here; the window id comes from winit.
    unsafe {
        let dpy = (xlib.XOpenDisplay)(std::ptr::null());
        if dpy.is_null() {
            return;
        }
        if has_window_manager(&xlib, dpy) {
            (xlib.XCloseDisplay)(dpy);
            return;
        }
        (xlib.XSetInputFocus)(
            dpy,
            h.window,
            x11_dl::xlib::RevertToParent,
            x11_dl::xlib::CurrentTime,
        );
        (xlib.XCloseDisplay)(dpy);
    }
}

/// EWMH: a running WM sets `_NET_SUPPORTING_WM_CHECK` on the root window.
unsafe fn has_window_manager(xlib: &x11_dl::xlib::Xlib, dpy: *mut x11_dl::xlib::Display) -> bool {
    unsafe {
        let root = (xlib.XDefaultRootWindow)(dpy);
        let atom = (xlib.XInternAtom)(dpy, c"_NET_SUPPORTING_WM_CHECK".as_ptr(), 1);
        if atom == 0 {
            return false;
        }
        let (mut ty, mut fmt, mut n, mut after) = (0, 0, 0, 0);
        let mut data: *mut u8 = std::ptr::null_mut();
        let status = (xlib.XGetWindowProperty)(
            dpy,
            root,
            atom,
            0,
            1,
            0,
            x11_dl::xlib::XA_WINDOW,
            &mut ty,
            &mut fmt,
            &mut n,
            &mut after,
            &mut data,
        );
        let found = status == 0 && !data.is_null() && n > 0;
        if !data.is_null() {
            (xlib.XFree)(data.cast());
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forces_focus_once_only_when_no_focus_event_arrived() {
        let mut f = FocusFallback::new();
        let t0 = Instant::now();
        assert!(!f.due(t0));
        assert!(!f.due(t0 + Duration::from_millis(100)));
        assert!(f.due(t0 + WAIT));
        assert!(!f.due(t0 + WAIT * 2));

        let mut g = FocusFallback::new();
        g.on_focused(true);
        let t1 = Instant::now();
        assert!(!g.due(t1));
        assert!(!g.due(t1 + WAIT * 2));

        let mut h = FocusFallback::new();
        let late = Instant::now() + WAIT * 10;
        assert!(!h.due(late));
        assert!(h.due(late + WAIT));
    }
}
