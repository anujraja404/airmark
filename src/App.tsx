import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type WatermarkMode = "image" | "text";

type WatermarkSettings = {
  enabled: boolean;
  setupCompleted: boolean;
  mode: WatermarkMode;
  imagePath: string | null;
  text: string;
  opacity: number;
  selectedDisplayId: string | null;
  launchAtLogin: boolean;
  textSize: number;
  textSpacing: number;
};

type DisplayInfo = {
  id: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  fullX: number;
  fullY: number;
  fullWidth: number;
  fullHeight: number;
  scaleFactor: number;
  primary: boolean;
};

const TEXT_SIZE_MIN = 12;
const TEXT_SIZE_MAX = 96;
const TEXT_SPACING_MIN = 120;
const TEXT_SPACING_MAX = 420;

const DEFAULT_SETTINGS: WatermarkSettings = {
  enabled: false,
  setupCompleted: false,
  mode: "text",
  imagePath: null,
  text: "CONFIDENTIAL",
  opacity: 0.2,
  selectedDisplayId: null,
  launchAtLogin: false,
  textSize: 40,
  textSpacing: 220,
};

async function loadSettings(): Promise<WatermarkSettings> {
  return invoke<WatermarkSettings>("get_settings");
}

async function saveSettings(settings: WatermarkSettings): Promise<WatermarkSettings> {
  return invoke<WatermarkSettings>("update_settings", { settings });
}

async function loadDisplays(): Promise<DisplayInfo[]> {
  return invoke<DisplayInfo[]>("list_displays");
}

function currentView(): "overlay" | "controls" {
  const query = new URLSearchParams(window.location.search);
  return query.get("view") === "overlay" ? "overlay" : "controls";
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function displayLabel(display: DisplayInfo) {
  return `${display.primary ? "Primary - " : ""}${display.name}`;
}

function App() {
  const view = useMemo(() => currentView(), []);
  const [settings, setSettings] = useState<WatermarkSettings>(DEFAULT_SETTINGS);
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [dragActive, setDragActive] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const bootstrap = async () => {
      try {
        const [savedSettings, availableDisplays] = await Promise.all([
          loadSettings(),
          loadDisplays(),
        ]);
        const primaryDisplay = availableDisplays.find((display) => display.primary);
        if (!cancelled) {
          setSettings({
            ...savedSettings,
            selectedDisplayId: savedSettings.selectedDisplayId ?? primaryDisplay?.id ?? null,
          });
          setDisplays(availableDisplays);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    bootstrap();

    const unlistenPromise = listen<WatermarkSettings>("settings-changed", (event) => {
      if (!cancelled) {
        setSettings(event.payload);
      }
    });

    return () => {
      cancelled = true;
      void unlistenPromise.then((dispose) => dispose());
    };
  }, []);

  const selectedDisplay =
    displays.find((display) => display.id === settings.selectedDisplayId) ??
    displays.find((display) => display.primary) ??
    displays[0];

  const patchSettings = async (patch: Partial<WatermarkSettings>) => {
    const next = { ...settings, ...patch };
    next.textSize = clamp(next.textSize, TEXT_SIZE_MIN, TEXT_SIZE_MAX);
    next.textSpacing = clamp(next.textSpacing, TEXT_SPACING_MIN, TEXT_SPACING_MAX);

    setSettings(next);
    try {
      const persisted = await saveSettings(next);
      setSettings(persisted);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => {
    if (view !== "controls") {
      return;
    }
    void invoke("resize_controls_for_mode", { mode: settings.mode });
  }, [settings.mode, view]);

  useEffect(() => {
    if (view !== "controls") {
      return;
    }

    const unlistenPromise = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") {
        setDragActive(true);
      }
      if (event.payload.type === "leave") {
        setDragActive(false);
      }
      if (event.payload.type === "drop") {
        setDragActive(false);
        const imagePath = event.payload.paths.find((path) =>
          /\.(png|jpe?g|webp|gif|bmp|tiff?)$/i.test(path),
        );
        if (imagePath) {
          void patchSettings({ imagePath, mode: "image" });
        } else {
          setError("Drop a supported image file.");
        }
      }
    });

    return () => {
      void unlistenPromise.then((dispose) => dispose());
    };
    // patchSettings reads current React state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, settings]);

  const confirmAndEnable = async () => {
    await patchSettings({
      enabled: true,
      setupCompleted: true,
      selectedDisplayId: selectedDisplay?.id ?? null,
    });
  };

  const chooseImage = async () => {
    try {
      const chosen = await open({
        title: "Choose watermark image",
        multiple: false,
        filters: [
          {
            name: "Images",
            extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff"],
          },
        ],
      });
      if (typeof chosen === "string") {
        await patchSettings({ imagePath: chosen, mode: "image" });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const pasteCopiedImage = async () => {
    try {
      const imagePath = await invoke<string | null>("paste_clipboard_image");
      if (imagePath) {
        await patchSettings({ imagePath, mode: "image" });
      } else {
        setError("Copy an image from ChatGPT, Preview, Safari, or Finder, then paste it here.");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  if (loading) {
    return <main className={view === "overlay" ? "overlay-root" : "controls-root"} />;
  }

  if (view === "overlay") {
    const imageSrc = settings.imagePath ? convertFileSrc(settings.imagePath) : null;
    return (
      <main className="overlay-root">
        {settings.enabled && settings.mode === "image" && imageSrc ? (
          <img
            className="overlay-image"
            src={imageSrc}
            style={{ opacity: settings.opacity }}
            alt=""
          />
        ) : null}
        {settings.enabled && settings.mode === "text" ? (
          <div
            className="overlay-text-grid"
            style={
              {
                "--watermark-opacity": settings.opacity,
                "--watermark-size": `${settings.textSize}px`,
                "--watermark-spacing": `${settings.textSpacing}px`,
              } as React.CSSProperties
            }
          >
            {Array.from({ length: 36 }).map((_, idx) => (
              <span key={idx} className="overlay-text-cell">
                {settings.text || "CONFIDENTIAL"}
              </span>
            ))}
          </div>
        ) : null}
      </main>
    );
  }

  return (
    <main className="controls-root">
      <section className="settings-panel">
        <div className="toggle-strip">
          <label className="toggle-pill">
            <span>Enable</span>
            <input
              type="checkbox"
              role="switch"
              checked={settings.enabled}
              onChange={(e) =>
                patchSettings({ enabled: e.currentTarget.checked, setupCompleted: true })
              }
            />
          </label>
          <label className="toggle-pill">
            <span>Start at Login</span>
            <input
              type="checkbox"
              role="switch"
              checked={settings.launchAtLogin}
              onChange={(e) => patchSettings({ launchAtLogin: e.currentTarget.checked })}
            />
          </label>
        </div>

        <label className="display-row">
          <span className="section-label">
            <MonitorIcon />
            Display
          </span>
          <span className="select-wrap">
            <select
              value={settings.selectedDisplayId ?? selectedDisplay?.id ?? ""}
              onChange={(e) =>
                patchSettings({
                  selectedDisplayId: e.currentTarget.value || null,
                  enabled: settings.setupCompleted ? settings.enabled : false,
                })
              }
            >
              {displays.map((display) => (
                <option key={display.id} value={display.id}>
                  {displayLabel(display)}
                </option>
              ))}
            </select>
            <ChevronIcon />
          </span>
        </label>

        <div className="segmented-control" role="group" aria-label="Watermark mode">
          <button
            type="button"
            className={settings.mode === "text" ? "selected" : ""}
            onClick={() => patchSettings({ mode: "text" })}
          >
            Text Mode
          </button>
          <button
            type="button"
            className={settings.mode === "image" ? "selected" : ""}
            onClick={() => patchSettings({ mode: "image" })}
          >
            Image Mode
          </button>
        </div>

        {settings.mode === "text" ? (
          <div className="mode-panel">
            <label className="text-field">
              <input
                value={settings.text}
                placeholder="Example text here"
                onChange={(e) => patchSettings({ text: e.currentTarget.value })}
              />
              {settings.text ? (
                <button
                  type="button"
                  className="field-clear"
                  aria-label="Clear watermark text"
                  onClick={() => patchSettings({ text: "" })}
                >
                  <CloseIcon />
                </button>
              ) : null}
            </label>

            <RangeField
              label="Opacity"
              icon={<DropIcon />}
              min={0.05}
              max={1}
              step={0.01}
              value={settings.opacity}
              displayValue={`${Math.round(settings.opacity * 100)}%`}
              onChange={(value) => patchSettings({ opacity: value })}
            />
            <RangeField
              label="Text Size"
              icon={<SizeIcon />}
              min={TEXT_SIZE_MIN}
              max={TEXT_SIZE_MAX}
              step={1}
              value={settings.textSize}
              displayValue={`${settings.textSize}px`}
              onChange={(value) => patchSettings({ textSize: Math.round(value) })}
            />
            <RangeField
              label="Text Spacing"
              icon={<SpacingIcon />}
              min={TEXT_SPACING_MIN}
              max={TEXT_SPACING_MAX}
              step={5}
              value={settings.textSpacing}
              displayValue={`${settings.textSpacing}px`}
              onChange={(value) => patchSettings({ textSpacing: Math.round(value) })}
            />
          </div>
        ) : (
          <div className="mode-panel">
            <div className="image-actions">
              <button
                type="button"
                className={dragActive ? "image-button dragging" : "image-button"}
                onClick={chooseImage}
              >
                <span>{settings.imagePath ? "Image selected" : "Select image from folder"}</span>
                <small>{settings.imagePath ?? "Drop an image here too"}</small>
              </button>
              <button type="button" className="image-button paste-button" onClick={pasteCopiedImage}>
                <span>Paste image</span>
                <small>Copied image data or file</small>
              </button>
            </div>

            <RangeField
              label="Opacity"
              icon={<DropIcon />}
              min={0.05}
              max={1}
              step={0.01}
              value={settings.opacity}
              displayValue={`${Math.round(settings.opacity * 100)}%`}
              onChange={(value) => patchSettings({ opacity: value })}
            />
          </div>
        )}

        <footer className="action-row">
          <button type="button" className="primary-button" onClick={confirmAndEnable}>
            Confirm & Enable
          </button>
          <button type="button" className="secondary-button" onClick={() => invoke("quit_app")}>
            Quit
          </button>
        </footer>

        {error ? <p className="error">{error}</p> : null}
      </section>
    </main>
  );
}

function RangeField({
  label,
  icon,
  min,
  max,
  step,
  value,
  displayValue,
  onChange,
}: {
  label: string;
  icon: ReactNode;
  min: number;
  max: number;
  step: number;
  value: number;
  displayValue: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="range-field">
      <span className="range-icon" aria-hidden="true">
        {icon}
      </span>
      <span className="range-label">{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number.parseFloat(e.currentTarget.value))}
      />
      <strong>{displayValue}</strong>
    </label>
  );
}

function CloseIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M4.25 4.25 11.75 11.75M11.75 4.25 4.25 11.75" />
    </svg>
  );
}

function MonitorIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <rect x="3.5" y="4" width="13" height="9" rx="1.4" />
      <path d="M8.25 15.75h3.5M10 13v2.75" />
    </svg>
  );
}

function ChevronIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="m4.5 6.25 3.5 3.5 3.5-3.5" />
    </svg>
  );
}

function DropIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path d="M10 3.25c2.6 3.05 4.75 5.45 4.75 8.05a4.75 4.75 0 0 1-9.5 0c0-2.6 2.15-5 4.75-8.05Z" />
    </svg>
  );
}

function SizeIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path d="M3.75 14.5 6.7 6.25h1.1l2.95 8.25M4.7 11.75h5.1M11.6 14.5l2.1-5.75h.8l2.1 5.75M12.3 12.55h3.8" />
    </svg>
  );
}

function SpacingIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path d="M4 10h12M4 10l2.25-2.25M4 10l2.25 2.25M16 10l-2.25-2.25M16 10l-2.25 2.25" />
    </svg>
  );
}

export default App;
