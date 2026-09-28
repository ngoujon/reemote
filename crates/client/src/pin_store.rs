use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Persists trust-on-first-use certificate pins, keyed by "host:port", so
/// a hostile network can't silently swap the host's identity on a later
/// connection attempt.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PinStore {
    pins: HashMap<String, String>,
}

fn store_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "reemote", "reemote-client")
        .context("could not determine config directory")?;
    let dir = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("known_hosts.json"))
}

impl PinStore {
    pub fn load() -> Result<Self> {
        let path = store_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data).unwrap_or_default())
    }

    pub fn save(&self) -> Result<()> {
        let path = store_path()?;
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn get(&self, target: &str) -> Option<String> {
        self.pins.get(target).cloned()
    }

    pub fn set(&mut self, target: &str, fingerprint: &str) {
        self.pins.insert(target.to_string(), fingerprint.to_string());
    }
}
