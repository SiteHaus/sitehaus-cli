use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerType {
    Ecom,
    Platform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(rename = "type")]
    pub server_type: ServerType,
    pub host: String,
    pub ssh_user: String,
    pub ssh_key_path: Option<String>,
    pub repo_path: String,
    pub health_url: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CliConfig {
    pub active_server: Option<String>,
    #[serde(default)]
    pub servers: HashMap<String, ServerConfig>,
}

pub fn config_path() -> PathBuf {
    let home = dirs::home_dir().expect("could not determine home directory");
    home.join(".sitehaus").join("config.yml")
}

pub fn read_config() -> Result<CliConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let config: CliConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(config)
}

pub fn write_config(config: &CliConfig) -> Result<()> {
    let path = config_path();
    fs::create_dir_all(path.parent().unwrap())?;
    let contents = serde_yaml::to_string(config)?;
    fs::write(&path, contents)?;
    Ok(())
}

pub fn get_server<'a>(config: &'a CliConfig, name: &str) -> Result<&'a ServerConfig> {
    config
        .servers
        .get(name)
        .with_context(|| format!("unknown server \"{name}\" — run: sitehaus server list"))
}

#[allow(dead_code)]
pub fn resolve_server<'a>(
    config: &'a CliConfig,
    override_name: Option<&'a str>,
) -> Result<(&'a str, &'a ServerConfig)> {
    let name = override_name
        .or(config.active_server.as_deref())
        .context("no active server — run: sitehaus use <server>")?;
    let server = get_server(config, name)?;
    Ok((name, server))
}
