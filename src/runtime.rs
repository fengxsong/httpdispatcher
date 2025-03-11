use crate::component::{Event, Sink, Source, Transform};
use crate::config::ConfigManager;
use crate::error::{Result, RuntimeError};
use dashmap::DashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

#[derive(Debug)]
pub struct Runtime {
    config_manager: Arc<ConfigManager>,
    tx: DashMap<String, broadcast::Sender<Event>>,
    rx: DashMap<String, broadcast::Receiver<Event>>,
    components: DashMap<String, ComponentRef>,
}

#[derive(Clone)]
enum ComponentRef {
    Source(Arc<Mutex<Box<dyn Source>>>),
    Transform(Arc<Mutex<Box<dyn Transform>>>),
    Sink(Arc<Mutex<Box<dyn Sink>>>),
}

impl std::fmt::Debug for ComponentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentRef::Source(source) => {
                write!(f, "Source({})", source.try_lock().unwrap().name())
            }
            ComponentRef::Transform(transform) => {
                write!(f, "Transform({})", transform.try_lock().unwrap().name())
            }
            ComponentRef::Sink(sink) => write!(f, "Sink({})", sink.try_lock().unwrap().name()),
        }
    }
}

impl Runtime {
    pub async fn build(config_path: impl AsRef<Path>) -> Result<Self> {
        let config_manager = ConfigManager::new(config_path).await?;

        let runtime = Runtime {
            config_manager: config_manager.clone(),
            components: DashMap::new(),
            tx: DashMap::new(),
            rx: DashMap::new(),
        };

        // Initialize components
        runtime.initialize_components().await?;

        // Start configuration file watcher if enabled
        if std::env::var("DISPATCHER_ENABLE_AUTO_RELOAD").is_ok() {
            if let Some(mut reload_rx) = config_manager.subscribe_to_reload() {
                let runtime = Arc::new(runtime);
                let runtime_weak = Arc::downgrade(&runtime);

                tokio::spawn(async move {
                    while reload_rx.recv().await.is_ok() {
                        if let Some(runtime) = runtime_weak.upgrade() {
                            match runtime.reload().await {
                                Ok(()) => info!("Runtime reloaded successfully"),
                                Err(e) => error!("Failed to reload runtime: {}", e),
                            }
                        }
                    }
                });

                config_manager.start_file_watcher().await?;
                Ok(Arc::try_unwrap(runtime).expect("Runtime still has references"))
            } else {
                error!("Failed to subscribe to config reload notifications");
                Ok(runtime)
            }
        } else {
            Ok(runtime)
        }
    }

    async fn initialize_components(&self) -> Result<()> {
        let config = self.config_manager.get_config();
        let config = config.read().await;

        // Initialize channels
        config
            .sources
            .keys()
            .chain(config.transforms.keys())
            .for_each(|name| {
                let (sender, receiver) = broadcast::channel(config.channel_capacity);
                self.tx.insert(name.to_string(), sender);
                self.rx.insert(name.to_string(), receiver);
            });

        // Create sources
        for (name, source_config) in &config.sources {
            let source = crate::sources::create_source(name.to_string(), source_config.clone())
                .map_err(|e| {
                    RuntimeError::init_error(format!("Failed to create source {}: {}", name, e))
                })?;
            self.components.insert(
                name.to_string(),
                ComponentRef::Source(Arc::new(Mutex::new(source))),
            );
        }

        // Create transforms
        for (name, transform_config) in &config.transforms {
            let transform =
                crate::transforms::create_transform(name.to_string(), transform_config.clone())
                    .map_err(|e| {
                        RuntimeError::init_error(format!(
                            "Failed to create transform {}: {}",
                            name, e
                        ))
                    })?;
            self.components.insert(
                name.to_string(),
                ComponentRef::Transform(Arc::new(Mutex::new(transform))),
            );
        }

        // Create sinks
        for (name, sink_config) in &config.sinks {
            let sink =
                crate::sinks::create_sink(name.to_string(), sink_config.clone()).map_err(|e| {
                    RuntimeError::init_error(format!("Failed to create sink {}: {}", name, e))
                })?;
            self.components.insert(
                name.to_string(),
                ComponentRef::Sink(Arc::new(Mutex::new(sink))),
            );
        }

        Runtime::connect_pipelines(self).await?;

        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        self.stop_all_components().await?;
        self.components.clear();
        self.tx.clear();
        self.rx.clear();

        self.initialize_components().await?;

        info!("Runtime configuration reloaded successfully");
        Ok(())
    }

    async fn stop_all_components(&self) -> Result<()> {
        for pair in self.components.iter() {
            if let ComponentRef::Source(source) = pair.value() {
                let mut source = source.lock().await;
                if let Err(e) = source.shutdown().await {
                    error!("Error stopping source {}: {}", pair.key(), e);
                }
            }
        }

        // 等待所有处理中的事件完成
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        Ok(())
    }

    async fn connect_pipelines(runtime: &Runtime) -> Result<()> {
        // Store all spawned tasks
        let mut tasks = Vec::new();

        for pair in runtime.components.iter() {
            let name = pair.key().clone();
            let component = pair.value().clone();

            match component {
                ComponentRef::Transform(transform) => {
                    let transform = transform.clone();

                    // Get inputs before spawning tasks
                    let inputs = {
                        let guard = transform.lock().await;
                        guard.inputs().to_vec()
                    };

                    for input in inputs {
                        if !runtime.components.contains_key(&input) {
                            return Err(RuntimeError::invalid_input(
                                name.clone(),
                                format!("Input {} not found", input),
                            ));
                        }

                        let tx = {
                            let sender = runtime.tx.get(&name).ok_or_else(|| {
                                RuntimeError::channel_error(format!(
                                    "Channel not found for {}",
                                    name
                                ))
                            })?;
                            sender.value().clone()
                        };

                        let mut input_rx = {
                            let receiver = runtime.rx.get(&input).ok_or_else(|| {
                                RuntimeError::channel_error(format!(
                                    "Channel not found for input {}",
                                    input
                                ))
                            })?;
                            receiver.value().resubscribe()
                        };

                        let transform = transform.clone();

                        // Log connection
                        {
                            let guard = transform.lock().await;
                            info!("Connecting transform {} to input {}", guard.name(), input);
                        }

                        let task = tokio::spawn(async move {
                            while let Ok(event) = input_rx.recv().await {
                                let processed = {
                                    let guard = transform.lock().await;
                                    guard.transform(&event).await
                                };

                                match processed {
                                    Ok(processed_event) => {
                                        let _ = tx.send(processed_event);
                                    }
                                    Err(e) => {
                                        let guard = transform.lock().await;
                                        error!(
                                            "Transform {} failed to process event: {:?}, err: {}",
                                            guard.name(),
                                            event.id,
                                            e
                                        );
                                    }
                                }
                            }
                        });

                        tasks.push(task);
                    }
                }
                ComponentRef::Sink(sink) => {
                    let sink = sink.clone();

                    // Get inputs before spawning tasks
                    let inputs = {
                        let guard = sink.lock().await;
                        guard.inputs().to_vec()
                    };

                    for input in inputs {
                        if !runtime.components.contains_key(&input) {
                            return Err(RuntimeError::invalid_input(
                                name.clone(),
                                format!("Input {} not found", input),
                            ));
                        }

                        let mut input_rx = {
                            let receiver = runtime.rx.get(&input).ok_or_else(|| {
                                RuntimeError::channel_error(format!(
                                    "Channel not found for input {}",
                                    input
                                ))
                            })?;
                            receiver.value().resubscribe()
                        };

                        let sink = sink.clone();

                        // Log connection
                        {
                            let guard = sink.lock().await;
                            info!("Connecting sink {} to input {}", guard.name(), input);
                        }

                        let task = tokio::spawn(async move {
                            while let Ok(event) = input_rx.recv().await {
                                let processed = {
                                    let mut guard = sink.lock().await;
                                    guard.process(&event).await
                                };

                                if let Err(e) = processed {
                                    let guard = sink.lock().await;
                                    error!(
                                        "Sink {} failed to process event: {:?}, err: {}",
                                        guard.name(),
                                        event.id,
                                        e
                                    );
                                }
                            }
                        });

                        tasks.push(task);
                    }
                }
                ComponentRef::Source(source) => {
                    info!(
                        "source {} don't need to be connected",
                        source.lock().await.name()
                    );
                } // Sources don't need to be connected
            }
        }

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        let mut tasks = Vec::new();

        // Start sources
        for pair in self.components.iter() {
            if let ComponentRef::Source(source) = pair.value() {
                let source = source.clone();
                let name = pair.key().clone();

                let tx = self
                    .tx
                    .get(&name)
                    .ok_or_else(|| {
                        RuntimeError::channel_error(format!("Channel not found for {}", name))
                    })?
                    .value()
                    .clone();

                let task = tokio::spawn(async move {
                    let mut guard = source.lock().await;
                    if let Err(e) = guard.run(tx).await {
                        error!("Source {} failed: {}", name, e);
                    }
                });

                tasks.push(task);
            }
        }

        // Wait for all tasks to complete
        for task in tasks {
            task.await.map_err(|e| RuntimeError::Other(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        // Stop file watcher if running
        if std::env::var("DISPATCHER_ENABLE_AUTO_RELOAD").is_ok() {
            if let Some(config_manager) = Arc::get_mut(&mut self.config_manager) {
                config_manager.stop_file_watcher().await;
            }
        }

        // Clear all components and channels
        self.components.clear();
        self.tx.clear();
        self.rx.clear();

        info!("Runtime shut down successfully");
        Ok(())
    }
}
