//! Integration tests for AccessKit support.

#![cfg(all(windows, feature = "accessibility"))]
#![expect(clippy::expect_used, reason = "panicking is how tests report failure")]

use baseview::{
    AccessibilityEvent, Event, EventStatus, HandlerError, PlatformHandle, Window, WindowContext,
    WindowHandler, WindowSettings, WindowSize,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, SendMessageW, TranslateMessage, MSG, PM_REMOVE, WM_GETOBJECT,
};

/// The UI Automation root object id, as passed in `WM_GETOBJECT`'s lparam.
const UIA_ROOT_OBJECT_ID: LPARAM = -25;

#[derive(Default)]
struct Shared {
    handle: Option<PlatformHandle>,
    accessibility_enabled: bool,
}

struct TestHandler {
    shared: Arc<Mutex<Shared>>,
}

impl WindowHandler for TestHandler {
    fn on_frame(&self) -> Result<(), HandlerError> {
        Ok(())
    }

    fn resized(&self, _new_size: WindowSize) -> Result<(), HandlerError> {
        Ok(())
    }

    fn on_event(&self, event: Event) -> EventStatus {
        if let Event::Accessibility(AccessibilityEvent::Enabled) = event {
            if let Ok(mut shared) = self.shared.lock() {
                shared.accessibility_enabled = true;
            }
        }

        EventStatus::Ignored
    }
}

fn pump_messages() {
    let mut msg: MSG = unsafe { std::mem::zeroed() };

    // SAFETY: standard Win32 message pump over this thread's queue.
    unsafe {
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[test]
fn wm_getobject_activates_accessibility() {
    // SAFETY: this test process is a standalone application, not a plugin host.
    unsafe { baseview::assume_standalone_in_process() };

    let shared = Arc::new(Mutex::new(Shared::default()));

    let window = {
        let shared = Arc::clone(&shared);

        Window::create(WindowSettings::default(), move |ctx: WindowContext| {
            if let Ok(mut guard) = shared.lock() {
                guard.handle = Some(ctx.platform_handle());
            }

            Ok(TestHandler { shared: Arc::clone(&shared) })
        })
        .expect("window creation failed")
    };

    window.show().expect("failed to show window");
    pump_messages();

    let hwnd = {
        let guard = shared.lock().expect("shared state poisoned");
        let handle = guard.handle.as_ref().expect("no platform handle recorded");
        match handle.window_handle().expect("no window handle").as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as HWND,
            other => panic!("unexpected window handle type: {other:?}"),
        }
    };

    // SAFETY: `hwnd` belongs to a live window owned by this thread. Sending from the owning
    // thread invokes `wnd_proc` directly.
    let result: LRESULT =
        unsafe { SendMessageW(hwnd, WM_GETOBJECT, 0 as WPARAM, UIA_ROOT_OBJECT_ID) };

    assert_ne!(result, 0, "WM_GETOBJECT was not handled by the AccessKit adapter");

    pump_messages();

    let enabled = shared.lock().expect("shared state poisoned").accessibility_enabled;
    assert!(enabled, "handler never received AccessibilityEvent::Enabled");
}
