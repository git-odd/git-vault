use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_side_by_side: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_pager: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_paging: Option<String>,
}

pub fn get_vault_home() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "Could not determine user home directory (USERPROFILE/HOME not set)".to_string())?;
    Ok(PathBuf::from(home).join(".vault"))
}

pub fn get_vault_repo_dir() -> Result<PathBuf, String> {
    Ok(get_vault_home()?.join("repo"))
}

pub fn get_config_path() -> Result<PathBuf, String> {
    Ok(get_vault_home()?.join("config.json"))
}

pub fn load_config() -> Result<VaultConfig, String> {
    let config_path = get_config_path()?;
    if !config_path.exists() {
        return Ok(VaultConfig::default());
    }
    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config file at {}: {}", config_path.display(), e))?;
    let config: VaultConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config file at {}: {}", config_path.display(), e))?;
    Ok(config)
}

pub fn save_config(config: &VaultConfig) -> Result<(), String> {
    let vault_home = get_vault_home()?;
    if !vault_home.exists() {
        fs::create_dir_all(&vault_home)
            .map_err(|e| format!("Failed to create directory {}: {}", vault_home.display(), e))?;
    }
    let config_path = get_config_path()?;
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write config file to {}: {}", config_path.display(), e))?;
    Ok(())
}
