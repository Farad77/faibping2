//! Game Profile Parser & Auto-detection Engine
//!
//! Supports declarative JSON game definitions with port range resolution,
//! protocol filtering, and executable signature matching.

use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize, Deserializer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameProfile {
    pub game_id: String,
    pub name: String,
    pub executables: Vec<String>,
    pub routing: RoutingConfig,
    pub optimizations: OptimizationsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub protocol: String, // "TCP", "UDP", "BOTH"
    pub ports: Vec<PortMatcher>,
    pub target_subnets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortMatcher {
    Single(u16),
    Range(u16, u16),
}

impl PortMatcher {
    pub fn matches(&self, port: u16) -> bool {
        match self {
            PortMatcher::Single(p) => *p == port,
            PortMatcher::Range(start, end) => port >= *start && port <= *end,
        }
    }
}

impl Serialize for PortMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PortMatcher::Single(p) => serializer.serialize_u16(*p),
            PortMatcher::Range(s, e) => serializer.serialize_str(&format!("{}-{}", s, e)),
        }
    }
}

impl<'de> Deserialize<'de> for PortMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Number(n) => {
                if let Some(port) = n.as_u64() {
                    Ok(PortMatcher::Single(port as u16))
                } else {
                    Err(serde::de::Error::custom("Invalid port number"))
                }
            }
            serde_json::Value::String(s) => {
                if let Some((start_s, end_s)) = s.split_once('-') {
                    let start = start_s.trim().parse::<u16>().map_err(serde::de::Error::custom)?;
                    let end = end_s.trim().parse::<u16>().map_err(serde::de::Error::custom)?;
                    Ok(PortMatcher::Range(start, end))
                } else {
                    let port = s.trim().parse::<u16>().map_err(serde::de::Error::custom)?;
                    Ok(PortMatcher::Single(port))
                }
            }
            _ => Err(serde::de::Error::custom("Port must be integer or range string")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationsConfig {
    pub enable_fastconnect: bool,
    pub enable_multipath_dup: bool,
    pub packet_duplication_rate: u32,
    pub dscp_tag: u8,
}

impl GameProfile {
    /// Load a profile from a JSON file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let profile: Self = serde_json::from_str(&content)?;
        Ok(profile)
    }

    /// Check if a given port matches the routing specification
    pub fn matches_port(&self, port: u16) -> bool {
        if self.routing.ports.is_empty() {
            return true;
        }
        self.routing.ports.iter().any(|m| m.matches(port))
    }

    /// Check if an executable filename matches this profile
    pub fn matches_executable(&self, exe_name: &str) -> bool {
        let exe_lower = exe_name.to_lowercase();
        self.executables.iter().any(|e| {
            let target_lower = e.to_lowercase();
            exe_lower == target_lower || exe_lower.ends_with(&target_lower)
        })
    }

    /// Auto-detect matching profile for an executable by scanning a profile directory
    pub fn auto_detect_profile<P: AsRef<Path>>(
        profiles_dir: P,
        exe_path: &Path,
    ) -> Option<Self> {
        let exe_file_name = exe_path.file_name()?.to_str()?;
        let entries = fs::read_dir(profiles_dir).ok()?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Ok(profile) = Self::load_from_file(&path) {
                    if profile.matches_executable(exe_file_name) {
                        return Some(profile);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_aion2_profile() {
        let json_data = r#"{
            "game_id": "aion2",
            "name": "Aion 2 (MMORPG)",
            "executables": [
                "Aion2.exe",
                "Aion2-Win64-Shipping.exe",
                "Aion2Launcher.exe"
            ],
            "routing": {
                "protocol": "BOTH",
                "ports": [3724, 7777, 8000, "9000-9100"],
                "target_subnets": ["0.0.0.0/0"]
            },
            "optimizations": {
                "enable_fastconnect": true,
                "enable_multipath_dup": true,
                "packet_duplication_rate": 2,
                "dscp_tag": 46
            }
        }"#;

        let profile: GameProfile = serde_json::from_str(json_data).unwrap();
        assert_eq!(profile.game_id, "aion2");
        assert!(profile.matches_executable("Aion2.exe"));
        assert!(profile.matches_executable("C:\\Games\\Aion2\\Binaries\\Win64\\Aion2-Win64-Shipping.exe"));
        assert!(!profile.matches_executable("chrome.exe"));

        assert!(profile.matches_port(3724));
        assert!(profile.matches_port(7777));
        assert!(profile.matches_port(9050)); // Inside 9000-9100 range
        assert!(!profile.matches_port(443)); // Out of range
    }
}
