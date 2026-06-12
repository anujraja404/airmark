use std::{fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WatermarkMode {
    Image,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkSettings {
    pub enabled: bool,
    #[serde(default)]
    pub setup_completed: bool,
    pub mode: WatermarkMode,
    pub image_path: Option<String>,
    pub text: String,
    pub opacity: f64,
    pub selected_display_id: Option<String>,
    pub launch_at_login: bool,
    pub text_size: u16,
    pub text_spacing: u16,
}

impl Default for WatermarkSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            setup_completed: false,
            mode: WatermarkMode::Text,
            image_path: None,
            text: "CONFIDENTIAL".to_string(),
            opacity: 0.20,
            selected_display_id: None,
            launch_at_login: false,
            text_size: 40,
            text_spacing: 220,
        }
    }
}

pub struct PersistedState {
    path: PathBuf,
    settings: Mutex<WatermarkSettings>,
}

impl PersistedState {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let mut config_dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("failed to resolve app config dir: {e}"))?;
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("failed to create app config dir {:?}: {e}", config_dir))?;
        config_dir.push("watermark-settings.json");

        let mut settings = if config_dir.exists() {
            let raw = fs::read_to_string(&config_dir)
                .map_err(|e| format!("failed to read settings file {:?}: {e}", config_dir))?;
            serde_json::from_str::<WatermarkSettings>(&raw)
                .map_err(|e| format!("failed to parse settings file {:?}: {e}", config_dir))?
        } else {
            WatermarkSettings::default()
        };
        if !settings.setup_completed {
            settings.enabled = false;
        }

        Ok(Self {
            path: config_dir,
            settings: Mutex::new(settings),
        })
    }

    pub fn get(&self) -> WatermarkSettings {
        self.settings
            .lock()
            .expect("settings mutex poisoned")
            .clone()
    }

    pub fn set(&self, next: WatermarkSettings) -> Result<WatermarkSettings, String> {
        {
            let mut guard = self.settings.lock().expect("settings mutex poisoned");
            *guard = next.clone();
        }
        self.persist()?;
        Ok(next)
    }

    pub fn update<F>(&self, mutator: F) -> Result<WatermarkSettings, String>
    where
        F: FnOnce(&mut WatermarkSettings),
    {
        let next = {
            let mut guard = self.settings.lock().expect("settings mutex poisoned");
            mutator(&mut guard);
            guard.clone()
        };
        self.persist()?;
        Ok(next)
    }

    fn persist(&self) -> Result<(), String> {
        let payload = {
            let guard = self.settings.lock().expect("settings mutex poisoned");
            serde_json::to_string_pretty(&*guard)
                .map_err(|e| format!("failed to serialize settings: {e}"))?
        };
        fs::write(&self.path, payload)
            .map_err(|e| format!("failed to write settings {:?}: {e}", self.path))?;
        Ok(())
    }
}
