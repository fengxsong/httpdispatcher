use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub sources: HashMap<String, SourceConfig>,
    pub transforms: HashMap<String, TransformConfig>,
    pub sinks: HashMap<String, SinkConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SourceConfig {
    #[serde(rename = "type")]
    pub type_: String,
    pub address: String,
    pub port: Option<u16>,
    pub path: String,
    pub max_body_size_bytes: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransformConfig {
    #[serde(rename = "type")]
    pub type_: String,
    pub inputs: Vec<String>,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SinkConfig {
    #[serde(rename = "type")]
    pub type_: String,
    pub inputs: Vec<String>,
    #[serde(default)]
    pub uri: Option<String>,
    pub encoding: Option<String>,
    pub template: Option<String>,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub query_params: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub retry_attempts: Option<u32>,
    pub retry_interval_ms: Option<u64>,
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Error> {
        let content = fs::read_to_string(path)?;

        let extension = Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let config = match extension.to_lowercase().as_str() {
            "toml" => toml::from_str(&content)?,
            "yaml" | "yml" => serde_yaml::from_str(&content)?,
            "json" => serde_json::from_str(&content)?,
            _ => return Err(anyhow!("Unknown config file format: {}", extension)),
        };

        Ok(config)
    }
}
