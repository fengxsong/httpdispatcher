use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub sources: HashMap<String, SourceConfig>,
    #[serde(default)]
    pub transforms: HashMap<String, TransformConfig>,
    #[serde(default)]
    pub sinks: HashMap<String, SinkConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            transforms: HashMap::new(),
            sinks: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum SourceConfig {
    #[serde(rename = "http")]
    Http(HttpSourceConfig),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HttpSourceConfig {
    #[serde(default = "default_address")]
    pub address: String,
    #[serde(default = "default_port")]
    pub port: Option<u16>,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default = "default_max_body_size")]
    pub max_body_size_bytes: Option<usize>,
    #[serde(default = "default_enable_echo")]
    pub enable_echo: Option<bool>,
    #[serde(default)]
    pub basic_auth: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum TransformConfig {
    #[serde(rename = "remap")]
    Remap(RemapTransformConfig),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RemapTransformConfig {
    #[serde(default)]
    pub inputs: Vec<String>,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum SinkConfig {
    #[serde(rename = "http")]
    Http(HttpSinkConfig),
    #[serde(rename = "console")]
    Console(ConsoleSinkConfig),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HttpSinkConfig {
    #[serde(default)]
    pub inputs: Vec<String>,
    pub uri: String,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default = "default_method")]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub query_params: Option<HashMap<String, String>>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: Option<u32>,
    #[serde(default = "default_retry_interval")]
    pub retry_interval_ms: Option<u64>,
    #[serde(default)]
    pub basic_auth: Option<String>,
    #[serde(default)]
    pub authorization: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub proxy_from_environment: bool,
    #[serde(default)]
    pub proxy_connect_headers: Option<HashMap<String, String>>,
    #[serde(default = "default_follow_redirects")]
    pub follow_redirects: bool,
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConsoleSinkConfig {
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
}

fn default_address() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> Option<u16> {
    Some(3000)
}

fn default_path() -> String {
    "/ingest".to_string()
}

fn default_max_body_size() -> Option<usize> {
    Some(1048576) // 1MB
}

fn default_enable_echo() -> Option<bool> {
    Some(true)
}

fn default_method() -> Option<String> {
    Some("POST".to_string())
}

fn default_timeout() -> Option<u64> {
    Some(30000)
}

fn default_retry_attempts() -> Option<u32> {
    Some(3)
}

fn default_retry_interval() -> Option<u64> {
    Some(1000)
}

fn default_follow_redirects() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TlsConfig {
    #[serde(default)]
    pub ca_file: Option<String>,
    #[serde(default)]
    pub client_cert: Option<String>,
    #[serde(default)]
    pub client_key: Option<String>,
    #[serde(default = "default_verify")]
    pub verify: bool,
}

fn default_verify() -> bool {
    true
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
