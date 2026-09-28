use anyhow::{Context, Result};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub port: u16,
    /// Argon2 hash of the access password. `None` until the operator sets one.
    pub password_hash: Option<String>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            port: 7723,
            password_hash: None,
        }
    }
}

fn config_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "reemote", "reemote-host")
        .context("could not determine config directory")?;
    let dir = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

impl HostConfig {
    pub fn load_or_default() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn set_password(&mut self, password: &str) -> Result<()> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?
            .to_string();
        self.password_hash = Some(hash);
        Ok(())
    }

    pub fn verify_password(&self, password: &str) -> bool {
        let Some(hash) = &self.password_hash else {
            return false;
        };
        let Ok(parsed) = PasswordHash::new(hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }
}
