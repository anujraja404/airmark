# Airmark

Airmark is a macOS 13+ menu-bar app that puts a watermark on one selected display without blocking the keyboard, mouse, or menu bar.

<p>
  <a href="https://github.com/anujraja/Airmark/releases/latest">
    <strong>Download the latest DMG</strong>
  </a>
</p>

<p>
  <a href="https://www.anujraja.com/projects/airmark">
    <strong>View the landing page</strong>
  </a>
</p>

## Metrics

- Native app bundle, not Electron.
- Rust handles tray, state, display selection, overlay placement, and macOS bridges.
- React renders only the settings UI and watermark content.
- Settings persist in a small local JSON file.
- No background server process.

## Architecture

```mermaid
flowchart LR
  subgraph macOS["macOS 13+"]
    Tray["Menu Bar Tray"]
    SettingsWindow["Native Settings Window"]
    OverlayWindow["Transparent Overlay Window"]
    LoginItem["Launch at Login"]
  end

  subgraph App["Airmark App"]
    Rust["Rust / Tauri Core"]
    State["Persisted App State"]
    MacOSShim["macOS Shim\n(AppKit bridge)"]
    Frontend["React + TypeScript UI"]
    Renderer["Overlay Renderer"]
  end

  Tray --> Rust
  SettingsWindow --> Frontend
  Frontend --> Rust
  Rust --> State
  Rust --> MacOSShim
  Rust --> OverlayWindow
  Frontend --> Renderer
  Renderer --> OverlayWindow
  LoginItem --> Rust
```

## Features

- Dockless macOS utility.
- Tray menu for enable/disable, settings, display selection, and quit.
- Text mode with opacity, size, and spacing controls.
- Image mode with drag/drop, file picker, and clipboard paste.
- Settings persist across relaunches.
- Launch-at-login support.

## Screenshots

### Menu Bar Options

![Menu bar](docs/screenshots/menubar.png)

### Settings Page: Text Mode

![Text mode settings](docs/screenshots/textmode.png)

### Settings Page: Image Mode

![Image mode settings](docs/screenshots/imagemode.png)

### Desktop Overlay Example

![Desktop overlay](docs/screenshots/desktopexample.png)

## Install

1. Open the DMG.
2. Drag `Airmark.app` into `Applications`.
3. Launch Airmark from `Applications` or the menu bar.

## Develop

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

Release artifacts:

- `src-tauri/target/release/bundle/macos/Airmark.app`
- `src-tauri/target/release/bundle/dmg/Airmark_0.1.0_aarch64.dmg`
