#![allow(unexpected_cfgs)]

use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
use cocoa::{
    appkit::NSApp,
    base::{id, nil, NO, YES},
    foundation::NSString,
};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use std::{ffi::CStr, os::raw::c_char, path::PathBuf};

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

pub fn localized_display_names() -> Vec<String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let screens: id = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        let mut names = Vec::with_capacity(count);
        for index in 0..count {
            let screen: id = msg_send![screens, objectAtIndex: index];
            let localized_name: id = msg_send![screen, localizedName];
            if localized_name != std::ptr::null_mut() {
                let raw: *const c_char = msg_send![localized_name, UTF8String];
                if !raw.is_null() {
                    if let Ok(name) = CStr::from_ptr(raw).to_str() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        return names;
    }

    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

pub fn copied_image_file_path() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let pasteboard: id = msg_send![class!(NSPasteboard), generalPasteboard];
        let file_url_type = NSString::alloc(std::ptr::null_mut()).init_str("public.file-url");
        let raw_url: id = msg_send![pasteboard, stringForType: file_url_type];
        let _: () = msg_send![file_url_type, release];

        if raw_url == std::ptr::null_mut() {
            return Ok(None);
        }

        let raw: *const c_char = msg_send![raw_url, UTF8String];
        if raw.is_null() {
            return Ok(None);
        }

        let value = CStr::from_ptr(raw)
            .to_str()
            .map_err(|e| format!("failed to read copied file URL: {e}"))?;
        let Some(path) = file_url_to_path(value) else {
            return Ok(None);
        };

        if path.exists() {
            return Ok(Some(path.to_string_lossy().to_string()));
        }

        return Ok(None);
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

pub fn copied_image_png_bytes() -> Result<Option<Vec<u8>>, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        if let Some(bytes) = pasteboard_data_for_type("public.png")? {
            return Ok(Some(bytes));
        }

        if let Some(tiff_bytes) = pasteboard_data_for_type("public.tiff")? {
            let tiff_data: id = msg_send![class!(NSData), dataWithBytes: tiff_bytes.as_ptr() length: tiff_bytes.len()];
            if tiff_data == nil {
                return Ok(None);
            }

            let bitmap: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff_data];
            if bitmap == nil {
                return Ok(None);
            }

            // NSBitmapImageFileType.png
            let png_data: id = msg_send![bitmap, representationUsingType: 4usize properties: nil];
            if png_data == nil {
                return Ok(None);
            }

            return ns_data_to_vec(png_data).map(Some);
        }

        Ok(None)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

#[cfg(target_os = "macos")]
unsafe fn pasteboard_data_for_type(type_name: &str) -> Result<Option<Vec<u8>>, String> {
    let pasteboard: id = msg_send![class!(NSPasteboard), generalPasteboard];
    let ns_type = NSString::alloc(nil).init_str(type_name);
    let data: id = msg_send![pasteboard, dataForType: ns_type];
    let _: () = msg_send![ns_type, release];

    if data == nil {
        return Ok(None);
    }

    ns_data_to_vec(data).map(Some)
}

#[cfg(target_os = "macos")]
unsafe fn ns_data_to_vec(data: id) -> Result<Vec<u8>, String> {
    let length: usize = msg_send![data, length];
    let bytes: *const u8 = msg_send![data, bytes];
    if bytes.is_null() || length == 0 {
        return Ok(Vec::new());
    }
    Ok(std::slice::from_raw_parts(bytes, length).to_vec())
}

#[cfg(target_os = "macos")]
fn file_url_to_path(value: &str) -> Option<PathBuf> {
    let without_scheme = value.strip_prefix("file://")?;
    let decoded = percent_decode(without_scheme);
    Some(PathBuf::from(decoded))
}

#[cfg(target_os = "macos")]
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}
