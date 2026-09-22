//! Persistent Application Settings Manager
//!
//! Loads and saves user preferences to `settings.json` (VPS IP, ports, active profile).

use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub vps_host: String,
    pub vps_ch1: u16,
    pub vps_ch2: u16,
    pub active_profile: String,
    pub enable_registry_tuning: bool,
    pub auto_connect: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            vps_host: "72.61.111.131".to_string(),
            vps_ch1: 51820,
            vps_ch2: 4433,
            active_profile: "profiles/farever.json".to_string(),
            enable_registry_tuning: true,
            auto_connect: false,
        }
    }
}

impl AppSettings {
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Self {
        let p = path.as_ref();
        if p.exists() {
            match fs::read_to_string(p) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(settings) => return settings,
                    Err(e) => warn!("Failed to parse settings from {:?}: {}", p, e),
                },
                Err(e) => warn!("Failed to read settings from {:?}: {}", p, e),
            }
        }
        // Fallback or create default
        let default_settings = Self::default();
        let _ = default_settings.save(p);
        default_settings
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}
