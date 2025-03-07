use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,
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
            channel_capacity: default_channel_capacity(),
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

fn default_channel_capacity() -> usize {
    100
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut config: Config = match Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "toml" => toml::from_str(&content)?,
            "yaml" | "yml" => serde_yaml::from_str(&content)?,
            "json" => serde_json::from_str(&content)?,
            _ => return Err(anyhow!("Unknown config file format: {}", path)),
        };

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate sources
        for (name, source) in &self.sources {
            match source {
                SourceConfig::Http(http) => {
                    if http.port.unwrap_or(0) == 0 {
                        return Err(anyhow!("Source {} has invalid port configuration", name));
                    }
                }
            }
        }

        // Validate transforms
        for (name, transform) in &self.transforms {
            match transform {
                TransformConfig::Remap(remap) => {
                    if remap.inputs.is_empty() {
                        return Err(anyhow!("Transform {} has no inputs configured", name));
                    }
                    if remap.source.is_empty() {
                        return Err(anyhow!("Transform {} has empty VRL source", name));
                    }
                }
            }
        }

        // Validate sinks
        for (name, sink) in &self.sinks {
            match sink {
                SinkConfig::Http(http) => {
                    if http.inputs.is_empty() {
                        return Err(anyhow!("Sink {} has no inputs configured", name));
                    }
                    if http.uri.is_empty() {
                        return Err(anyhow!("Sink {} has empty URI", name));
                    }
                }
                SinkConfig::Console(console) => {
                    if console.inputs.is_empty() {
                        return Err(anyhow!("Sink {} has no inputs configured", name));
                    }
                }
            }
        }

        // Validate pipeline connections
        self.validate_pipeline_connections()?;

        Ok(())
    }

    /// Validate that all pipeline connections are valid
    fn validate_pipeline_connections(&self) -> Result<()> {
        let mut available_outputs: Vec<String> = self.sources.keys().cloned().collect();

        // Validate transform inputs
        for (name, transform) in &self.transforms {
            match transform {
                TransformConfig::Remap(remap) => {
                    for input in &remap.inputs {
                        if !available_outputs.contains(input) {
                            return Err(anyhow!(
                                "Transform {} references unknown input {}",
                                name,
                                input
                            ));
                        }
                    }
                }
            }
            available_outputs.push(name.clone());
        }

        // Validate sink inputs
        for (name, sink) in &self.sinks {
            let inputs = match sink {
                SinkConfig::Http(http) => &http.inputs,
                SinkConfig::Console(console) => &console.inputs,
            };

            for input in inputs {
                if !available_outputs.contains(input) {
                    return Err(anyhow!("Sink {} references unknown input {}", name, input));
                }
            }
        }

        Ok(())
    }

    /// Apply environment variable overrides to the configuration
    fn apply_env_overrides(&mut self) {
        // Override channel capacity
        if let Ok(capacity) = env::var("DISPATCHER_CHANNEL_CAPACITY") {
            if let Ok(capacity) = capacity.parse() {
                self.channel_capacity = capacity;
                info!("Overriding channel capacity from environment: {}", capacity);
            }
        }

        // Override HTTP source configurations
        for (name, source) in &mut self.sources {
            #[allow(irrefutable_let_patterns)]
            if let SourceConfig::Http(http) = source {
                let prefix = format!("DISPATCHER_SOURCE_{}_", name.to_uppercase());
                
                if let Ok(address) = env::var(format!("{}ADDRESS", prefix)) {
                    http.address = address;
                    info!("Overriding source {} address from environment", name);
                }
                
                if let Ok(port) = env::var(format!("{}PORT", prefix)) {
                    if let Ok(port) = port.parse() {
                        http.port = Some(port);
                        info!("Overriding source {} port from environment", name);
                    }
                }
            }
        }

        // Override HTTP sink configurations
        for (name, sink) in &mut self.sinks {
            if let SinkConfig::Http(http) = sink {
                let prefix = format!("DISPATCHER_SINK_{}_", name.to_uppercase());
                
                if let Ok(uri) = env::var(format!("{}URI", prefix)) {
                    http.uri = uri;
                    info!("Overriding sink {} URI from environment", name);
                }
                
                if let Ok(timeout) = env::var(format!("{}TIMEOUT_MS", prefix)) {
                    if let Ok(timeout) = timeout.parse() {
                        http.timeout_ms = Some(timeout);
                        info!("Overriding sink {} timeout from environment", name);
                    }
                }
            }
        }
    }
}

// Configuration manager for hot reloading
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    path: String,
}

impl ConfigManager {
    pub fn new(path: String, config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            path,
        }
    }

    pub async fn get_config(&self) -> Arc<RwLock<Config>> {
        self.config.clone()
    }

    pub async fn reload(&self) -> Result<()> {
        let new_config = Config::load(&self.path)?;
        new_config.validate()?;
        
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Configuration reloaded successfully");
        
        Ok(())
    }

    pub async fn start_auto_reload(self: Arc<Self>) {
        let reload_interval = env::var("DISPATCHER_CONFIG_RELOAD_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30); // Default 30 seconds

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(reload_interval)).await;
                if let Err(e) = self.reload().await {
                    warn!("Failed to reload configuration: {}", e);
                }
            }
        });
    }
}
