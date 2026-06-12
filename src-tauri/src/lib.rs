mod macos_shim;
mod state;

use serde::Serialize;
use state::{PersistedState, WatermarkSettings};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const CONTROLS_WINDOW_LABEL: &str = "controls";
const OVERLAY_WINDOW_LABEL: &str = "overlay";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayInfo {
    id: String,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    full_x: i32,
    full_y: i32,
    full_width: u32,
    full_height: u32,
    scale_factor: f64,
    primary: bool,
}

#[tauri::command]
fn get_settings(state: State<'_, PersistedState>) -> WatermarkSettings {
    state.get()
}

#[tauri::command]
fn list_displays(app: AppHandle) -> Result<Vec<DisplayInfo>, String> {
    displays_from_app(&app)
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, PersistedState>,
    settings: WatermarkSettings,
) -> Result<WatermarkSettings, String> {
    let next = state.set(settings)?;
    apply_autostart(&app, next.launch_at_login)?;
    apply_overlay_state(&app, &next)?;
    emit_settings_changed(&app, &next)?;
    refresh_tray_menu(&app)?;
    Ok(next)
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    show_controls_window(&app)
}

#[tauri::command]
fn hide_controls(app: AppHandle) -> Result<(), String> {
    if let Some(controls) = app.get_webview_window(CONTROLS_WINDOW_LABEL) {
        controls
            .hide()
            .map_err(|e| format!("failed to hide controls window: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn resize_controls_for_mode(app: AppHandle, mode: String) -> Result<(), String> {
    if let Some(controls) = app.get_webview_window(CONTROLS_WINDOW_LABEL) {
        let height = if mode == "image" { 340.0 } else { 398.0 };
        controls
            .set_size(LogicalSize::new(430.0, height))
            .map_err(|e| format!("failed to resize settings window: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
fn paste_copied_image_file() -> Result<Option<String>, String> {
    let Some(path) = macos_shim::copied_image_file_path()? else {
        return Ok(None);
    };

    let extension = Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_image = matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
    );
    if !is_image {
        return Err("The copied file is not a supported image.".to_string());
    }

    Ok(Some(path))
}

#[tauri::command]
fn paste_clipboard_image(app: AppHandle) -> Result<Option<String>, String> {
    if let Some(bytes) = macos_shim::copied_image_png_bytes()? {
        let mut image_dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("failed to resolve app config dir: {e}"))?;
        image_dir.push("pasted-images");
        fs::create_dir_all(&image_dir)
            .map_err(|e| format!("failed to create pasted image directory: {e}"))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("failed to create pasted image timestamp: {e}"))?
            .as_millis();
        image_dir.push(format!("airmark-pasted-{timestamp}.png"));
        fs::write(&image_dir, bytes)
            .map_err(|e| format!("failed to save pasted image: {e}"))?;
        return Ok(Some(image_dir.to_string_lossy().to_string()));
    }

    paste_copied_image_file()
}

#[tauri::command]
fn toggle_enabled(app: AppHandle, state: State<'_, PersistedState>) -> Result<WatermarkSettings, String> {
    let next = state.update(|settings| {
        settings.setup_completed = true;
        settings.enabled = !settings.enabled;
    })?;
    apply_overlay_state(&app, &next)?;
    emit_settings_changed(&app, &next)?;
    refresh_tray_menu(&app)?;
    Ok(next)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .macos_launcher(MacosLauncher::LaunchAgent)
                .build(),
        )
        .on_menu_event(|app, event| {
            if let Err(error) = handle_tray_menu_event(app, event.id.as_ref()) {
                eprintln!("tray event error: {error}");
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let state = PersistedState::load(&app.handle())?;
            let initial_settings = state.get();
            app.manage(state);

            create_controls_window(&app.handle())?;
            create_overlay_window(&app.handle())?;
            apply_autostart(&app.handle(), initial_settings.launch_at_login)?;
            apply_overlay_state(&app.handle(), &initial_settings)?;
            create_tray(&app.handle())?;
            if !initial_settings.setup_completed {
                show_controls_window_at_default_position(&app.handle())?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            list_displays,
            update_settings,
            open_settings,
            hide_controls,
            quit_app,
            resize_controls_for_mode,
            paste_copied_image_file,
            paste_clipboard_image,
            toggle_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn create_controls_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(CONTROLS_WINDOW_LABEL) {
        return Ok(existing);
    }

    let window = WebviewWindowBuilder::new(
        app,
        CONTROLS_WINDOW_LABEL,
        WebviewUrl::App("index.html?view=controls".into()),
    )
    .title("Airmark")
    .inner_size(430.0, 398.0)
    .resizable(false)
    .decorations(true)
    .shadow(true)
    .always_on_top(false)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|e| format!("failed to create controls window: {e}"))?;

    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(controls) = app_handle.get_webview_window(CONTROLS_WINDOW_LABEL) {
                let _ = controls.hide();
            }
        }
    });

    Ok(window)
}

fn create_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        return Ok(existing);
    }

    let overlay = WebviewWindowBuilder::new(
        app,
        OVERLAY_WINDOW_LABEL,
        WebviewUrl::App("index.html?view=overlay".into()),
    )
    .title("Watermark Overlay")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .focusable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|e| format!("failed to create overlay window: {e}"))?;

    overlay
        .set_visible_on_all_workspaces(true)
        .map_err(|e| format!("failed to set workspace visibility: {e}"))?;
    macos_shim::apply_overlay_window_behavior(&overlay)?;
    macos_shim::set_click_through(&overlay, true)?;

    Ok(overlay)
}

fn apply_overlay_state(app: &AppHandle, settings: &WatermarkSettings) -> Result<(), String> {
    let overlay = app
        .get_webview_window(OVERLAY_WINDOW_LABEL)
        .ok_or_else(|| "overlay window is not available".to_string())?;

    if !settings.enabled || !settings.setup_completed {
        overlay
            .hide()
            .map_err(|e| format!("failed to hide overlay: {e}"))?;
        return Ok(());
    }

    let displays = displays_from_app(app)?;
    let chosen = match settings.selected_display_id.as_ref() {
        Some(display_id) => displays.iter().find(|d| &d.id == display_id),
        None => None,
    };
    let target_display = chosen.or_else(|| displays.iter().find(|d| d.primary)).or_else(|| displays.first());

    if let Some(display) = target_display {
        overlay
            .set_position(PhysicalPosition::new(display.x, display.y))
            .map_err(|e| format!("failed to position overlay: {e}"))?;
        overlay
            .set_size(PhysicalSize::new(display.width, display.height))
            .map_err(|e| format!("failed to resize overlay: {e}"))?;
    }

    overlay
        .show()
        .map_err(|e| format!("failed to show overlay: {e}"))?;
    overlay
        .set_focusable(false)
        .map_err(|e| format!("failed to keep overlay non-focusable: {e}"))?;
    macos_shim::set_click_through(&overlay, true)?;

    Ok(())
}

fn create_tray(app: &AppHandle) -> Result<(), String> {
    let menu = build_tray_menu(app)?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))
            .map_err(|e| format!("failed to attach tray menu: {e}"))?;
        tray.set_show_menu_on_left_click(true)
            .map_err(|e| format!("failed to configure tray click behavior: {e}"))?;
        tray.set_icon_as_template(true)
            .map_err(|e| format!("failed to set tray icon template mode: {e}"))?;
        return Ok(());
    }

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon_as_template(true);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone()).icon_as_template(true);
    }
    builder
        .build(app)
        .map_err(|e| format!("failed to build tray icon: {e}"))?;

    Ok(())
}

fn refresh_tray_menu(app: &AppHandle) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id("main") {
        let menu = build_tray_menu(app)?;
        tray.set_menu(Some(menu))
            .map_err(|e| format!("failed to refresh tray menu: {e}"))?;
    }
    Ok(())
}

fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, String> {
    let settings = app.state::<PersistedState>().get();
    let displays = displays_from_app(app)?;

    let toggle_label = if settings.enabled {
        "Disable Watermark"
    } else {
        "Enable Watermark"
    };

    let toggle_item = MenuItem::with_id(app, "toggle_watermark", toggle_label, true, None::<&str>)
        .map_err(|e| format!("failed to create toggle menu item: {e}"))?;
    let controls_item = MenuItem::with_id(app, "show_controls", "Open Settings", true, None::<&str>)
        .map_err(|e| format!("failed to create controls menu item: {e}"))?;
    let display_items = displays
        .iter()
        .map(|display| {
            CheckMenuItem::with_id(
                app,
                format!("display::{}", display.id),
                display_label_for_menu(display),
                true,
                settings
                    .selected_display_id
                    .as_ref()
                    .map(|selected| selected == &display.id)
                    .unwrap_or(display.primary),
                None::<&str>,
            )
            .map_err(|e| format!("failed to create display menu item: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let display_item_refs = display_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<tauri::Wry>)
        .collect::<Vec<_>>();
    let display_submenu = Submenu::with_items(app, "Choose Display", true, &display_item_refs)
        .map_err(|e| format!("failed to create display submenu: {e}"))?;

    let separator =
        PredefinedMenuItem::separator(app).map_err(|e| format!("failed to create separator: {e}"))?;
    let quit_item =
        MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
            .map_err(|e| format!("failed to create quit menu item: {e}"))?;

    let menu = Menu::with_items(
        app,
        &[
            &toggle_item,
            &controls_item,
            &display_submenu,
            &separator,
            &quit_item,
        ],
    )
    .map_err(|e| format!("failed to create tray menu: {e}"))?;

    Ok(menu)
}

fn display_label_for_menu(display: &DisplayInfo) -> String {
    if display.primary {
        format!("Primary - {}", display.name)
    } else {
        display.name.clone()
    }
}

fn handle_tray_menu_event(app: &AppHandle, event_id: &str) -> Result<(), String> {
    match event_id {
        "show_controls" | "open_settings" => {
            show_controls_window(app)?;
        }
        "toggle_watermark" => {
            let state = app.state::<PersistedState>();
            let next = state.update(|settings| {
                settings.setup_completed = true;
                settings.enabled = !settings.enabled;
            })?;
            apply_overlay_state(app, &next)?;
            emit_settings_changed(app, &next)?;
            refresh_tray_menu(app)?;
        }
        "quit" => {
            app.exit(0);
        }
        _ if event_id.starts_with("display::") => {
            let chosen_display = event_id.trim_start_matches("display::").to_string();
            let state = app.state::<PersistedState>();
            let next = state.update(|settings| {
                settings.setup_completed = true;
                settings.selected_display_id = Some(chosen_display);
            })?;
            apply_overlay_state(app, &next)?;
            emit_settings_changed(app, &next)?;
            refresh_tray_menu(app)?;
        }
        _ => {}
    }
    Ok(())
}

fn show_controls_window(app: &AppHandle) -> Result<(), String> {
    let controls_window = app
        .get_webview_window(CONTROLS_WINDOW_LABEL)
        .ok_or_else(|| "controls window not found".to_string())?;

    controls_window
        .show()
        .map_err(|e| format!("failed to show controls window: {e}"))?;
    controls_window
        .set_focus()
        .map_err(|e| format!("failed to focus controls window: {e}"))?;
    macos_shim::bring_settings_window_to_front(&controls_window)?;
    Ok(())
}

fn show_controls_window_at_default_position(app: &AppHandle) -> Result<(), String> {
    let controls_window = app
        .get_webview_window(CONTROLS_WINDOW_LABEL)
        .ok_or_else(|| "controls window not found".to_string())?;
    position_controls_window_top_right(app, &controls_window)?;
    show_controls_window(app)
}

fn position_controls_window_top_right(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Result<(), String> {
    let size = window
        .inner_size()
        .map_err(|e| format!("failed to read controls window size: {e}"))?;
    let displays = displays_from_app(app)?;
    let display = displays
        .iter()
        .find(|display| display.primary)
        .or_else(|| displays.first());

    if let Some(display) = display {
        let x = display.full_x + display.full_width as i32 - size.width as i32 - 12;
        let y = display.y + 8;
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| format!("failed to position controls window: {e}"))?;
    }
    Ok(())
}

fn displays_from_app(app: &AppHandle) -> Result<Vec<DisplayInfo>, String> {
    let all_monitors = app
        .available_monitors()
        .map_err(|e| format!("failed to read monitors: {e}"))?;
    let primary = app
        .primary_monitor()
        .map_err(|e| format!("failed to read primary monitor: {e}"))?;
    let primary_id = primary.as_ref().map(stable_monitor_id);
    let localized_names = macos_shim::localized_display_names();

    let mut displays = Vec::new();
    for (index, monitor) in all_monitors.into_iter().enumerate() {
        let id = stable_monitor_id(&monitor);
        let is_primary = primary_id
            .as_ref()
            .map(|candidate| candidate == &id)
            .unwrap_or(false);
        let tauri_name = monitor.name().cloned();
        let localized_name = localized_names.get(index).cloned();
        let name = localized_name.or_else(|| tauri_name.filter(|name| !name.starts_with("Monitor #"))).unwrap_or_else(|| {
            if is_primary {
                "Primary Display".to_string()
            } else {
                format!("Display {}", index + 1)
            }
        });
        let position = monitor.position();
        let size = monitor.size();
        let work_area = monitor.work_area();
        displays.push(DisplayInfo {
            id: id.clone(),
            name,
            x: work_area.position.x,
            y: work_area.position.y,
            width: work_area.size.width,
            height: work_area.size.height,
            full_x: position.x,
            full_y: position.y,
            full_width: size.width,
            full_height: size.height,
            scale_factor: monitor.scale_factor(),
            primary: is_primary,
        });
    }
    Ok(displays)
}

fn stable_monitor_id(monitor: &tauri::window::Monitor) -> String {
    let name = monitor.name().cloned().unwrap_or_else(|| "unknown".to_string());
    let position = monitor.position();
    let size = monitor.size();
    format!(
        "{}::{}::{}::{}::{}",
        name, position.x, position.y, size.width, size.height
    )
}

fn apply_autostart(app: &AppHandle, should_enable: bool) -> Result<(), String> {
    let autostart = app.autolaunch();
    if should_enable {
        autostart
            .enable()
            .map_err(|e| format!("failed to enable launch at login: {e}"))?;
    } else {
        autostart
            .disable()
            .map_err(|e| format!("failed to disable launch at login: {e}"))?;
    }
    Ok(())
}

fn emit_settings_changed(app: &AppHandle, settings: &WatermarkSettings) -> Result<(), String> {
    app.emit("settings-changed", settings)
        .map_err(|e| format!("failed to emit settings change event: {e}"))
}
