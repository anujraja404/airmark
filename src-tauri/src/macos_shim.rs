#![allow(unexpected_cfgs)]

use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
use cocoa::{
    appkit::NSApp,
    base::{id, NO, YES},
};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

pub fn apply_overlay_window_behavior(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window
            .ns_window()
            .map_err(|e| format!("failed to access native NSWindow: {e}"))?
            as id;

        unsafe {
            let _: () = msg_send![native_window, setIgnoresMouseEvents: YES];

            // NSWindowCollectionBehaviorCanJoinAllSpaces | Stationary | FullScreenAuxiliary
            let behavior: u64 = (1_u64 << 0) | (1_u64 << 4) | (1_u64 << 8);
            let _: () = msg_send![native_window, setCollectionBehavior: behavior];

            // Floating level keeps the overlay above ordinary app windows without covering
            // menu-bar/system UI layers.
            let level: i64 = 3;
            let _: () = msg_send![native_window, setLevel: level];
        }
    }

    Ok(())
}

pub fn set_click_through(window: &WebviewWindow, click_through: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(click_through)
        .map_err(|e| format!("failed to update cursor behavior: {e}"))?;

    #[cfg(target_os = "macos")]
    {
        let native_window = window
            .ns_window()
            .map_err(|e| format!("failed to access native NSWindow: {e}"))?
            as id;
        let ignores_mouse = if click_through { YES } else { NO };
        unsafe {
            let _: () = msg_send![native_window, setIgnoresMouseEvents: ignores_mouse];
        }
    }

    Ok(())
}

pub fn bring_settings_window_to_front(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window
            .ns_window()
            .map_err(|e| format!("failed to access native NSWindow: {e}"))?
            as id;
        unsafe {
            let app = NSApp();
            let _: () = msg_send![app, activateIgnoringOtherApps: YES];
            let _: () = msg_send![native_window, makeKeyAndOrderFront: std::ptr::null::<std::ffi::c_void>()];
            let _: () = msg_send![native_window, orderFrontRegardless];
        }
    }

    Ok(())
}
