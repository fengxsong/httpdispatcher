use crate::error::RuntimeError;
use anyhow::{anyhow, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer, FileIdMap};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

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
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| RuntimeError::ConfigError(format!("Failed to read config file: {}", e)))?;

        let config: Config = match Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "toml" => toml::from_str::<Config>(&content)
                .map_err(|e| RuntimeError::ConfigError(format!("Failed to parse TOML: {}", e)))?,
            "yaml" | "yml" => serde_yaml::from_str::<Config>(&content)
                .map_err(|e| RuntimeError::ConfigError(format!("Failed to parse YAML: {}", e)))?,
            "json" => serde_json::from_str::<Config>(&content)
                .map_err(|e| RuntimeError::ConfigError(format!("Failed to parse JSON: {}", e)))?,
            _ => {
                return Err(RuntimeError::ConfigError(format!(
                    "Unknown config file format: {}",
                    path.display()
                )))
            }
        };

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
        let available_outputs: Vec<String> = self
            .sources
            .keys()
            .cloned()
            .chain(self.transforms.keys().cloned())
            .collect();

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
}

// Configuration manager for hot reloading
#[derive(Debug)]
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    config_path: PathBuf,
    reload_tx: Option<broadcast::Sender<()>>,
    watcher: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
}

impl ConfigManager {
    pub async fn new(config_path: impl AsRef<Path>) -> Result<Arc<Self>, RuntimeError> {
        let config_path = config_path.as_ref().canonicalize().map_err(|e| {
            RuntimeError::ConfigError(format!("Failed to get absolute path: {}", e))
        })?;

        let config = Config::load(&config_path).await?;
        let (reload_tx, _) = broadcast::channel(1);

        Ok(Arc::new(Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
            reload_tx: Some(reload_tx),
            watcher: None,
        }))
    }

    pub fn get_config(&self) -> Arc<RwLock<Config>> {
        self.config.clone()
    }

    pub fn subscribe_to_reload(&self) -> Option<broadcast::Receiver<()>> {
        self.reload_tx.as_ref().map(|tx| tx.subscribe())
    }

    pub async fn reload(&self) -> Result<(), RuntimeError> {
        let new_config = Config::load(&self.config_path).await?;
        let mut config = self.config.write().await;
        *config = new_config;

        if let Some(tx) = &self.reload_tx {
            let _ = tx.send(());
        }

        Ok(())
    }

    pub async fn start_file_watcher(self: &Arc<Self>) -> Result<(), RuntimeError> {
        let (tx, mut rx) = mpsc::channel(1);
        let config_path = self.config_path.clone();
        let manager = self.clone();

        let watch_path = self.config_path.parent().ok_or_else(|| {
            RuntimeError::ConfigError("Failed to get parent directory".to_string())
        })?;

        info!(
            "Starting file watcher for absolute path: {}",
            watch_path.display()
        );
        debug!("Watching config file: {}", config_path.display());

        let mut debouncer = new_debouncer(
            Duration::from_millis(100),
            None,
            move |res: std::result::Result<Vec<DebouncedEvent>, _>| match res {
                Ok(events) => {
                    for event in events {
                        debug!("Received file system event: {:?}", event);
                        if event.event.paths.iter().any(|p| p == &config_path) {
                            info!("Detected change in config file: {}", config_path.display());
                            if let Err(e) = tx.blocking_send(()) {
                                error!("Failed to send reload signal: {}", e);
                            }
                        }
                    }
                }
                Err(e) => error!("Watch error: {:?}", e),
            },
        )
        .map_err(|e| RuntimeError::Other(format!("Failed to create watcher: {}", e)))?;

        debouncer
            .watcher()
            .configure(
                notify::Config::default()
                    .with_poll_interval(Duration::from_secs(1))
                    .with_compare_contents(true),
            )
            .map_err(|e| RuntimeError::Other(format!("Failed to configure watcher: {}", e)))?;

        debouncer
            .watcher()
            .watch(watch_path, RecursiveMode::NonRecursive)
            .map_err(|e| RuntimeError::Other(format!("Failed to watch config file: {}", e)))?;

        let mut this = self.clone();
        if let Some(this) = Arc::get_mut(&mut this) {
            this.watcher = Some(debouncer);
        }

        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                debug!("Config file change detected, reloading...");
                match manager.reload().await {
                    Ok(()) => info!("Configuration reloaded successfully"),
                    Err(e) => error!("Failed to reload configuration: {}", e),
                }
            }
        });

        Ok(())
    }

    pub async fn stop_file_watcher(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }
    }
}

impl Drop for ConfigManager {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }
    }
}
